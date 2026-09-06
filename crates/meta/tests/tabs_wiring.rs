//! 顶部 tab 栏接线断言。
//! 关闭按钮故意还没放：dirty 状态尚未下沉到 Workspace，先守住“不提供静默丢数据入口”。

use yi_edit_meta as meta;

#[test]
fn top_level_tab_wrapper_is_the_compiled_app() {
    let main = meta::read("crates/app/src/main.rs");
    assert!(main.contains("mod tabs_ui"));
    assert!(main.contains("tabs_ui::TabsUi::new"));
    let tabs = meta::read("crates/app/src/tabs_ui.rs");
    for needle in ["struct TabsUi", "tabs: Vec<YiEdit>", "self.active = i", "YiEdit::new(None)"] {
        assert!(tabs.contains(needle), "tab UI 缺少 {needle}");
    }
}

#[test]
fn tab_ui_has_no_silent_close_button_before_dirty_state_is_wired() {
    let tabs = meta::read("crates/app/src/tabs_ui.rs");
    assert!(!tabs.contains("small_button(\"×\")"));
    assert!(tabs.contains("dirty-close") || tabs.contains("关闭按钮暂不画"));
}
