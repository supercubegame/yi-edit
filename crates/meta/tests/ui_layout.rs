//! 界面布局里的耦合参数，以及「那个交付阻断项真的接上了」。
//! 这份断言必须跟随 main.rs 的真实 UI 入口，不能把旧文件当真身。

use yi_edit_meta as meta;

fn app_ui_source() -> String {
    let main = meta::read("crates/app/src/main.rs");
    let path = main
        .lines()
        .find_map(|l| l.trim().strip_prefix("#[path = \"").and_then(|s| s.strip_suffix("\"]")))
        .unwrap_or("ui.rs");
    meta::read(&format!("crates/app/src/{path}"))
}

fn theme_const(name: &str) -> f64 {
    let src = meta::read("crates/app/src/theme.rs");
    for line in src.lines() { let t=line.trim(); if t.starts_with('#')||t.starts_with("//"){continue} let Some(rest)=t.strip_prefix("pub const ")else{continue};let Some((decl,value))=rest.split_once('=')else{continue};if decl.split(':').next().unwrap_or_default().trim()==name{return value.trim().trim_end_matches(';').trim().parse().unwrap();} }
    panic!("找不到 {name}");
}

#[test]
fn layout_and_delivery_invariants() {
    let src = app_ui_source();
    assert!(src.contains("Event::Ime") && src.contains("IMERect") && src.contains("fn handle_ime"));
    assert!(src.contains("show_rows") || src.contains("ScrollArea"));
    assert!(src.contains("copy_text") && src.contains("Event::Paste"));
    let main=meta::read("crates/app/src/main.rs");
    let min_w=main.lines().find(|l|l.contains("with_min_inner_size")).and_then(|l|{let s=l.find('[')?+1;let e=l[s..].find(',')?+s;l[s..e].trim().parse::<f64>().ok()}).unwrap();
    let min_h=main.lines().find(|l|l.contains("with_min_inner_size")).and_then(|l|{let s=l.find(',')?+1;let e=l[s..].find(']')?+s;l[s..e].trim().parse::<f64>().ok()}).unwrap();
    let gutter=theme_const("GUTTER_W");
    assert!(theme_const("SIDEBAR_W")+theme_const("JUMP_W")+gutter<min_w/2.0);
    assert!(theme_const("TOOLBAR_H")+theme_const("FINDBAR_H")+theme_const("STATUS_H")<min_h/2.0);
}

#[test]
fn runtime_entry_and_ime_are_not_stale_copies() {
    let main=meta::read("crates/app/src/main.rs");
    let path=main.lines().find_map(|l|l.trim().strip_prefix("#[path = \"").and_then(|s|s.strip_suffix("\"]"))).unwrap_or("ui.rs");
    let src=meta::read(&format!("crates/app/src/{path}"));
    assert!(src.contains("Event::Ime"));
    assert!(src.contains("IMERect"));
    assert!(src.contains("fn handle_ime"));
    assert!(src.contains("fn sidebar"));
}
