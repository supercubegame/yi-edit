//! 语法高亮。手写的扇描器，不引入 syntect 之类的重依赖：极简风格只需要六类 token，
//! 而换来的是零依赖 + 纯函数 + 毫秒级的逐行高亮（大文件只高亮可见行）。
//!
//! 输出的 span **铺满整行**：首段从 0 开始，前一段的 end 等于后一段的 start，
//! 末段的 end 等于行长，且所有边界都是字符边界。这条不变量在 tests/highlight.rs 里
//! 对一整份语料（含中文、emoji、未闭合引号）逐行断言 —— 因为它是「不把多字节字符
//! 切坏」的唯一依据，而切坏之后的表现是 panic。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    PlainText,
    Rust,
    Python,
    JavaScript,
    Json,
    Toml,
    C,
    Markdown,
}

impl Lang {
    pub fn name(self) -> &'static str {
        match self {
            Lang::PlainText => "text",
            Lang::Rust => "rust",
            Lang::Python => "python",
            Lang::JavaScript => "javascript",
            Lang::Json => "json",
            Lang::Toml => "toml",
            Lang::C => "c",
            Lang::Markdown => "markdown",
        }
    }

    /// 所有支持的语言。这份清单就是期望：tests 里的不变量断言会遍历它，
    /// 新加语言忘了登记的话，它就完全在扫描范围之外。
    pub fn all() -> &'static [Lang] {
        &[
            Lang::PlainText,
            Lang::Rust,
            Lang::Python,
            Lang::JavaScript,
            Lang::Json,
            Lang::Toml,
            Lang::C,
            Lang::Markdown,
        ]
    }
}

pub fn lang_from_path(path: &str) -> Lang {
    let lower = path.to_ascii_lowercase();
    let ext = match lower.rsplit_once('.') {
        Some((_, e)) => e.to_string(),
        None => String::new(),
    };
    match ext.as_str() {
        "rs" => Lang::Rust,
        "py" | "pyi" => Lang::Python,
        "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" => Lang::JavaScript,
        "json" => Lang::Json,
        "toml" => Lang::Toml,
        "c" | "h" | "cc" | "cpp" | "hpp" => Lang::C,
        "md" | "markdown" => Lang::Markdown,
        _ => Lang::PlainText,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Text,
    Keyword,
    Type,
    Str,
    Number,
    Comment,
    Punct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

/// 跨行状态。块注释与块字符串（Python 三引号、Markdown 代码围栅）会跨行，
/// 而「只高亮可见行」意味着渲染第 N 行时手里必须有第 N-1 行的出口状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LineState {
    pub block_comment: bool,
    pub block_string: bool,
}

struct Syntax {
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    block_string: Option<(&'static str, &'static str)>,
    quotes: &'static [u8],
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    numbers: bool,
    markdown_headings: bool,
    /// Rust 里的 `'` 可能是生命周期（`&'a str`）而不是字符字面量。
    /// 不区分的话，一个生命周期会把后面整行染成字符串。
    quote_needs_char_shape: bool,
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
    "mut", "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];
const RUST_TYPES: &[&str] = &[
    "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "str", "u8", "u16",
    "u32", "u64", "u128", "usize", "String", "Vec", "Option", "Result", "Box",
];
const PY_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True", "try", "while",
    "with", "yield",
];
const JS_KEYWORDS: &[&str] = &[
    "async", "await", "break", "case", "catch", "class", "const", "continue", "default", "delete",
    "do", "else", "export", "extends", "false", "finally", "for", "function", "if", "import",
    "in", "instanceof", "let", "new", "null", "of", "return", "super", "switch", "this", "throw",
    "true", "try", "typeof", "undefined", "var", "void", "while", "yield",
];
const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
    "enum", "extern", "float", "for", "goto", "if", "int", "long", "register", "return", "short",
    "signed", "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned", "void",
    "volatile", "while",
];
const JSON_KEYWORDS: &[&str] = &["true", "false", "null"];
const TOML_KEYWORDS: &[&str] = &["true", "false"];

