//! 编辑器会话。两种模式，而且差别必须在界面上看得见：
//!
//! - `Memory`：小于阈值的文件，整份进内存，可编辑、可撤销。
//! - `Huge`：超过阈值的文件，**只读浏览**（只建行索引 + 按需读窗口），
//!   搜索与替换走磁盘上的流式路径。V1 故意不假装它可编辑：假装的代价是
//!   用户敲了一下键之后才发现改不了，而那时候他已经信任它了。
//!
//! **这一层不依赖任何 GUI。** 它本来就在 crates/app 里且一行 egui 都没用，
//! 而快闸门故意不编 GUI，于是这堆承重逻辑一条断言都没有。搬出来之后它进了快闸门。
#![forbid(unsafe_code)]

pub mod browser;
pub mod jump;
pub mod status;

use std::io;
use std::path::{Path, PathBuf};

use yi_edit_core::{
    highlight_line, lang_from_path, Doc, Eol, Lang, LineIndex, LineState, Pos, SearchOptions,
    HUGE_FILE_THRESHOLD, MAX_PATTERN_LEN,
};
use yi_edit_fileio as fio;

pub use status::StatusBar;

/// 搜索结果上限。到顶之后必须报 truncated，不能静默截断。
pub const MAX_HITS: usize = 5000;

/// 大文件模式下一次缓存多少行。
pub const WINDOW_LINES: usize = 400;

/// 把列号夹到合法的字符边界。不做这一步的话，在中文行上切片直接 panic。
fn clamp_col(line: &str, col: usize) -> usize {
    let mut c = col.min(line.len());
    while c > 0 && !line.is_char_boundary(c) {
        c -= 1;
    }
    c
}

pub enum Source {
    Memory(Doc),
    Huge {
        path: PathBuf,
        index: LineIndex,
        /// 磁盘上的字节数。只读模式下状态栏用它；重算一遍要整文件扫一遗。
        bytes: usize,
        /// 缓存的可见窗口，避免每帧都去碰磁盘。
        cache_from: usize,
        cache_lines: Vec<String>,
    },
}

pub struct Editor {
    pub path: Option<PathBuf>,
    pub source: Source,
    pub lang: Lang,
    pub cursor: Pos,
    pub anchor: Option<Pos>,
    pub status: String,
    /// 高亮的跨行状态索引：states[i] 是第 i 行的**入口**状态。
    /// 只高亮可见行的代价就是得自己管这个；不管的话，从中间开始看的文件
    /// 会把块注释内的代码染成普通色。
    states: Vec<LineState>,
}

impl Editor {
    pub fn empty() -> Self {
        Self {
            path: None,
            source: Source::Memory(Doc::from_text(WELCOME)),
            lang: Lang::Markdown,
            cursor: Pos::default(),
            anchor: None,
            status: String::from("Yi Edit 已就绪"),
            states: Vec::new(),
        }
    }

    /// 打开一个文件。大于阈值走只读模式。
    pub fn open(path: &Path) -> io::Result<Self> {
        Self::open_with_threshold(path, HUGE_FILE_THRESHOLD)
    }

    /// 只为了可测：不能为了验一条只读路径就真写 64MB。
    /// `open` 仍用文档里那个常量，而且 crates/meta 里有一条断言钉着这一点 ——
    /// 否则这个参数就成了一个悤悤改掉真实阈值的后门。
    pub fn open_with_threshold(path: &Path, huge_threshold: u64) -> io::Result<Self> {
        let meta = fio::info(path)?;
        let lang = lang_from_path(&path.to_string_lossy());
        if meta.len > huge_threshold {
            let index = fio::index_lines(path)?;
            let mut e = Self {
                path: Some(path.to_path_buf()),
                source: Source::Huge {
                    path: path.to_path_buf(),
                    index,
                    bytes: meta.len as usize,
                    cache_from: 0,
                    cache_lines: Vec::new(),
                },
                lang,
                cursor: Pos::default(),
                anchor: None,
                status: String::new(),
                states: Vec::new(),
            };
            e.status = format!(
                "{} —— {:.1} MB，{} 行，大文件只读模式（搜索与替换走磁盘）",
                path.display(),
                meta.len as f64 / (1024.0 * 1024.0),
                e.line_count()
            );
            return Ok(e);
        }
        let bytes = fio::read_all(path)?;
        let doc = Doc::from_bytes_lossy(&bytes);
        let lines = doc.line_count();
        Ok(Self {
            path: Some(path.to_path_buf()),
            source: Source::Memory(doc),
            lang,
            cursor: Pos::default(),
            anchor: None,
            status: format!("{} —— {} 字节，{lines} 行", path.display(), bytes.len()),
            states: Vec::new(),
        })
    }

