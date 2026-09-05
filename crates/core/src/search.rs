//! 纯字节搜索。Boyer–Moore–Horspool，零依赖。
//!
//! 明确写下来的语义（tests/search.rs 里每条都有断言，不是文档承诺）：
//! - 匹配**不重叠**：在 "aaa" 里找 "aa" 只有一个匹配（偏移 0）。这与替换的语义一致，
//!   否则「搜到 2 个、替换掉 1 个」这种不一致会在大文件上变成难查的差异。
//! - 大小写不敏感**只折叠 ASCII**。非 ASCII 的大小写（İ、ß 之类）不支持，见 docs/PITFALLS.md。
//! - 在合法 UTF-8 里找合法 UTF-8，匹配位置必然落在字符边界上（UTF-8 自同步）。
//!   这一条容易被当成显然的事而不测，但它是「不切坏中文」的唯一依据，所以有断言。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
}

impl SearchOptions {
    pub fn exact() -> Self {
        Self {
            case_sensitive: true,
            whole_word: false,
        }
    }
}

#[inline]
fn fold(b: u8, case_sensitive: bool) -> u8 {
    if case_sensitive {
        b
    } else {
        b.to_ascii_lowercase()
    }
}

/// 词字符：ASCII 字母数字、下划线，以及所有非 ASCII 字节（多字节字符算词内）。
#[inline]
pub fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

fn eq_folded(a: &[u8], b: &[u8], case_sensitive: bool) -> bool {
    if case_sensitive {
        return a == b;
    }
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.to_ascii_lowercase() == y.to_ascii_lowercase())
}

fn word_bounded(hay: &[u8], start: usize, len: usize) -> bool {
    let left = start == 0 || !is_word_byte(hay[start - 1]);
    let right = start + len >= hay.len() || !is_word_byte(hay[start + len]);
    left && right
}

/// 从 from 开始找第一个匹配的起点。
pub fn find_from(hay: &[u8], needle: &[u8], from: usize, opts: SearchOptions) -> Option<usize> {
    let m = needle.len();
    if m == 0 || from > hay.len() || hay.len() - from < m {
        return None;
    }
    // BMH 跳表：按「窗口最后一个字节」决定位移。整数溢出不可能，位移最大为 m。
    let mut skip = [m; 256];
    for (i, b) in needle.iter().enumerate().take(m - 1) {
        skip[fold(*b, opts.case_sensitive) as usize] = m - 1 - i;
    }
    let last = fold(needle[m - 1], opts.case_sensitive);
    let mut pos = from;
    while pos + m <= hay.len() {
        let tail = fold(hay[pos + m - 1], opts.case_sensitive);
        if tail == last
            && eq_folded(&hay[pos..pos + m], needle, opts.case_sensitive)
            && (!opts.whole_word || word_bounded(hay, pos, m))
        {
            return Some(pos);
        }
        // whole_word 判失败时也用同样的位移：整词匹配是普通匹配的子集，
        // 而 BMH 的位移只依赖窗口末字节的对齐，不会跳过任何可能的起点。
        pos += skip[tail as usize];
    }
    None
}

/// 所有不重叠匹配的起点。
pub fn find_all(hay: &[u8], needle: &[u8], opts: SearchOptions) -> Vec<usize> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while let Some(p) = find_from(hay, needle, pos, opts) {
        out.push(p);
        pos = p + needle.len().max(1);
    }
    out
}

/// 起点严格小于 before 的最后一个匹配（向上查找）。
/// V1 的实现是从头正扫，在超大文件上是 O(n)。这是已知的性能洞，记在 docs/PITFALLS.md 里，
/// 不假装它是 O(1)。
pub fn find_before(hay: &[u8], needle: &[u8], before: usize, opts: SearchOptions) -> Option<usize> {
    let mut best = None;
    let mut pos = 0usize;
    while let Some(p) = find_from(hay, needle, pos, opts) {
        if p >= before {
            break;
        }
        best = Some(p);
        pos = p + needle.len().max(1);
    }
    best
}
