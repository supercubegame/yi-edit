//! 侧栏路径截短的接线断言。
//!
//! 纯逻辑层有测试还不够：上一轮 jump.rs 的逻辑也全绿，但运行时没画。
//! 这里直接守「路径预算真的被 UI 消费」以及「截短不等于丢信息」。

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
fn sidebar_uses_column_budgeted_elision_and_hover_preserves_information() {
    let src = compiled_ui();
    let code = meta::strip_comments_and_strings(&src);
    for needle in [
        "elide::elide_path",
        "columns_that_fit",
        "mono_small",
        "on_hover_text",
    ] {
        assert!(
            !meta::hits_in_code(&src, needle).is_empty(),
            "侧栏没有消费 {needle}，长路径会重新挤成多行"
        );
    }
    assert!(
        code.contains("elide::elide_path(&full_dir")
            && code.contains("elide::elide_path(&full"),
        "目录路径与条目名没有都走截短"
    );
    // 路径预算必须量自字体，而不是再拍一个字符数。
    assert!(
        code.contains("layout_no_wrap") && code.contains("columns_that_fit"),
        "侧栏预算不是从实际字体宽度量出来的"
    );
    // `full_dir` / `full` 必须仍存在，且 hover 读取它们；截短只是显示层变化。
    assert!(
        code.contains("on_hover_text(full_dir)") && code.contains("on_hover_text(full)"),
        "截短后没有保留完整路径的 hover 入口"
    );
}

#[test]
fn core_elide_module_is_tested_and_exported() {
    let lib = meta::read("crates/core/src/lib.rs");
    assert!(lib.contains("pub mod elide"), "elide 模块没进入 core");
    assert!(meta::exists("crates/core/tests/elide.rs"), "elide 没有测试文件");
    let tests = meta::read("crates/core/tests/elide.rs");
    for needle in [
        "the_column_budget_is_never_exceeded",
        "text_that_fits_is_returned_untouched",
        "a_char_count_budget_really_would_have_overflowed",
    ] {
        assert!(tests.contains(needle), "elide 缺少承重测试 {needle}");
    }
}
