//! 自动缩进与括号匹配的**接线**断言。
//!
//! 为什么单独一份：上一轮已经踩过一次「一层纯逻辑写完了、断言全绿、
//! 而输出从没上屏」（jump.rs）。那种形状看起来比没写还像做完了，
//! 而它不会让任何东西报错 —— 所以每加一层纯逻辑，就配一条「它真的被消费了」。

use yi_edit_meta as meta;

fn compiled_ui() -> String {
    let main = meta::read("crates/app/src/main.rs");
    let path = main
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("#[path = \"")
                .and_then(|s| s.strip_suffix("\"]"))
        })
        .unwrap_or("ui.rs");
    meta::read(&format!("crates/app/src/{path}"))
}

#[test]
fn the_runtime_really_consumes_the_indent_layer() {
    let src = compiled_ui();
    for needle in [
        "indent::newline_edit",
        "indent::bracket_pair_at",
        "indent::Mask::from_text",
        "indent::indent_unit",
    ] {
        assert!(
            !meta::hits_in_code(&src, needle).is_empty(),
            "compiled UI 没有走 {needle}，那层断言就守着一堆没人看的输出"
        );
    }
    // Enter 必须走自动缩进那条路，不能还是直接插一个换行符。
    let code = meta::strip_comments_and_strings(&src);
    assert!(
        code.contains("fn newline_with_indent") && code.contains("self.newline_with_indent()"),
        "自动缩进定义了却没接到 Enter 上"
    );
    // 括号高亮同理：算了而不画等于没做。
    assert!(
        code.contains("self.bracket = self.bracket_pair()"),
        "括号配对没在每帧算"
    );
    assert!(
        code.contains("if let Some((here, other)) = bracket"),
        "算出来的配对没被画到行上"
    );
}

/// Tab 与自动缩进共用同一个缩进单位。
///
/// 负向那条是承重的：写死四个空格不会报错，只会在改了缩进宽度之后
/// 让 Tab 与回车的缩进对不齐 —— 而那时候人会以为自动缩进坏了。
#[test]
fn tab_and_auto_indent_share_one_source_of_truth() {
    let src = compiled_ui();
    let code = meta::strip_comments_and_strings(&src);
    assert!(
        code.contains("indent::indent_unit()"),
        "Tab 没用共用的缩进单位"
    );
    // 字符串字面量要从原文里找（剥离器会把字符串抹成空白）。
    assert!(
        !src.contains("self.insert(\"    \")"),
        "Tab 又写死了四个空格，现在有两份真身"
    );
}

/// 括号匹配要扫全文，所以必须有一个显式上限。
/// 没上限的话它不会报错，只会在大文件上每帧扫几十 MB（表现是“编辑器很卡”）。
#[test]
fn bracket_matching_has_an_explicit_size_limit() {
    let src = compiled_ui();
    let code = meta::strip_comments_and_strings(&src);
    assert!(
        code.contains("MAX_BRACKET_MATCH_BYTES"),
        "括号匹配没有任何尺寸上限"
    );
    assert!(code.contains("is_huge()"), "大文件只读模式下没有跳过匹配");
    let core = meta::read("crates/core/src/indent.rs");
    assert!(
        meta::hits_in_code(&core, "MAX_BRACKET_MATCH_BYTES")
            .iter()
            .any(|(_, l)| l.contains("pub const")),
        "上限不是一个公开常量，那 UI 里就会再拍一个数"
    );
}

/// 断言本身要真的跑得到：新模块必须有一份测试文件，而且里面有那条对照实验。
/// 只验「引号里的括号不算」的话，一个永远返回 None 的实现也能完美交差。
#[test]
fn the_indent_layer_has_tests_including_the_control_experiment() {
    assert!(
        meta::exists("crates/core/tests/indent.rs"),
        "新模块没有测试文件"
    );
    let tests = meta::read("crates/core/tests/indent.rs");
    assert!(
        tests.contains("brackets_inside_strings_and_comments_do_not_count"),
        "字符串/注释屏蔽那条断言没了"
    );
    assert!(
        tests.contains("对照侧"),
        "只验了屏蔽侧，没有对照实验；一个永远返回 None 的实现也能过"
    );
    assert!(
        tests.contains("splitting_a_pair_puts_the_cursor_in_the_middle"),
        "拆开括号时光标位置那条断言没了"
    );
}