    pub fn is_huge(&self) -> bool {
        matches!(self.source, Source::Huge { .. })
    }

    pub fn doc(&self) -> Option<&Doc> {
        match &self.source {
            Source::Memory(d) => Some(d),
            Source::Huge { .. } => None,
        }
    }

    pub fn doc_mut(&mut self) -> Option<&mut Doc> {
        match &mut self.source {
            Source::Memory(d) => Some(d),
            Source::Huge { .. } => None,
        }
    }

    pub fn line_count(&self) -> usize {
        match &self.source {
            Source::Memory(d) => d.line_count(),
            Source::Huge { index, .. } => index.line_count(),
        }
    }

    /// 当前内容的字节数。内存模式下随编辑变（用户看的就是这个），
    /// 只读模式下取打开时的磁盘尺寸（重算要整文件扫一遗）。
    pub fn byte_len(&self) -> usize {
        match &self.source {
            Source::Memory(d) => d.to_text().len(),
            Source::Huge { bytes, .. } => *bytes,
        }
    }

    /// 底部状态栏的内容。放在会话层而不是 UI 里：它显示错一个数字不会报错，
    /// 而用户会信它。
    pub fn status_bar(&mut self) -> StatusBar {
        let cursor = self.cursor;
        let cursor_line = self.line(cursor.line);
        let column = status::char_column(&cursor_line, cursor.col);
        let selected_chars = self.selection().and_then(|(a, b)| {
            // 只有内存模式拿得到整份行集合；只读模式下不假装能算跨行选区字符数。
            self.doc()
                .map(|d| status::selected_chars(d.lines(), a, b))
        });
        StatusBar {
            name: self
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| String::from("未命名")),
            line: cursor.line + 1,
            column,
            total_lines: self.line_count(),
            total_bytes: self.byte_len(),
            selected_chars,
            eol: self.doc().map(|d| d.eol()).unwrap_or(Eol::Lf),
            lang: self.lang,
            read_only: self.is_huge(),
            dirty: self.doc().map(|d| d.is_dirty()).unwrap_or(false),
        }
    }

    /// 拉一行文本。大文件模式下可能要碰磁盘，所以需要 &mut self。
    pub fn line(&mut self, i: usize) -> String {
        match &mut self.source {
            Source::Memory(d) => d.line(i).to_string(),
            Source::Huge {
                path,
                index,
                cache_from,
                cache_lines,
                ..
            } => {
                if i < *cache_from || i >= *cache_from + cache_lines.len() {
                    let from = i.saturating_sub(WINDOW_LINES / 4);
                    let to = (from + WINDOW_LINES).min(index.line_count());
                    let start = index.line_span(from).map(|s| s.start).unwrap_or(0);
                    let end = index
                        .line_span(to.saturating_sub(1))
                        .map(|s| s.end)
                        .unwrap_or(start);
                    let bytes = fio::read_range(path, start as u64, end.saturating_sub(start))
                        .unwrap_or_default();
                    let text = String::from_utf8_lossy(&bytes);
                    *cache_from = from;
                    *cache_lines = text
                        .split('\n')
                        .map(|s| s.trim_end_matches('\r').to_string())
                        .collect();
                    // split 会在末尾多出一个空串（窗口末尾就是换行符），去掉它，
                    // 不去的话每个窗口结尾都会多一行空行。
                    if cache_lines.last().map(|s| s.is_empty()).unwrap_or(false) {
                        cache_lines.pop();
                    }
                }
                cache_lines
                    .get(i - *cache_from)
                    .cloned()
                    .unwrap_or_default()
            }
        }
    }

    /// 第 i 行的高亮入口状态。从已知的最近一行算到 i，结果缓存下来。
    pub fn state_at(&mut self, i: usize) -> LineState {
        if self.is_huge() {
            // 大文件模式不回溯跨行状态：那要从文件头扫到当前行。
            // 这是一条已知限制，写在 docs/PITFALLS.md 里，不假装它准。
            return LineState::default();
        }
        if self.states.is_empty() {
            self.states.push(LineState::default());
        }
        while self.states.len() <= i {
            let k = self.states.len() - 1;
            let text = self.line(k);
            let st = self.states[k];
            let (_, next) = highlight_line(&text, self.lang, st);
            self.states.push(next);
        }
        self.states[i]
    }

    pub fn invalidate_states(&mut self, from_line: usize) {
        self.states.truncate(from_line.max(1));
    }

    /// 已缓存的跨行状态数。只给测试用：否则「失效了」与「未失效」在行为上看不出区别。
    pub fn cached_state_count(&self) -> usize {
        self.states.len()
    }

    pub fn selection(&self) -> Option<(Pos, Pos)> {
        let a = self.anchor?;
        if a == self.cursor {
            return None;
        }
        Some(if a <= self.cursor {
            (a, self.cursor)
        } else {
            (self.cursor, a)
        })
    }

    /// 选区文本（复制用）。**故意不限只读模式**：从 64MB 只读文件里复制一段
    /// 是完全合法的需求，而它不改动任何东西。
    ///
    /// 与 `cut_selection()` 是两个独立路径，且必须逐字节一致 —— 否则「复制到的」
    /// 与「剪掉的」不一样，而那不会报错。tests/clipboard.rs 里有一条交叉校验。
    pub fn selected_text(&mut self) -> Option<String> {
        let (a, b) = self.selection()?;
        if a.line == b.line {
            let l = self.line(a.line);
            let from = clamp_col(&l, a.col);
            let to = clamp_col(&l, b.col);
            if to <= from {
                return None;
            }
            return Some(l[from..to].to_string());
        }
        let mut out = String::new();
        let first = self.line(a.line);
        out.push_str(&first[clamp_col(&first, a.col)..]);
        out.push('\n');
        for i in a.line + 1..b.line {
            out.push_str(&self.line(i));
            out.push('\n');
        }
        let last = self.line(b.line);
        out.push_str(&last[..clamp_col(&last, b.col)]);
        Some(out)
    }

    /// 剪切：返回被删的文本并真的删掉。只读模式返回 None **且不改动文件**。
    pub fn cut_selection(&mut self) -> Option<String> {
        let (a, b) = self.selection()?;
        if self.is_huge() {
            return None;
        }
        let text = self.doc_mut()?.delete(a, b);
        self.cursor = a;
        self.anchor = None;
        self.invalidate_states(a.line);
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    /// 插入文本（粘贴与键盘输入走同一条）。返回是否真的改动了内容。
    ///
    /// **走 `EditOp`**，所以粘贴可撤销、脏标记与高亮失效全部免费复用。
    /// 这也是后面接 AI 编辑的地基：AI 的改动必须走同一套算子，否则撤销会漏掉它们。
    pub fn insert_text(&mut self, text: &str) -> bool {
        if self.is_huge() || text.is_empty() {
            return false;
        }
        // 有选区先删。注意：这是**两个**算子，所以撤销要敲两下。
        // 已知行为，记在 docs/PITFALLS.md 里（撤销粒度那一步会一并收拾）。
        if self.selection().is_some() {
            self.cut_selection();
        }
        let at = self.cursor;
        let Some(d) = self.doc_mut() else {
            return false;
        };
        let end = d.insert(at, text);
        self.cursor = end;
        self.anchor = None;
        self.invalidate_states(at.line);
        true
    }

    /// 全选。只读模式下也允许（全选 + 复制是合法的）。
    pub fn select_all(&mut self) {
        let last = self.line_count().saturating_sub(1);
        let len = self.line(last).len();
        self.anchor = Some(Pos::new(0, 0));
        self.cursor = Pos::new(last, len);
    }

    pub fn save(&mut self) -> io::Result<()> {
        let Some(path) = self.path.clone() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "还没有文件名，先在上方输入路径",
            ));
        };
        match &mut self.source {
            Source::Memory(d) => {
                let text = d.to_text();
                fio::save_atomic(&path, text.as_bytes())?;
                d.mark_saved();
                self.status = format!("已保存 {} （{} 字节）", path.display(), text.len());
                Ok(())
            }
            Source::Huge { .. } => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "大文件为只读模式，没有未保存的修改",
            )),
        }
    }

    /// 有没有未保存的修改。关窗拦截靠它，而不是在 UI 里自己猜。
    pub fn is_dirty(&self) -> bool {
        self.doc().map(|d| d.is_dirty()).unwrap_or(false)
    }

    /// 搜索。返回 (命中位置, 是否到达上限)。
    pub fn search(&mut self, needle: &str, opts: SearchOptions) -> (Vec<Pos>, bool) {
        if needle.is_empty() {
            return (Vec::new(), false);
        }
        if needle.len() > MAX_PATTERN_LEN {
            self.status = format!(
                "搜索内容太长：{} 字节，上限 {MAX_PATTERN_LEN}",
                needle.len()
            );
            return (Vec::new(), true);
        }
        match &self.source {
            Source::Memory(d) => {
                let hits = d.find_all(needle, opts);
                let truncated = hits.len() > MAX_HITS;
                (
                    hits.into_iter().take(MAX_HITS).map(|(a, _)| a).collect(),
                    truncated,
                )
            }
            Source::Huge { path, index, .. } => {
                match fio::find_offsets(path, needle.as_bytes(), opts, MAX_HITS) {
                    Ok((offsets, truncated)) => {
                        let out = offsets
                            .into_iter()
                            .map(|off| {
                                let line = index.line_of_offset(off as usize);
                                let start = index.line_span(line).map(|s| s.start).unwrap_or(0);
                                Pos::new(line, off as usize - start)
                            })
                            .collect();
                        (out, truncated)
                    }
                    Err(e) => {
                        self.status = format!("搜索失败：{e}");
                        (Vec::new(), true)
                    }
                }
            }
        }
    }

    /// 全部替换。大文件走磁盘上的流式路径，完事重建索引。
    pub fn replace_all(
        &mut self,
        needle: &str,
        repl: &str,
        opts: SearchOptions,
    ) -> io::Result<usize> {
        if needle.is_empty() {
            return Ok(0);
        }
        match &mut self.source {
            Source::Memory(d) => {
                let n = d.replace_all(needle, repl, opts);
                self.states.clear();
                Ok(n)
            }
            Source::Huge { path, .. } => {
                let p = path.clone();
                let n = fio::replace_in_place(&p, needle.as_bytes(), repl.as_bytes(), opts)?;
                let index = fio::index_lines(&p)?;
                let bytes = fio::info(&p)?.len as usize;
                self.source = Source::Huge {
                    path: p,
                    index,
                    bytes,
                    cache_from: 0,
                    cache_lines: Vec::new(),
                };
                Ok(n)
            }
        }
    }

    pub fn clamp(&self, p: Pos) -> Pos {
        match &self.source {
            Source::Memory(d) => d.clamp(p),
            Source::Huge { index, .. } => Pos::new(p.line.min(index.line_count() - 1), p.col),
        }
    }

    /// 向前一个**字符**（不是一个字节）。按字节走的话一敲左箭头就能把中文切开，
    /// 而切开之后的表现是 panic。这两个本来卡在 UI 层里，永远验不到。
    pub fn prev_pos(&mut self, p: Pos) -> Pos {
        if p.col > 0 {
            let line = self.line(p.line);
            let mut c = (p.col - 1).min(line.len());
            while c > 0 && !line.is_char_boundary(c) {
                c -= 1;
            }
            return Pos::new(p.line, c);
        }
        if p.line == 0 {
            return p;
        }
        let prev_len = self.line(p.line - 1).len();
        Pos::new(p.line - 1, prev_len)
    }

    pub fn next_pos(&mut self, p: Pos) -> Pos {
        let line = self.line(p.line);
        if p.col < line.len() {
            let mut c = p.col + 1;
            while c < line.len() && !line.is_char_boundary(c) {
                c += 1;
            }
            return Pos::new(p.line, c);
        }
        if p.line + 1 >= self.line_count() {
            return p;
        }
        Pos::new(p.line + 1, 0)
    }
}

pub const WELCOME: &str = "# Yi Edit

极简跨平台代码编辑器。快捷键：

Ctrl+O  打开输入框里的路径
Ctrl+S  保存（写临时文件再 rename，不会把原文件写成半截）
Ctrl+F  定位到查找框
Ctrl+A  全选
Ctrl+C / Ctrl+X / Ctrl+V  复制 / 剪切 / 粘贴
Ctrl+Z / Ctrl+Y  撤销 / 重做
Ctrl+B  开关侧栏
Enter / Shift+Enter  下一个 / 上一个匹配

超过 64 MB 的文件会以只读模式打开：只建行索引、按需读可见行，
搜索与替换在磁盘上流式完成，不把文件整份拉进内存。只读模式下仍可以选中并复制。
";
