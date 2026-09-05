//! 编辑器状态。两种模式，而且差别必须在界面上看得见：
//!
//! - `Memory`：小于阈值的文件，整份进内存，可编辑、可撤销。
//! - `Huge`：超过阈值的文件，**只读浏览**（只建行索引 + 按需读窗口），
//!   搜索与替换走磁盘上的流式路径。V1 故意不假装它可编辑：假装的代价是
//!   用户敲了一下键之后才发现改不了，而那时候他已经信任它了。

use std::io;
use std::path::{Path, PathBuf};

use yi_edit_core::{
    lang_from_path, Doc, Lang, LineIndex, LineState, Pos, SearchOptions, MAX_PATTERN_LEN,
};
use yi_edit_fileio as fio;

/// 搜索结果上限。到顶之后必须报 truncated，不能静默截断。
pub const MAX_HITS: usize = 5000;

pub enum Source {
    Memory(Doc),
    Huge {
        path: PathBuf,
        index: LineIndex,
        /// 缓存的可见窗口（行号区间 + 字节），避免每帧都去碰磁盘。
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
    /// 高亮的跨行状态索引：state_at[i] 是第 i 行的**入口**状态。
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

    pub fn open(path: &Path) -> io::Result<Self> {
        let info = fio::info(path)?;
        let lang = lang_from_path(&path.to_string_lossy());
        if info.is_huge {
            let index = fio::index_lines(path)?;
            let mut e = Self {
                path: Some(path.to_path_buf()),
                source: Source::Huge {
                    path: path.to_path_buf(),
                    index,
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
                info.len as f64 / (1024.0 * 1024.0),
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

    /// 拉一行文本。大文件模式下可能要碰磁盘，所以需要 &mut self。
    pub fn line(&mut self, i: usize) -> String {
        match &mut self.source {
            Source::Memory(d) => d.line(i).to_string(),
            Source::Huge {
                path,
                index,
                cache_from,
                cache_lines,
            } => {
                if i < *cache_from || i >= *cache_from + cache_lines.len() {
                    const WINDOW: usize = 400;
                    let from = i.saturating_sub(WINDOW / 4);
                    let to = (from + WINDOW).min(index.line_count());
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
            let (_, next) = yi_edit_core::highlight_line(&text, self.lang, st);
            self.states.push(next);
        }
        self.states[i]
    }

    pub fn invalidate_states(&mut self, from_line: usize) {
        self.states.truncate(from_line.max(1));
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

    /// 搜索。返回 (命中的行号与列, 是否到达上限)。
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
    pub fn replace_all(&mut self, needle: &str, repl: &str, opts: SearchOptions) -> io::Result<usize> {
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
                self.source = Source::Huge {
                    path: p,
                    index,
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
}

pub const WELCOME: &str = "# Yi Edit

极简跨平台代码编辑器。快捷键：

Ctrl+O  打开输入框里的路径
Ctrl+S  保存（写临时文件再 rename，不会把原文件写成半截）
Ctrl+F  定位到查找框
Ctrl+Z / Ctrl+Y  撤销 / 重做
Enter / Shift+Enter  下一个 / 上一个匹配

超过 64 MB 的文件会以只读模式打开：只建行索引、按需读可见行，
搜索与替换在磁盘上流式完成，不把文件整份拉进内存。
";
