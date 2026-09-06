//! 底部状态与统计栏的内容。纯函数：同样的会话状态必然得到同样的栏。
//!
//! 为什么不直接在 UI 里拼字符串：状态栏是用户判断「我现在在哪 / 改了没」的主要依据。
//! 它显示错一个数字不会报错，而用户会信它。

use yi_edit_core::{Eol, Lang, Pos};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBar {
    /// 展示用的文件名（没文件时为无标题）。
    pub name: String,
    /// 光标行号，**从 1 开始**（界面上行号从 1 起，内部从 0 起，这一层转换很容易差一）。
    pub line: usize,
    /// 光标列号，按**字符**而不是字节，也从 1 开始。
    /// 按字节算的话，光标放在一个中文字后面会显示「第 4 列」，而那是错的。
    pub column: usize,
    pub total_lines: usize,
    pub total_bytes: usize,
    pub selected_chars: Option<usize>,
    pub eol: Eol,
    pub lang: Lang,
    pub read_only: bool,
    pub dirty: bool,
}

impl StatusBar {
    /// 行号与列号的文本。
    pub fn position_text(&self) -> String {
        format!("行 {} 列 {}", self.line, self.column)
    }

    /// 规模文本。字节数超过 1 MB 时转成 MB，否则保留精确字节数。
    pub fn size_text(&self) -> String {
        if self.total_bytes >= 1024 * 1024 {
            format!(
                "{} 行 · {:.1} MB",
                self.total_lines,
                self.total_bytes as f64 / (1024.0 * 1024.0)
            )
        } else {
            format!("{} 行 · {} 字节", self.total_lines, self.total_bytes)
        }
    }

    /// 右侧的模式标签。只读与未保存不能同时出现：只读模式下根本没有未保存的修改。
    pub fn badges(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.read_only {
            out.push(String::from("只读"));
        } else if self.dirty {
            out.push(String::from("未保存"));
        }
        out.push(String::from(match self.eol {
            Eol::Lf => "LF",
            Eol::Crlf => "CRLF",
        }));
        out.push(self.lang.name().to_uppercase());
        if let Some(n) = self.selected_chars {
            out.push(format!("已选 {n} 字符"));
        }
        out
    }
}

/// 把字节列转成字符列（从 1 开始）。越过行尾或落在非字符边界时夹到合法位置。
pub fn char_column(line: &str, byte_col: usize) -> usize {
    let mut c = byte_col.min(line.len());
    while c > 0 && !line.is_char_boundary(c) {
        c -= 1;
    }
    line[..c].chars().count() + 1
}

/// 选区里的字符数。跨行时每个换行符计一个字符。
pub fn selected_chars(lines: &[String], from: Pos, to: Pos) -> usize {
    if from == to {
        return 0;
    }
    let (from, to) = if from <= to { (from, to) } else { (to, from) };
    if from.line == to.line {
        let l = lines.get(from.line).map(|s| s.as_str()).unwrap_or("");
        let a = char_column(l, from.col);
        let b = char_column(l, to.col);
        return b.saturating_sub(a);
    }
    let mut n = 0usize;
    let first = lines.get(from.line).map(|s| s.as_str()).unwrap_or("");
    n += first.chars().count() + 1 - (char_column(first, from.col) - 1);
    for i in from.line + 1..to.line {
        let l = lines.get(i).map(|s| s.as_str()).unwrap_or("");
        n += l.chars().count() + 1;
    }
    let last = lines.get(to.line).map(|s| s.as_str()).unwrap_or("");
    n += char_column(last, to.col) - 1;
    n
}
