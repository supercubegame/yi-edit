use yi_edit_core::highlight::TokenKind;
use yi_edit_core::{highlight_line, lang_from_path, Lang, LineState};

/// 语料：每种语言都会拿它跑一遍（故意让它们互相语法错乱）。
/// 含中文、emoji、未闭合引号、未闭合块注释、生命周期、空行。
const CORPUS: &[&str] = &[
    "",
    "    ",
    "fn main() { let x = 42; }",
    "let s = \"中文 string with spaces\";",
    "// 注释里有 std::fs 和 \"引号\"",
    "/* 未闭合的块注释",
    "x = 'a'; y = '\\n'; z: &'a str = \"q\";",
    "s = 'unterminated",
    "# 标题 with emoji \u{1f600}",
    "value = 3.14 + 0xff_u8 + 1..10",
    "中文变量名 = 中文值  # 尾巴注释",
    "```rust",
    "\"\"\"docstring 开头",
    "{\"key\": [1, 2, true, null]}",
    "a\u{1f600}b = '中'",
];

/// 承重不变量：span 铺满整行、不重叠、边界在字符边界上。
/// 最后一条是「不把中文/emoji 切坏」的唯一依据；前两条保证渲染时不会漏字或重字。
#[test]
fn spans_tile_the_line_on_char_boundaries() {
    let mut checked = 0usize;
    for lang in Lang::all() {
        for line in CORPUS {
            for state in [
                LineState::default(),
                LineState {
                    block_comment: true,
                    block_string: false,
                },
                LineState {
                    block_comment: false,
                    block_string: true,
                },
            ] {
                let (spans, _next) = highlight_line(line, *lang, state);
                let mut cursor = 0usize;
                for s in &spans {
                    assert_eq!(
                        s.start, cursor,
                        "span 不连续：lang={:?} line={line:?} spans={spans:?}",
                        lang
                    );
                    assert!(s.end > s.start, "空 span：lang={:?} line={line:?}", lang);
                    assert!(
                        line.is_char_boundary(s.start) && line.is_char_boundary(s.end),
                        "span 边界切到了字符中间：lang={:?} line={line:?} span={s:?}",
                        lang
                    );
                    cursor = s.end;
                }
                assert_eq!(
                    cursor,
                    line.len(),
                    "span 没铺满整行：lang={:?} line={line:?} spans={spans:?}",
                    lang
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 300, "只检查了 {checked} 组，语料或语言清单缩水了");
}

#[test]
fn block_comment_carries_to_the_next_line() {
    let (_, st) = highlight_line("/* 开头", Lang::Rust, LineState::default());
    assert!(st.block_comment, "未闭合的块注释应该把状态带到下一行");
    let (spans, st2) = highlight_line("中间 */ let x = 1;", Lang::Rust, st);
    assert!(!st2.block_comment, "遇到 */ 应该结束块注释");
    assert_eq!(spans[0].kind, TokenKind::Comment);
    assert!(
        spans.iter().any(|s| s.kind == TokenKind::Keyword),
        "注释结束之后的 let 应该还能识别成关键字：{spans:?}"
    );
}

#[test]
fn keywords_are_not_matched_inside_identifiers() {
    let (spans, _) = highlight_line("letter fnord ifx", Lang::Rust, LineState::default());
    assert!(
        !spans.iter().any(|s| s.kind == TokenKind::Keyword),
        "标识符内部的关键字不应该命中：{spans:?}"
    );
}

/// Rust 的 `&'a str` 里的 `'` 是生命周期，不是字符串开头。
/// 不区分的话，它后面整行都会变成字符串色。
#[test]
fn rust_lifetime_is_not_a_string() {
    let (spans, _) = highlight_line("fn f(s: &'a str) -> u8 { 1 }", Lang::Rust, LineState::default());
    let str_spans: Vec<_> = spans.iter().filter(|s| s.kind == TokenKind::Str).collect();
    assert!(str_spans.is_empty(), "生命周期被当成字符串了：{str_spans:?}");
    let (spans2, _) = highlight_line("let c = 'x';", Lang::Rust, LineState::default());
    assert!(
        spans2.iter().any(|s| s.kind == TokenKind::Str),
        "真正的字符字面量还是要高亮：{spans2:?}"
    );
}

#[test]
fn python_triple_quote_and_markdown_fence_carry_state() {
    let (_, st) = highlight_line("s = \"\"\"开头", Lang::Python, LineState::default());
    assert!(st.block_string);
    let (_, st2) = highlight_line("结尾\"\"\"", Lang::Python, st);
    assert!(!st2.block_string);

    let (_, md) = highlight_line("```rust", Lang::Markdown, LineState::default());
    assert!(md.block_string, "代码围栅要能跨行");
    let (_, md2) = highlight_line("```", Lang::Markdown, md);
    assert!(!md2.block_string);
}

#[test]
fn lang_from_path_covers_the_documented_extensions() {
    assert_eq!(lang_from_path("a/b/c.rs"), Lang::Rust);
    assert_eq!(lang_from_path("MAIN.PY"), Lang::Python);
    assert_eq!(lang_from_path("x.tsx"), Lang::JavaScript);
    assert_eq!(lang_from_path("Cargo.toml"), Lang::Toml);
    assert_eq!(lang_from_path("README.md"), Lang::Markdown);
    assert_eq!(lang_from_path("noext"), Lang::PlainText);
    assert_eq!(lang_from_path("data.unknown"), Lang::PlainText);
}
