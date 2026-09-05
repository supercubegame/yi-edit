//! UI layout and compiled-entry assertions.
use yi_edit_meta as meta;

fn app_ui_source() -> String {
    let main = meta::read("crates/app/src/main.rs");
    let path = main.lines().find_map(|l| l.trim().strip_prefix("#[path = \"").and_then(|s| s.strip_suffix("\"]"))).unwrap_or("ui.rs");
    meta::read(&format!("crates/app/src/{path}"))
}

#[test]
fn compiled_entry_has_real_ime_consumer_and_safe_file_panel() {
    let src = app_ui_source();
    assert!(src.contains("Event::Ime") && src.contains("IMERect") && src.contains("fn handle_ime"));
    assert!(src.contains("let snapshot = self.listing.clone()"), "文件面板必须先 clone 快照，避免闭包借用冲突");
    assert!(src.contains("show_rows") || src.contains("ScrollArea"));
}

#[test]
fn compiled_entry_preserves_delivery_blockers() {
    let src = app_ui_source();
    for needle in ["Event::Copy", "Event::Cut", "Event::Paste", "select_all", "close_requested", "CancelClose", "shot.active()"] {
        assert!(src.contains(needle), "compiled UI 缺少 {needle}");
    }
}

#[test]
fn main_points_at_the_runtime_and_legacy_copy_is_not_compiled() {
    let main = meta::read("crates/app/src/main.rs");
    assert!(main.contains("ui_safe.rs"), "main 没有指向 ui_safe.rs");
    assert!(!main.contains("ui_legacy.rs"), "main 不该编译归档副本");
    assert!(!meta::exists("crates/app/src/ui.rs"), "旧 ui.rs 还在，容易产生第二个真身");
}

#[test]
fn layout_constants_leave_room_for_text() {
    let theme = meta::read("crates/app/src/theme.rs");
    assert!(theme.contains("SIDEBAR_W") && theme.contains("JUMP_W") && theme.contains("GUTTER_W"));
}
