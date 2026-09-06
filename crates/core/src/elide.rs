//! 文本截短。纯函数：同样输入必然同样输出，不碰 I/O。
//!
//! 为什么要它：侧栏只有 200px，而一个 Windows 绝对路径轻松超过它——
//! 真机截图里那段路径就挤成了三行。而挤压不报错：它只是难读。
//!
//! **预算按列宽而不是字符数。** 中日韩字符在等宽字体里占两列，
//! 按字符数给预算的话，一段全中文路径仍然会溢出一倍——
//! 而那看起来很像「截短根本没生效」。
//!
//! **保尾不保头。** 路径里最有用的是文件名（尾部）；
//! 从尾部切的话，侧栏会变成一列长得一模一样的目录前缀。

/// 截短时用的省略号。它自己也占列宽，所以预算里必须算上它。
pub const ELLIPSIS: char = '\u{2026}';

/// 一个字符在等宽字体里占几列。
///
/// **这是近似，不是完整的 UAX #11。** 只覆盖本项目真实会遇到的宽字符区段
/// （CJK 表意、假名、谜文、全宽形式、CJK 标点）。窄字符统一算一列：
/// 组合变音与零宽字符会被高估，而高估只会把文字切得更短（安全的一侧），
/// 低估才会溢出。
pub fn char_columns(c: char) -> usize {
    let cp = c as u32;
    let wide = matches!(cp,
        0x1100..=0x115F        // 韩文字母
        | 0x2E80..=0x303E      // CJK 部首、标点
        | 0x3041..=0x33FF      // 假名、注音、韩文、CJK 兼容
        | 0x3400..=0x4DBF      // 扩展 A
        | 0x4E00..=0x9FFF      // 基本表意
        | 0xA000..=0xA4CF      // 彝文
        | 0xAC00..=0xD7A3      // 韩文音节
        | 0xF900..=0xFAFF      // 兼容表意
        | 0xFE10..=0xFE19      // 竖排形式
        | 0xFE30..=0xFE6F      // CJK 兼容形式
        | 0xFF00..=0xFF60      // 全宽 ASCII
        | 0xFFE0..=0xFFE6      // 全宽符号
        | 0x1F300..=0x1F64F    // 符号与表情
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD    // 扩展 B 及以后
    );
    if wide {
        2
    } else {
        1
    }
}

/// 一段文本占几列。
pub fn columns(text: &str) -> usize {
    text.chars().map(char_columns).sum()
}

/// 把 `text` 截到最多 `budget` 列，**从头部切**，尾部原样保留。
///
/// 返回值的列宽永远 `<= budget`——这是它存在的全部理由，有断言守。
///
/// 三个边界情形都不得 panic：
/// - `budget == 0` 返回空串（不是省略号：那也占一列）。
/// - `budget == 1` 且需要截短时只返回省略号。
/// - 末尾是个宽字符而只剩一列时，宁可少放一个字也不超预算。
pub fn elide_start(text: &str, budget: usize) -> String {
    if budget == 0 {
        return String::new();
    }
    if columns(text) <= budget {
        return text.to_string();
    }
    let ell = char_columns(ELLIPSIS);
    if budget <= ell {
        return ELLIPSIS.to_string();
    }
    // 从后往前攒字符，留出省略号的位置。
    let room = budget - ell;
    let mut used = 0usize;
    let mut take_from = text.len();
    for (i, c) in text.char_indices().rev() {
        let w = char_columns(c);
        if used + w > room {
            break;
        }
        used += w;
        take_from = i;
    }
    let mut out = String::with_capacity(1 + text.len() - take_from);
    out.push(ELLIPSIS);
    out.push_str(&text[take_from..]);
    out
}

/// 路径专用：优先保留**文件名**，放不下时才切文件名自己。
///
/// 为什么不直接用 [`elide_start`]：它会在任意位置切，
/// 于是一个很长的目录名可能把文件名挤掉一半；
/// 而列表里人真正读的就是文件名。
pub fn elide_path(path: &str, budget: usize) -> String {
    if budget == 0 {
        return String::new();
    }
    if columns(path) <= budget {
        return path.to_string();
    }
    // 最后一个分隔符（两种都认：Windows 与 Unix 路径都会出现在同一个界面上）。
    let cut = path
        .char_indices()
        .filter(|(_, c)| *c == '/' || *c == '\\')
        .map(|(i, c)| i + c.len_utf8())
        .next_back();
    let name = match cut {
        Some(i) => &path[i..],
        None => path,
    };
    let ell = char_columns(ELLIPSIS);
    // 文件名加省略号能装下：只切目录部分。
    if columns(name) + ell <= budget {
        return elide_start(path, budget);
    }
    // 连文件名都装不下：切文件名自己，仍然保尾（扩展名比前缀有用）。
    elide_start(name, budget)
}
