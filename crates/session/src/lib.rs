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
pub mod fontpick;
pub mod ime;
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

pub const MAX_HITS: usize = 5000;
pub const WINDOW_LINES: usize = 400;

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
        bytes: usize,
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

    pub fn open(path: &Path) -> io::Result<Self> {
        Self::open_with_threshold(path, HUGE_FILE_THRESHOLD)
    }

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

    pub fn is_huge(&self) -> bool { matches!(self.source, Source::Huge { .. }) }

    pub fn doc(&self) -> Option<&Doc> {
        match &self.source { Source::Memory(d) => Some(d), Source::Huge { .. } => None }
    }

    pub fn doc_mut(&mut self) -> Option<&mut Doc> {
        match &mut self.source { Source::Memory(d) => Some(d), Source::Huge { .. } => None }
    }

    pub fn line_count(&self) -> usize {
        match &self.source { Source::Memory(d) => d.line_count(), Source::Huge { index, .. } => index.line_count() }
    }

    pub fn byte_len(&self) -> usize {
        match &self.source { Source::Memory(d) => d.to_text().len(), Source::Huge { bytes, .. } => *bytes }
    }

    pub fn status_bar(&mut self) -> StatusBar {
        let cursor = self.cursor;
        let cursor_line = self.line(cursor.line);
        let column = status::char_column(&cursor_line, cursor.col);
        let selected_chars = self.selection().and_then(|(a, b)| self.doc().map(|d| status::selected_chars(d.lines(), a, b)));
        StatusBar {
            name: self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| String::from("未命名")),
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

    pub fn line(&mut self, i: usize) -> String {
        match &mut self.source {
            Source::Memory(d) => d.line(i).to_string(),
            Source::Huge { path, index, cache_from, cache_lines, .. } => {
                if i < *cache_from || i >= *cache_from + cache_lines.len() {
                    let from = i.saturating_sub(WINDOW_LINES / 4);
                    let to = (from + WINDOW_LINES).min(index.line_count());
                    let start = index.line_span(from).map(|s| s.start).unwrap_or(0);
                    let end = index.line_span(to.saturating_sub(1)).map(|s| s.end).unwrap_or(start);
                    let bytes = fio::read_range(path, start as u64, end.saturating_sub(start)).unwrap_or_default();
                    let text = String::from_utf8_lossy(&bytes);
                    *cache_from = from;
                    *cache_lines = text.split('\n').map(|s| s.trim_end_matches('\r').to_string()).collect();
                    if cache_lines.last().map(|s| s.is_empty()).unwrap_or(false) { cache_lines.pop(); }
                }
                cache_lines.get(i - *cache_from).cloned().unwrap_or_default()
            }
        }
    }

    pub fn state_at(&mut self, i: usize) -> LineState {
        if self.is_huge() { return LineState::default(); }
        if self.states.is_empty() { self.states.push(LineState::default()); }
        while self.states.len() <= i {
            let k = self.states.len() - 1;
            let text = self.line(k);
            let st = self.states[k];
            let (_, next) = highlight_line(&text, self.lang, st);
            self.states.push(next);
        }
        self.states[i]
    }

    pub fn invalidate_states(&mut self, from_line: usize) { self.states.truncate(from_line.max(1)); }
    pub fn cached_state_count(&self) -> usize { self.states.len() }

    pub fn selection(&self) -> Option<(Pos, Pos)> {
        let a = self.anchor?;
        if a == self.cursor { return None; }
        Some(if a <= self.cursor { (a, self.cursor) } else { (self.cursor, a) })
    }

    pub fn commit_undo_group(&mut self) {
        if let Some(d) = self.doc_mut() { d.commit_undo_group(); }
    }

    pub fn can_undo(&self) -> bool { self.doc().map(|d| d.can_undo()).unwrap_or(false) }
    pub fn can_redo(&self) -> bool { self.doc().map(|d| d.can_redo()).unwrap_or(false) }

    /// 撤销一**组**，并把光标放到被改动的地方。
    ///
    /// 为什么不让 UI 直接调 `doc_mut().undo()`：那样光标与高亮缓存的失效就散在 GUI 里，
    /// 而快闸门碰不到 GUI。忘了失效的后果不是报错，是撤销之后高亮颜色停在旧状态上。
    pub fn undo(&mut self) -> Option<Pos> {
        let pos = self.doc_mut()?.undo()?;
        self.cursor = pos;
        self.anchor = None;
        self.invalidate_states(pos.line);
        Some(pos)
    }

    pub fn redo(&mut self) -> Option<Pos> {
        let pos = self.doc_mut()?.redo()?;
        self.cursor = pos;
        self.anchor = None;
        self.invalidate_states(pos.line);
        Some(pos)
    }

    pub fn selected_text(&mut self) -> Option<String> {
        let (a, b) = self.selection()?;
        if a.line == b.line {
            let l = self.line(a.line);
            let from = clamp_col(&l, a.col);
            let to = clamp_col(&l, b.col);
            if to <= from { return None; }
            return Some(l[from..to].to_string());
        }
        let mut out = String::new();
        let first = self.line(a.line);
        out.push_str(&first[clamp_col(&first, a.col)..]);
        out.push('\n');
        for i in a.line + 1..b.line { out.push_str(&self.line(i)); out.push('\n'); }
        let last = self.line(b.line);
        out.push_str(&last[..clamp_col(&last, b.col)]);
        Some(out)
    }

    pub fn cut_selection(&mut self) -> Option<String> {
        let (a, b) = self.selection()?;
        if self.is_huge() { return None; }
        let text = self.doc_mut()?.delete(a, b);
        self.cursor = a;
        self.anchor = None;
        self.invalidate_states(a.line);
        if text.is_empty() { None } else { Some(text) }
    }

    pub fn insert_text(&mut self, text: &str) -> bool {
        if self.is_huge() || text.is_empty() { return false; }
        let sel = self.selection();
        let at = sel.map(|(a, _)| a).unwrap_or(self.cursor);
        let Some(d) = self.doc_mut() else { return false; };
        let end = match sel { Some((a, b)) => d.replace_range(a, b, text), None => d.insert(at, text) };
        self.cursor = end;
        self.anchor = None;
        self.invalidate_states(at.line);
        true
    }

    pub fn select_all(&mut self) {
        let last = self.line_count().saturating_sub(1);
        let len = self.line(last).len();
        self.anchor = Some(Pos::new(0, 0));
        self.cursor = Pos::new(last, len);
    }

    pub fn save(&mut self) -> io::Result<()> {
        let Some(path) = self.path.clone() else { return Err(io::Error::new(io::ErrorKind::InvalidInput, "还没有文件名，先在上方输入路径")); };
        match &mut self.source {
            Source::Memory(d) => { let text = d.to_text(); fio::save_atomic(&path, text.as_bytes())?; d.mark_saved(); self.status = format!("已保存 {} （{} 字节）", path.display(), text.len()); Ok(()) }
            Source::Huge { .. } => Err(io::Error::new(io::ErrorKind::Unsupported, "大文件为只读模式，没有未保存的修改")),
        }
    }

    pub fn is_dirty(&self) -> bool { self.doc().map(|d| d.is_dirty()).unwrap_or(false) }

    pub fn search(&mut self, needle: &str, opts: SearchOptions) -> (Vec<Pos>, bool) {
        if needle.is_empty() { return (Vec::new(), false); }
        if needle.len() > MAX_PATTERN_LEN { self.status = format!("搜索内容太长：{} 字节，上限 {MAX_PATTERN_LEN}", needle.len()); return (Vec::new(), true); }
        match &self.source {
            Source::Memory(d) => { let hits = d.find_all(needle, opts); let truncated = hits.len() > MAX_HITS; (hits.into_iter().take(MAX_HITS).map(|(a, _)| a).collect(), truncated) }
            Source::Huge { path, index, .. } => match fio::find_offsets(path, needle.as_bytes(), opts, MAX_HITS) {
                Ok((offsets, truncated)) => { let out = offsets.into_iter().map(|off| { let line = index.line_of_offset(off as usize); let start = index.line_span(line).map(|s| s.start).unwrap_or(0); Pos::new(line, off as usize - start) }).collect(); (out, truncated) }
                Err(e) => { self.status = format!("搜索失败：{e}"); (Vec::new(), true) }
            },
        }
    }

    pub fn replace_all(&mut self, needle: &str, repl: &str, opts: SearchOptions) -> io::Result<usize> {
        if needle.is_empty() { return Ok(0); }
        match &mut self.source {
            Source::Memory(d) => { let n = d.replace_all(needle, repl, opts); self.states.clear(); Ok(n) }
            Source::Huge { path, .. } => { let p = path.clone(); let n = fio::replace_in_place(&p, needle.as_bytes(), repl.as_bytes(), opts)?; let index = fio::index_lines(&p)?; let bytes = fio::info(&p)?.len as usize; self.source = Source::Huge { path: p, index, bytes, cache_from: 0, cache_lines: Vec::new() }; Ok(n) }
        }
    }

    pub fn clamp(&self, p: Pos) -> Pos {
        match &self.source { Source::Memory(d) => d.clamp(p), Source::Huge { index, .. } => Pos::new(p.line.min(index.line_count() - 1), p.col) }
    }

    pub fn prev_pos(&mut self, p: Pos) -> Pos {
        if p.col > 0 { let line = self.line(p.line); let mut c = (p.col - 1).min(line.len()); while c > 0 && !line.is_char_boundary(c) { c -= 1; } return Pos::new(p.line, c); }
        if p.line == 0 { return p; }
        Pos::new(p.line - 1, self.line(p.line - 1).len())
    }

    pub fn next_pos(&mut self, p: Pos) -> Pos {
        let line = self.line(p.line);
        if p.col < line.len() { let mut c = p.col + 1; while c < line.len() && !line.is_char_boundary(c) { c += 1; } return Pos::new(p.line, c); }
        if p.line + 1 >= self.line_count() { return p; }
        Pos::new(p.line + 1, 0)
    }
}

pub const WELCOME: &str = "# Yi Edit

极简跳平台代码编辑器。快捷键：

Ctrl+O  打开输入框里的路径
Ctrl+S  保存（写临时文件再 rename，不会把原文件写成半截）
Ctrl+F  定位到查找框
Ctrl+A  全选
Ctrl+C / Ctrl+X / Ctrl+V  复制 / 剪切 / 粘贴
Ctrl+Z / Ctrl+Y  撤销 / 重做（按输入组，不是每个字符一步）
Ctrl+B  开关侧栏
Enter / Shift+Enter  下一个 / 上一个匹配

超过 64 MB 的文件会以只读模式打开：只建行索引、按需读可见行，
搜索与替换在磁盘上流式完成，不把文件整份拉进内存。只读模式下仍可以选中并复制。
";
