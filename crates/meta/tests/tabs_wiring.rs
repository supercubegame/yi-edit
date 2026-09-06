//! 顶部 tab 栏接线断言。
//! 关闭按钮暂不提供：dirty 状态尚未成为 UI 的唯一状态源，不能给用户一个静默丢数据入口。

use yi_edit_meta as meta;

#[test]
fn top_level_tab_wrapper_is_compiled_and_safe_actions_are_present() {
    let main = meta::read("crates/app/src/main.rs");
    assert!(main.contains("mod tabs_ui"));
    assert!(main.contains("tabs_ui::TabsUi::new"));
    let tabs = meta::read("crates/app/src/tabs_ui.rs");
    for needle in [
        "struct TabsUi",
        "tabs: Vec<YiEdit>",
        "self.active = i",
        "YiEdit::new(None)",
    ] {
        assert!(tabs.contains(needle), "tab UI 缺少 {needle}");
    }
    assert!(
        !tabs.contains("small_button(\"×\")"),
        "dirty-close 还没接完，不能提供关闭按钮"
    );
}
