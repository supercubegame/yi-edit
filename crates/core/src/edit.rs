//! 文档模型：按行存的可变文本 + 算子栈式撤销。
//!
//! 为什么按行存而不是一个大 String：在一个几十 MB 的 String 中间插一个字符是
//! O(n) 的内存搬动，每敲一下键搬一遍。按行存之后，行内编辑只动那一行。
//!
//! 撤销不存快照存**算子**：快照在大文件上是災难，而算子可逆。
//! 算子里存的必须是**真正被删掉的文本**，不是当时的搜索词 —— 大小写不敏感替换下
//! 两者不相等，而差异只在撤销之后才看得见。
//!
//! # 撤销粒度
//!
//! 撤销以**组**为单位，不是以单个算子为单位。否则用户为了撤销一个词要敲
//! 二十下 Ctrl+Z，而那不符合商业交付水准。
//!
//! 合并规则故意写成**不靠时间**的形式（“停顿超过 N 毫秒就分组”需要系统时间，
//! 而这个 crate 是纯核心 —— 纯度扫描器会直接红），而且每条都可断言：
//!
//! - 连续的**单字符非空白**插入合成一组，上限 [`MAX_GROUP_CHARS`]。
//! - 空白与换行自成一组并封口 —— 于是得到词级撤销。
//! - 连续退格与连续向后删除各自合成一组。
//! - 粘贴、[`Doc::replace_range`]（替换选区）、[`Doc::replace_all`] 各自一组：
//!   一下 Ctrl+Z 整个撤掉。
//! - 光标移动 / 保存 / 失焦由上层调 [`Doc::commit_undo_group`] 封口。
pub use crate::consts::MAX_UNDO;
use crate::search::{self, SearchOptions};

/// 一个撤销组最多合并多少个字符。
///
/// 不封顶的话，一口气敲一整段不带空格的东西（比如一串很长的中文）会变成
/// 一个巨大的撤销组，一下 Ctrl+Z 全没了。120 是拍的，但有两侧断言夹着：
/// 它不能小到连一个词都装不下，也不能大到形同无上限。
pub const MAX_GROUP_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Pos {
    pub line: usize,
    /// 行内**字节**偏移（不是字符序号）。clamp 会把它吐回字符边界。
    pub col: usize,
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Eol {
    #[default]
    Lf,
    Crlf,
}

impl Eol {
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Lf => "\n",
            Eol::Crlf => "\r\n",
        }
    }
}

/// 一次编辑。存的是**正向**操作，撤销时施加它的逆。
///
/// 故意只存「起点 + 文本」：终点能从这两者算出来（[`advance`]），而存两份的话
/// 合并两个算子之后终点会变，跟不上的那一份不会报错只会静默算错。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    Insert { at: Pos, text: String },
    Delete { from: Pos, text: String },
}

/// 当前组能跟什么合并。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Coalesce {
    /// 连续插入（敲字）。
    Typing,
    /// 连续删除（退格或向后删）。
    Deleting,
    /// 不合并：粘贴、替换、空白字符。
    Never,
}

#[derive(Debug, Clone)]
struct Group {
    ops: Vec<EditOp>,
    coalesce: Coalesce,
    /// 本组已经合并了多少个字符（只用于封顶判断）。
    chars: usize,
}

/// 从 `at` 开始插入 `text` 之后的位置。纯函数，不碰文档。
///
/// 插入与删除的终点都用它算，于是不存在「终点字段跟不上文本字段」这种分岜。
pub fn advance(at: Pos, text: &str) -> Pos {
    let mut parts = text.split('\n');
    let first = parts.next().unwrap_or("");
    let mut extra = 0usize;
    let mut last = first;
    for p in parts {
        extra += 1;
        last = p;
    }
    if extra == 0 {
        Pos::new(at.line, at.col + first.len())
    } else {
        Pos::new(at.line + extra, last.len())
    }
}

#[derive(Debug, Clone)]
pub struct Doc {
    lines: Vec<String>,
    undo: Vec<Group>,
    redo: Vec<Group>,
    /// 最后一组还能不能继续合并。封口之后下一次编辑一定开新组。
    open: bool,
    dirty: bool,
    eol: Eol,
}

