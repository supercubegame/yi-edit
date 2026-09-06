//! 自动缩进与括号匹配。纯函数：同样输入必然同样输出，不碰 I/O。
//!
//! 括号匹配**不允许数字符串与注释里的括号**：`let s = "(";` 里那个开括号
//! 一旦参与计数，它后面整个文件的配对全部错位 —— 而错位不报错，
//! 只是高亮到一个不相干的位置，用户会以为自己的代码不平衡。
//! 屏蔽区域直接复用高亮器的输出（Str / Comment 两类 token），不再写一份真身。

use crate::highlight::{highlight_line, Lang, LineState, TokenKind};

/// 一个缩进单位的宽度（空格数）。
///
/// UI 里的 Tab 插入必须走 [`indent_unit`]，不得自己写四个空格：
/// 两份真身会各自漂，而漂了之后 Tab 与自动缩进对不齐。
const INDENT_WIDTH: usize = 4;

/// 括号匹配的上限。超过它就不匹配：匹配需要扫全文，
/// 而每帧扫一遍几十 MB 不可接受。这是一条已知限制，不是失败。
pub const MAX_BRACKET_MATCH_BYTES: usize = 1024 * 1024;

const PAIRS: &[(char, char)] = &[('(', ')'), ('[', ']'), ('{', '}')];

/// 一个缩进单位的文本。Tab 与自动缩进共用它。
pub fn indent_unit() -> String {
    " ".repeat(INDENT_WIDTH)
}

pub fn indent_width() -> usize {
    INDENT_WIDTH
}

/// 配对的那一半；不是括号就是 None。
pub fn matching_char(c: char) -> Option<char> {
    PAIRS.iter().find_map(|(open, close)| {
        if *open == c {
            Some(*close)
        } else if *close == c {
            Some(*open)
        } else {
            None
        }
    })
}

pub fn is_open(c: char) -> bool {
    PAIRS.iter().any(|(open, _)| *open == c)
}

pub fn is_close(c: char) -> bool {
    PAIRS.iter().any(|(_, close)| *close == c)
}

/// 一行的前导空白（原样返回，制表符与空格都保留）。
pub fn leading_whitespace(line: &str) -> &str {
    let end = line
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .map(|(i, _)| i)
        .unwrap_or(line.len());
    &line[..end]
}

fn clamp_col(line: &str, col: usize) -> usize {
    let mut c = col.min(line.len());
    while c > 0 && !line.is_char_boundary(c) {
        c -= 1;
    }
    c
}

/// 回车时要插入什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewlineEdit {
    /// 要插入的文本（含换行符与缩进）。
    pub insert: String,
    /// 插入完之后光标应该落在 `insert` 的哪个字节偏移上。
    ///
    /// 它不一定等于 `insert.len()`：在 `{}` 中间回车时插入两行，
    /// 而光标要停在**中间那行**的末尾。
    pub cursor_offset: usize,
    /// 是不是把一对括号拆成了三行。上层靠它决定要不要回调光标。
    pub split_pair: bool,
}

/// 在 `line` 的第 `col` 个字节处敲回车。
///
/// 三条规则，全部与语言无关（不猜“这是 Rust”）：
/// 1. 默认继承本行的缩进。
/// 2. 光标前的最后一个非空白字符是开括号 -> 多缩一级。
/// 3. 另外光标后的第一个非空白字符正好是它的配对 -> 再插一行把闭括号顶下去，
///    光标停在中间。
pub fn newline_edit(line: &str, col: usize) -> NewlineEdit {
    let col = clamp_col(line, col);
    let before = &line[..col];
    let after = &line[col..];

    // 光标落在前导空白里时，继承的缩进不得超过光标自己的位置，
    // 否则在行首敲回车会凭空多出一段缩进。
    let base_len = leading_whitespace(line).len().min(col);
    let base = &line[..base_len];

    let opened = before.trim_end().chars().last().filter(|c| is_open(*c));
    let next = after.trim_start().chars().next();

    let mut insert = String::from("\n");
    insert.push_str(base);
    if opened.is_some() {
        insert.push_str(&indent_unit());
    }
    let cursor_offset = insert.len();

    let split_pair = match opened {
        Some(open) => next == matching_char(open),
        None => false,
    };
    if split_pair {
        insert.push('\n');
        insert.push_str(base);
    }

    NewlineEdit {
        insert,
        cursor_offset,
        split_pair,
    }
}

/// 不参与括号匹配的字节区间（字符串与注释）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mask {
    /// 按起点升序、不重叠。`contains` 靠这两个性质做二分。
    spans: Vec<(usize, usize)>,
}

impl Mask {
    /// 把字符串与注释标成屏蔽区。`text` 的行分隔符必须是 `\n`。
    pub fn from_text(text: &str, lang: Lang) -> Self {
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut state = LineState::default();
        let mut base = 0usize;
        for line in text.split('\n') {
            let (tokens, next) = highlight_line(line, lang, state);
            for span in tokens {
                if matches!(span.kind, TokenKind::Str | TokenKind::Comment) {
                    spans.push((base + span.start, base + span.end));
                }
            }
            state = next;
            base += line.len() + 1;
        }
        Self { spans }
    }

    pub fn contains(&self, at: usize) -> bool {
        let i = self.spans.partition_point(|(start, _)| *start <= at);
        i > 0 && self.spans[i - 1].1 > at
    }

    pub fn span_count(&self) -> usize {
        self.spans.len()
    }
}

/// `at` 处那个括号的配对位置。
///
/// `at` 不是括号、落在屏蔽区里、或者根本不平衡时返回 None。
/// 三种情形都归 None 是故意的：对调用方来说它们的动作一样（不画高亮）。
pub fn match_bracket(text: &str, mask: &Mask, at: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if at >= bytes.len() || mask.contains(at) {
        return None;
    }
    let here = text[at..].chars().next()?;
    let other = matching_char(here)?;
    let mut depth = 0i32;

    // 按字节扫：括号全是 ASCII，而多字节字符的任何一个字节都 ≥ 0x80，
    // 不可能等于括号，所以不会把中文里的某个字节误认成括号。
    if is_open(here) {
        let mut i = at;
        while i < bytes.len() {
            if !mask.contains(i) {
                let c = bytes[i] as char;
                if c == here {
                    depth += 1;
                } else if c == other {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
            i += 1;
        }
    } else {
        let mut i = at + 1;
        while i > 0 {
            i -= 1;
            if !mask.contains(i) {
                let c = bytes[i] as char;
                if c == here {
                    depth += 1;
                } else if c == other {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
        }
    }
    None
}

/// 光标处或光标**前一个字符**的括号与它的配对。
///
/// 为什么要看前一个：刚敲完 `}` 时光标在它右边，而那正是最想看到配对的时候。
pub fn bracket_pair_at(text: &str, mask: &Mask, cursor: usize) -> Option<(usize, usize)> {
    let mut candidates: Vec<usize> = Vec::new();
    if cursor < text.len() && text.is_char_boundary(cursor) {
        candidates.push(cursor);
    }
    if cursor > 0 {
        let mut prev = cursor - 1;
        while prev > 0 && !text.is_char_boundary(prev) {
            prev -= 1;
        }
        candidates.push(prev);
    }
    for at in candidates {
        if let Some(other) = match_bracket(text, mask, at) {
            return Some((at, other));
        }
    }
    None
}
