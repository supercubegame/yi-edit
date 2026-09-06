//! 长文本缩略（中间省略号）。纯函数，不碰 I/O。
//!
//! 为什么这一小块值得单独抽出来：它有一个**不报错就不会被发现**的形状，
//! 和一个**报错得很难看**的形状：
//!
//! - 不报错的：缩略完之后仍然比上限长。侧栏照样被挤压，而没任何东西会喊。
//! - 难看的：按**字节**切。Windows 路径里很容易出现中文目录名，
//!   而在多字节字符中间切一刀直接 panic —— 编辑器会在用户点开一个目录时当场退出。
//!
//! 所以这里一律按**字符**算，而且有一条模糊断言扫过所有上限值（包括 0 与 1）。

/// 省略号。它自己就是一个多字节字符，所以下面一律不能用字节长度去算。
pub const ELLIPSIS: char = '…';

/// 把 `text` 缩到最多 `max_chars` 个**字符**，多余部分用中间省略号代替。
///
/// 装得下时原样返回（**不**加省略号）。这一侧与另一侧同样重要：
/// 只验「太长时真的缩了」的话，一个无条件加省略号的实现也能完美交差。
pub fn elide_middle(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return ELLIPSIS.to_string();
    }
    // 省略号占一个字符位；剩下的头尾均分，奇数时头部多拿一个
    // （路径的开头比中间更能帮人定位：盘符与顶层目录）。
    let keep = max_chars - 1;
    let head = keep - keep / 2;
    let tail = keep - head;
    let mut out = String::new();
    out.extend(chars[..head].iter());
    out.push(ELLIPSIS);
    out.extend(chars[chars.len() - tail..].iter());
    out
}

/// 路径里最后一段（最后一个 `/` 或 `\` 之后）。末尾就是分隔符时返回 None。
pub fn last_segment(path: &str) -> Option<&str> {
    let after = path
        .char_indices()
        .filter(|(_, c)| *c == '/' || *c == '\\')
        .map(|(i, c)| i + c.len_utf8())
        .next_back()?;
    let seg = &path[after..];
    if seg.is_empty() {
        None
    } else {
        Some(seg)
    }
}

/// 路径专用缩略：**优先保住最后一段完整**。
///
/// 为什么不直接用中间省略：对一个在看文件列表的人来说，最有用的是末段
/// （当前目录名），而中间省略会把末段也切掉一半。末段本身都装不下时
/// 退回中间省略（而不是超出上限：超出上限才是那个不报错的 bug）。
pub fn elide_path(path: &str, max_chars: usize) -> String {
    if path.chars().count() <= max_chars {
        return path.to_string();
    }
    if let Some(tail) = last_segment(path) {
        let tail_len = tail.chars().count();
        // 除了末段还需要：1 个省略号 + 至少 1 个头部字符。否则不如中间省略。
        if tail_len + 2 <= max_chars {
            let head_len = max_chars - tail_len - 1;
            let head: String = path.chars().take(head_len).collect();
            return format!("{head}{ELLIPSIS}{tail}");
        }
    }
    elide_middle(path, max_chars)
}

/// 根据可用像素宽与字号估算一行能装多少个字符。
///
/// **这是估算，不是排版。** 等宽字体下 ASCII 字符宽约为字号的 0.6，
/// 而中文是它的两倍——所以这个数对纯中文路径会偏大。它只用来防「挤压成一坨」，
/// 不用来保证像素级对齐；真正的宽度由 GUI 自己排。
pub fn fit_chars(available_px: f32, font_size: f32) -> usize {
    if available_px <= 0.0 || font_size <= 0.0 {
        return 0;
    }
    let per_char = font_size * 0.6;
    (available_px / per_char).floor().max(0.0) as usize
}