impl Default for Doc {
    fn default() -> Self {
        Self::new()
    }
}

fn order(a: Pos, b: Pos) -> (Pos, Pos) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

impl Doc {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            undo: Vec::new(),
            redo: Vec::new(),
            open: false,
            dirty: false,
            eol: Eol::Lf,
        }
    }

    /// 从文本加载。CRLF 会被归一化成 LF，但原来的行尾风格记下来，
    /// 保存时写回去 —— 否则在 Windows 上打开再保存会把整个文件改成一条巨大的 diff。
    pub fn from_text(text: &str) -> Self {
        let eol = if text.contains("\r\n") {
            Eol::Crlf
        } else {
            Eol::Lf
        };
        let normalized = text.replace("\r\n", "\n");
        let lines: Vec<String> = normalized.split('\n').map(|s| s.to_string()).collect();
        Self {
            lines,
            undo: Vec::new(),
            redo: Vec::new(),
            open: false,
            dirty: false,
            eol,
        }
    }

    pub fn from_bytes_lossy(bytes: &[u8]) -> Self {
        Self::from_text(&String::from_utf8_lossy(bytes))
    }

    pub fn to_text(&self) -> String {
        self.lines.join(self.eol.as_str())
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn line(&self, i: usize) -> &str {
        self.lines.get(i).map(|s| s.as_str()).unwrap_or("")
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// 保存之后调。附带封口当前撤销组：保存是一个语义边界，
    /// 保存前后的输入不应该被一下 Ctrl+Z 一起撤掉。
    pub fn mark_saved(&mut self) {
        self.dirty = false;
        self.open = false;
    }

    pub fn eol(&self) -> Eol {
        self.eol
    }

    /// 撤销栈里有多少**组**（不是多少个算子）。
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// 封口当前撤销组。光标移动、失焦、保存、粘贴边界都该调。
    ///
    /// 不调的后果不是报错，而是「在另一处敲的字被归进上一个词的撤销组」，
    /// 而那一点只有用户敲 Ctrl+Z 时才发现。
    pub fn commit_undo_group(&mut self) {
        self.open = false;
    }

    /// 把位置吐到合法范围，并且向左对齐到字符边界。
    /// 不做这一步的话，在中文行上按一下方向键就能 panic。
    pub fn clamp(&self, p: Pos) -> Pos {
        let line = p.line.min(self.lines.len() - 1);
        let s = &self.lines[line];
        let mut col = p.col.min(s.len());
        while col > 0 && !s.is_char_boundary(col) {
            col -= 1;
        }
        Pos { line, col }
    }

    fn apply_insert(&mut self, at: Pos, text: &str) -> Pos {
        let at = self.clamp(at);
        if text.is_empty() {
            return at;
        }
        let parts: Vec<&str> = text.split('\n').collect();
        let tail = self.lines[at.line].split_off(at.col);
        self.lines[at.line].push_str(parts[0]);
        if parts.len() == 1 {
            let end = Pos {
                line: at.line,
                col: at.col + parts[0].len(),
            };
            self.lines[at.line].push_str(&tail);
            return end;
        }
        let last = parts[parts.len() - 1];
        let end = Pos {
            line: at.line + parts.len() - 1,
            col: last.len(),
        };
        let mut inserted: Vec<String> = Vec::with_capacity(parts.len() - 1);
        for p in &parts[1..parts.len() - 1] {
            inserted.push((*p).to_string());
        }
        inserted.push(format!("{last}{tail}"));
        for (k, l) in inserted.into_iter().enumerate() {
            self.lines.insert(at.line + 1 + k, l);
        }
        end
    }

    fn apply_delete(&mut self, from: Pos, to: Pos) -> String {
        let (from, to) = order(self.clamp(from), self.clamp(to));
        if from == to {
            return String::new();
        }
        if from.line == to.line {
            let removed = self.lines[from.line][from.col..to.col].to_string();
            self.lines[from.line].replace_range(from.col..to.col, "");
            return removed;
        }
        let mut removed = String::new();
        removed.push_str(&self.lines[from.line][from.col..]);
        removed.push('\n');
        for l in &self.lines[from.line + 1..to.line] {
            removed.push_str(l);
            removed.push('\n');
        }
        removed.push_str(&self.lines[to.line][..to.col]);
        let tail = self.lines[to.line][to.col..].to_string();
        self.lines[from.line].truncate(from.col);
        self.lines[from.line].push_str(&tail);
        self.lines.drain(from.line + 1..=to.line);
        removed
    }

    /// 推一个新组。超过上限时丢最旧的一组（而不是拒绝新编辑）。
    fn push_group(&mut self, ops: Vec<EditOp>, coalesce: Coalesce, chars: usize) {
        if ops.is_empty() {
            return;
        }
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(Group {
            ops,
            coalesce,
            chars,
        });
        self.redo.clear();
        self.dirty = true;
        self.open = coalesce != Coalesce::Never;
    }

    /// 插入，返回插入内容的末端位置。
    ///
    /// 合并条件：当前组还开着、是敲字组、新内容是单个非空白字符、
    /// 插入点恰好接在上一次的末端、且本组未封顶。
    pub fn insert(&mut self, at: Pos, text: &str) -> Pos {
        let at = self.clamp(at);
        if text.is_empty() {
            return at;
        }
        let end = self.apply_insert(at, text);

        let single_char = text.chars().count() == 1;
        let is_space = text.chars().all(char::is_whitespace);
        // 空白与换行自成一组并封口 -> 词级撤销。
        // 多字符（粘贴 / IME 上屏）也自成一组：一下 Ctrl+Z 整个撤掉。
        if !single_char || is_space {
            self.push_group(
                vec![EditOp::Insert {
                    at,
                    text: text.to_string(),
                }],
                Coalesce::Never,
                0,
            );
            return end;
        }

        if self.open {
            if let Some(g) = self.undo.last_mut() {
                if g.coalesce == Coalesce::Typing && g.chars < MAX_GROUP_CHARS {
                    if let Some(EditOp::Insert {
                        at: gat,
                        text: gtext,
                    }) = g.ops.last_mut()
                    {
                        if advance(*gat, gtext) == at {
                            gtext.push_str(text);
                            g.chars += 1;
                            self.redo.clear();
                            self.dirty = true;
                            return end;
                        }
                    }
                }
            }
        }
        self.push_group(
            vec![EditOp::Insert {
                at,
                text: text.to_string(),
            }],
            Coalesce::Typing,
            1,
        );
        end
    }

    /// 删除 [from, to)，返回被删的文本。
    ///
    /// 两个方向各自合并：退格是往前拼，向后删是往后拼。
    pub fn delete(&mut self, from: Pos, to: Pos) -> String {
        let (from, to) = order(self.clamp(from), self.clamp(to));
        let text = self.apply_delete(from, to);
        if text.is_empty() {
            return text;
        }

        let single_char = text.chars().count() == 1;
        if single_char && self.open {
            if let Some(g) = self.undo.last_mut() {
                if g.coalesce == Coalesce::Deleting && g.chars < MAX_GROUP_CHARS {
                    if let Some(EditOp::Delete {
                        from: gfrom,
                        text: gtext,
                    }) = g.ops.last_mut()
                    {
                        // 退格：新删的范围紧接在旧范围之前。
                        if advance(from, &text) == *gfrom {
                            let merged = format!("{text}{gtext}");
                            *gfrom = from;
                            *gtext = merged;
                            g.chars += 1;
                            self.redo.clear();
                            self.dirty = true;
                            return text;
                        }
                        // 向后删：光标不动，新删的内容接在旧内容之后。
                        if from == *gfrom {
                            gtext.push_str(&text);
                            g.chars += 1;
                            self.redo.clear();
                            self.dirty = true;
                            return text;
                        }
                    }
                }
            }
        }
        self.push_group(
            vec![EditOp::Delete {
                from,
                text: text.clone(),
            }],
            if single_char {
                Coalesce::Deleting
            } else {
                Coalesce::Never
            },
            usize::from(single_char),
        );
        text
    }

    /// 把 [from, to) 换成 `text`，**两个算子装进同一组**。
    ///
    /// 为什么需要它：先 delete 再 insert 会得到两组，于是用户有选区时粘贴一段，
    /// 敲一下 Ctrl+Z 会停在一个**他从没见过的中间状态**（选区已删、新内容未插）。
    /// 那不会报错，只会让人以为编辑器坏了。
    pub fn replace_range(&mut self, from: Pos, to: Pos, text: &str) -> Pos {
        let (from, to) = order(self.clamp(from), self.clamp(to));
        let removed = self.apply_delete(from, to);
        let end = self.apply_insert(from, text);
        let mut ops: Vec<EditOp> = Vec::with_capacity(2);
        if !removed.is_empty() {
            ops.push(EditOp::Delete {
                from,
                text: removed,
            });
        }
        if !text.is_empty() {
            ops.push(EditOp::Insert {
                at: from,
                text: text.to_string(),
            });
        }
        self.push_group(ops, Coalesce::Never, 0);
        end
    }

    fn invert(&mut self, op: &EditOp) -> Pos {
        match op {
            EditOp::Insert { at, text } => {
                let end = advance(*at, text);
                self.apply_delete(*at, end);
                *at
            }
            EditOp::Delete { from, text } => self.apply_insert(*from, text),
        }
    }

    fn reapply(&mut self, op: &EditOp) -> Pos {
        match op {
            EditOp::Insert { at, text } => self.apply_insert(*at, text),
            EditOp::Delete { from, text } => {
                let end = advance(*from, text);
                self.apply_delete(*from, end);
                *from
            }
        }
    }

    /// 撤销一**组**。组内算子按相反顺序施加逆操作。
    pub fn undo(&mut self) -> Option<Pos> {
        let group = self.undo.pop()?;
        let mut cursor = None;
        for op in group.ops.iter().rev() {
            cursor = Some(self.invert(op));
        }
        self.redo.push(group);
        self.dirty = true;
        // 撤销之后一律封口：否则接着敲的字会被归进一个已经被撤销的组里。
        self.open = false;
        cursor
    }

    pub fn redo(&mut self) -> Option<Pos> {
        let group = self.redo.pop()?;
        let mut cursor = None;
        for op in group.ops.iter() {
            cursor = Some(self.reapply(op));
        }
        self.undo.push(group);
        self.dirty = true;
        self.open = false;
        cursor
    }

    /// 行内搜索。
    /// 已知限制（有断言在守，不是口头承诺）：含换行的模式在内存编辑器里一律返回零个匹配。
    /// 要跨行替换请走文件级的流式替换（crates/fileio）。
    pub fn find_all(&self, needle: &str, opts: SearchOptions) -> Vec<(Pos, Pos)> {
        let mut out = Vec::new();
        if needle.is_empty() || needle.contains('\n') {
            return out;
        }
        for (i, l) in self.lines.iter().enumerate() {
            for p in search::find_all(l.as_bytes(), needle.as_bytes(), opts) {
                out.push((
                    Pos { line: i, col: p },
                    Pos {
                        line: i,
                        col: p + needle.len(),
                    },
                ));
            }
        }
        out
    }

    /// 全部替换，返回替换次数。从后往前做，否则前面的替换会把后面的偏移全部废掉。
    ///
    /// **整个替换是一个撤销组**：一下 Ctrl+Z 全部撤掉，这才是用户预期。
    /// 每处替换一组的话，替了两千处就要敲四千下 Ctrl+Z。
    pub fn replace_all(&mut self, needle: &str, repl: &str, opts: SearchOptions) -> usize {
        let hits = self.find_all(needle, opts);
        if hits.is_empty() {
            return 0;
        }
        let mut ops: Vec<EditOp> = Vec::with_capacity(hits.len() * 2);
        let mut n = 0usize;
        for (from, to) in hits.into_iter().rev() {
            // 这里必须用 apply_delete 返回的真实文本：大小写不敏感时它与 needle 不相等，
            // 拿 needle 当撤销记录会把 Foo/FOO 全部还原成 foo，而且只在撤销后才看得见。
            let removed = self.apply_delete(from, to);
            self.apply_insert(from, repl);
            ops.push(EditOp::Delete {
                from,
                text: removed,
            });
            ops.push(EditOp::Insert {
                at: from,
                text: repl.to_string(),
            });
            n += 1;
        }
        self.push_group(ops, Coalesce::Never, 0);
        n
    }
}
