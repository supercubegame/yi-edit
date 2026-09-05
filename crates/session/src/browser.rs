//! 左侧文件面板的列举逻辑。碰文件系统，但不碰 UI。
//!
//! 两件事写成断言而不是口头约定：
//! - 排序是**确定的**（目录先、同类按名）。目录项的返回顺序本身不保证，
//!   而一个每次刷新都换顺序的文件面板比没有面板更难用。
//! - 隐藏文件默认不列，但不是静默丢掉：`Listing::hidden_skipped` 报出数量。

use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub dir: PathBuf,
    pub parent: Option<PathBuf>,
    pub entries: Vec<Entry>,
    /// 因为隐藏而没列出的数量。静默丢掉与「目录里本来就没有」不能长得一样。
    pub hidden_skipped: usize,
    /// 因为超过上限而没列出的数量。同理：截断必须报。
    pub truncated: usize,
}

/// 一次最多列多少条。目录里有两万个文件时面板不能卡死，但也不能静默截。
pub const MAX_ENTRIES: usize = 2000;

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// 列一个目录。`show_hidden` 为 false 时跳过点开头的条目并计数。
pub fn list_dir(dir: &Path, show_hidden: bool) -> io::Result<Listing> {
    let mut entries: Vec<Entry> = Vec::new();
    let mut hidden_skipped = 0usize;
    let mut seen = 0usize;
    let mut truncated = 0usize;

    for item in std::fs::read_dir(dir)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().to_string();
        if !show_hidden && is_hidden(&name) {
            hidden_skipped += 1;
            continue;
        }
        seen += 1;
        if entries.len() >= MAX_ENTRIES {
            truncated += 1;
            continue;
        }
        let is_dir = item.file_type().map(|t| t.is_dir()).unwrap_or(false);
        entries.push(Entry {
            name,
            path: item.path(),
            is_dir,
        });
    }
    debug_assert!(seen >= entries.len());

    // 确定排序：目录先，同类按名（忽略大小写，相同时再比原名保证全序）。
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });

    Ok(Listing {
        dir: dir.to_path_buf(),
        parent: dir.parent().map(|p| p.to_path_buf()),
        entries,
        hidden_skipped,
        truncated,
    })
}

/// 打开一个文件时面板应该停在哪个目录。
pub fn dir_for(path: &Path) -> Option<PathBuf> {
    path.parent().map(|p| p.to_path_buf())
}
