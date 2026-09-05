//! 文档模型：按行存的可变文本 + 算子栈式撤销。
//!
//! 为什么按行存而不是一个大 String：在一个几十 MB 的 String 中间插一个字符是
//! O(n) 的内存搬动，每敲一下键搬一遍。按行存之后，行内编辑只动那一行。
//!
//! 撤销不存快照存**算子**：快照在大文件上是災难，而算子可逆。
pub use crate::consts::MAX_UNDO;
use crate::search::{self, SearchOptions};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    Lf,
    Crlf,
}

impl Default for Eol {
    fn default() -> Self {
        Eol::Lf
    }
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    Insert { at: Pos, end: Pos, text: String },
    Delete { from: Pos, to: Pos, text: String },
}

#[derive(Debug, Clone)]
pub struct Doc {
    lines: Vec<String>,
    undo: Vec<EditOp>,
    redo: Vec<EditOp>,
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

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    pub fn eol(&self) -> Eol {
        self.eol
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
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

    fn push_undo(&mut self, op: EditOp) {
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(op);
        self.redo.clear();
        self.dirty = true;
    }

    /// 插入，返回插入内容的末端位置。
    pub fn insert(&mut self, at: Pos, text: &str) -> Pos {
        let at = self.clamp(at);
        let end = self.apply_insert(at, text);
        if !text.is_empty() {
            self.push_undo(EditOp::Insert {
                at,
                end,
                text: text.to_string(),
            });
        }
        end
    }

    /// 删除 [from, to)，返回被删的文本。
    pub fn delete(&mut self, from: Pos, to: Pos) -> String {
        let (from, to) = order(self.clamp(from), self.clamp(to));
        let text = self.apply_delete(from, to);
        if !text.is_empty() {
            self.push_undo(EditOp::Delete {
                from,
                to,
                text: text.clone(),
            });
        }
        text
    }

    pub fn undo(&mut self) -> Option<Pos> {
        let op = self.undo.pop()?;
        let cursor = match &op {
            EditOp::Insert { at, end, .. } => {
                self.apply_delete(*at, *end);
                *at
            }
            EditOp::Delete { from, text, .. } => self.apply_insert(*from, text),
        };
        self.redo.push(op);
        self.dirty = true;
        Some(cursor)
    }

    pub fn redo(&mut self) -> Option<Pos> {
        let op = self.redo.pop()?;
        let cursor = match &op {
            EditOp::Insert { at, text, .. } => self.apply_insert(*at, text),
            EditOp::Delete { from, to, .. } => {
                self.apply_delete(*from, *to);
                *from
            }
        };
        self.undo.push(op);
        self.dirty = true;
        Some(cursor)
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
    pub fn replace_all(&mut self, needle: &str, repl: &str, opts: SearchOptions) -> usize {
        let hits = self.find_all(needle, opts);
        let mut n = 0usize;
        for (from, to) in hits.into_iter().rev() {
            self.apply_delete(from, to);
            let end = self.apply_insert(from, repl);
            self.push_undo(EditOp::Delete {
                from,
                to,
                text: needle.to_string(),
            });
            self.push_undo(EditOp::Insert {
                at: from,
                end,
                text: repl.to_string(),
            });
            n += 1;
        }
        n
    }
}