fn syntax(lang: Lang) -> Syntax {
    match lang {
        Lang::Rust => Syntax {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            block_string: None,
            quotes: &[b'"', b'\''],
            keywords: RUST_KEYWORDS,
            types: RUST_TYPES,
            numbers: true,
            markdown_headings: false,
            quote_needs_char_shape: true,
        },
        Lang::Python => Syntax {
            line_comments: &["#"],
            block_comment: None,
            block_string: Some(("\"\"\"", "\"\"\"")),
            quotes: &[b'"', b'\''],
            keywords: PY_KEYWORDS,
            types: &["bool", "bytes", "dict", "float", "int", "list", "set", "str", "tuple"],
            numbers: true,
            markdown_headings: false,
            quote_needs_char_shape: false,
        },
        Lang::JavaScript => Syntax {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            block_string: None,
            quotes: &[b'"', b'\'', b'`'],
            keywords: JS_KEYWORDS,
            types: &["Array", "Boolean", "Number", "Object", "Promise", "String"],
            numbers: true,
            markdown_headings: false,
            quote_needs_char_shape: false,
        },
        Lang::Json => Syntax {
            line_comments: &[],
            block_comment: None,
            block_string: None,
            quotes: &[b'"'],
            keywords: JSON_KEYWORDS,
            types: &[],
            numbers: true,
            markdown_headings: false,
            quote_needs_char_shape: false,
        },
        Lang::Toml => Syntax {
            line_comments: &["#"],
            block_comment: None,
            block_string: None,
            quotes: &[b'"', b'\''],
            keywords: TOML_KEYWORDS,
            types: &[],
            numbers: true,
            markdown_headings: false,
            quote_needs_char_shape: false,
        },
        Lang::C => Syntax {
            line_comments: &["//"],
            block_comment: Some(("/*", "*/")),
            block_string: None,
            quotes: &[b'"', b'\''],
            keywords: C_KEYWORDS,
            types: &["FILE", "size_t", "ssize_t", "uint8_t", "uint32_t", "uint64_t"],
            numbers: true,
            markdown_headings: false,
            quote_needs_char_shape: false,
        },
        Lang::Markdown => Syntax {
            line_comments: &[],
            block_comment: None,
            block_string: Some(("```", "```")),
            quotes: &[b'`'],
            keywords: &[],
            types: &[],
            numbers: false,
            markdown_headings: true,
            quote_needs_char_shape: false,
        },
        Lang::PlainText => Syntax {
            line_comments: &[],
            block_comment: None,
            block_string: None,
            quotes: &[],
            keywords: &[],
            types: &[],
            numbers: false,
            markdown_headings: false,
            quote_needs_char_shape: false,
        },
    }
}

#[inline]
fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x80
}

#[inline]
fn is_ident_byte(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

fn starts_with(b: &[u8], i: usize, pat: &[u8]) -> bool {
    !pat.is_empty() && b.len() >= i + pat.len() && &b[i..i + pat.len()] == pat
}

fn starts_with_any(b: &[u8], i: usize, pats: &[&str]) -> bool {
    pats.iter().any(|p| starts_with(b, i, p.as_bytes()))
}

fn find_str(b: &[u8], pat: &[u8], from: usize) -> Option<usize> {
    if pat.is_empty() || from > b.len() {
        return None;
    }
    let mut i = from;
    while i + pat.len() <= b.len() {
        if &b[i..i + pat.len()] == pat {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// `'a'` / `'\n'` / `'你'` 返回结束偏移；`'a` （生命周期）返回 None。
fn char_literal_end(line: &str, i: usize) -> Option<usize> {
    let b = line.as_bytes();
    if b.get(i) != Some(&b'\'') || i + 1 >= b.len() {
        return None;
    }
    if b[i + 1] == b'\\' {
        let mut j = i + 2;
        while j < b.len() && b[j] != b'\'' {
            j += 1;
        }
        return if j < b.len() { Some(j + 1) } else { None };
    }
    let c = line[i + 1..].chars().next()?;
    let end = i + 1 + c.len_utf8();
    if b.get(end) == Some(&b'\'') {
        Some(end + 1)
    } else {
        None
    }
}

fn push(spans: &mut Vec<Span>, text_start: &mut usize, start: usize, end: usize, kind: TokenKind) {
    if start > *text_start {
        spans.push(Span {
            start: *text_start,
            end: start,
            kind: TokenKind::Text,
        });
    }
    if end > start {
        spans.push(Span { start, end, kind });
        *text_start = end;
    }
}

fn seal(mut spans: Vec<Span>, text_start: usize, len: usize) -> Vec<Span> {
    if text_start < len {
        spans.push(Span {
            start: text_start,
            end: len,
            kind: TokenKind::Text,
        });
    }
    spans
}

/// 高亮一行。`state` 是上一行的出口状态，返回值的第二项是本行的出口状态。
/// 纯函数：同样的 (line, lang, state) 必然得到同样的输出。
pub fn highlight_line(line: &str, lang: Lang, state_in: LineState) -> (Vec<Span>, LineState) {
    let sx = syntax(lang);
    let b = line.as_bytes();
    let len = b.len();
    let mut spans: Vec<Span> = Vec::new();
    let mut text_start = 0usize;
    let mut state = state_in;
    let mut i = 0usize;

    // 承接上一行的块注释 / 块字符串。
    if state.block_comment {
        let close = sx.block_comment.map(|(_, c)| c).unwrap_or("");
        match find_str(b, close.as_bytes(), 0) {
            Some(p) => {
                let e = p + close.len();
                push(&mut spans, &mut text_start, 0, e, TokenKind::Comment);
                state.block_comment = false;
                i = e;
            }
            None => {
                push(&mut spans, &mut text_start, 0, len, TokenKind::Comment);
                return (seal(spans, text_start, len), state);
            }
        }
    } else if state.block_string {
        let close = sx.block_string.map(|(_, c)| c).unwrap_or("");
        match find_str(b, close.as_bytes(), 0) {
            Some(p) => {
                let e = p + close.len();
                push(&mut spans, &mut text_start, 0, e, TokenKind::Str);
                state.block_string = false;
                i = e;
            }
            None => {
                push(&mut spans, &mut text_start, 0, len, TokenKind::Str);
                return (seal(spans, text_start, len), state);
            }
        }
    } else if sx.markdown_headings && starts_with(b, 0, b"#") {
        push(&mut spans, &mut text_start, 0, len, TokenKind::Keyword);
        return (seal(spans, text_start, len), state);
    }

    while i < len {
        if starts_with_any(b, i, sx.line_comments) {
            push(&mut spans, &mut text_start, i, len, TokenKind::Comment);
            i = len;
            break;
        }
        if let Some((open, close)) = sx.block_comment {
            if starts_with(b, i, open.as_bytes()) {
                match find_str(b, close.as_bytes(), i + open.len()) {
                    Some(p) => {
                        let e = p + close.len();
                        push(&mut spans, &mut text_start, i, e, TokenKind::Comment);
                        i = e;
                    }
                    None => {
                        push(&mut spans, &mut text_start, i, len, TokenKind::Comment);
                        state.block_comment = true;
                        i = len;
                    }
                }
                continue;
            }
        }
        if let Some((open, close)) = sx.block_string {
            if starts_with(b, i, open.as_bytes()) {
                match find_str(b, close.as_bytes(), i + open.len()) {
                    Some(p) => {
                        let e = p + close.len();
                        push(&mut spans, &mut text_start, i, e, TokenKind::Str);
                        i = e;
                    }
                    None => {
                        push(&mut spans, &mut text_start, i, len, TokenKind::Str);
                        state.block_string = true;
                        i = len;
                    }
                }
                continue;
            }
        }
        if sx.quotes.contains(&b[i]) {
            if b[i] == b'\'' && sx.quote_needs_char_shape {
                match char_literal_end(line, i) {
                    Some(e) => {
                        push(&mut spans, &mut text_start, i, e, TokenKind::Str);
                        i = e;
                    }
                    None => {
                        // 生命周期：当成标点，不开字符串。
                        push(&mut spans, &mut text_start, i, i + 1, TokenKind::Punct);
                        i += 1;
                    }
                }
                continue;
            }
            let q = b[i];
            let mut j = i + 1;
            while j < len {
                if b[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if b[j] == q {
                    j += 1;
                    break;
                }
                j += 1;
            }
            let e = j.min(len);
            push(&mut spans, &mut text_start, i, e, TokenKind::Str);
            i = e;
            continue;
        }
        if sx.numbers && b[i].is_ascii_digit() && (i == 0 || !is_ident_byte(b[i - 1])) {
            let mut j = i;
            while j < len && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'.') {
                // `1..10` 这种区间不要把第二个点吞进数字里。
                if b[j] == b'.' && j + 1 < len && b[j + 1] == b'.' {
                    break;
                }
                j += 1;
            }
            push(&mut spans, &mut text_start, i, j, TokenKind::Number);
            i = j;
            continue;
        }
        if is_ident_start(b[i]) {
            let mut j = i;
            while j < len && is_ident_byte(b[j]) {
                j += 1;
            }
            let word = &line[i..j];
            if sx.keywords.contains(&word) {
                push(&mut spans, &mut text_start, i, j, TokenKind::Keyword);
            } else if sx.types.contains(&word) || first_char_is_upper(word) {
                push(&mut spans, &mut text_start, i, j, TokenKind::Type);
            }
            i = j;
            continue;
        }
        if b[i].is_ascii_punctuation() {
            // 每次只吃一个字节：标点成串处理会把紧跟在后面的注释开头一起吞掉。
            push(&mut spans, &mut text_start, i, i + 1, TokenKind::Punct);
            i += 1;
            continue;
        }
        i += 1;
    }

    (seal(spans, text_start, len), state)
}

fn first_char_is_upper(word: &str) -> bool {
    word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
}
