//! 多文件工作区的承重测试。
//!
//! 重点不是「能 push 一个 tab」：重点是重复打开不制造两个缓冲区，
//! 关闭顺序不跳错当前 tab，脏 tab 不会被静默丢掉。

use std::fs;
use std::path::PathBuf;

use yi_edit_session::workspace::{CloseDecision, Workspace};
use yi_edit_core::Pos;

struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("yi-workspace-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("建临时目录");
        Self(dir)
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let p = self.0.join(name);
        fs::write(&p, text).expect("写临时文件");
        p
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn new_workspace_starts_with_one_clean_untitled_tab() {
    let ws = Workspace::new();
    assert_eq!(ws.tabs().len(), 1);
    assert_eq!(ws.active_index(), 0);
    assert!(ws.active().editor.path.is_none());
    assert!(!ws.active().is_dirty());
    assert_eq!(ws.active().title, "未命名");
}

#[test]
fn opening_the_same_path_twice_activates_instead_of_duplicating() {
    let t = Tmp::new();
    let a = t.file("a.rs", "fn a() {}\n");
    let mut ws = Workspace::new();
    let first = ws.open(&a).expect("第一次打开");
    let second = ws.open(&a).expect("第二次打开");
    assert_eq!(first, second);
    assert_eq!(ws.tabs().len(), 2, "同一文件被打开成了两个缓冲区");
    assert_eq!(ws.active().title, "a.rs");
}

#[test]
fn active_switch_and_close_keep_the_remaining_tab_active() {
    let t = Tmp::new();
    let a = t.file("a.rs", "a\n");
    let b = t.file("b.rs", "b\n");
    let mut ws = Workspace::new();
    ws.open(&a).expect("打开 a");
    ws.open(&b).expect("打开 b");
    assert_eq!(ws.active().title, "b.rs");
    assert!(ws.activate(1));
    assert_eq!(ws.active().title, "a.rs");
    assert_eq!(ws.close_active(), CloseDecision::Closed);
    assert_eq!(ws.active().title, "b.rs");
    assert!(!ws.activate(99), "越界 activate 居然成功");
}

#[test]
fn dirty_tab_cannot_be_closed_silently() {
    let t = Tmp::new();
    let a = t.file("a.rs", "fn a() {}\n");
    let mut ws = Workspace::new();
    ws.open(&a).expect("打开");
    ws.active_mut().editor.cursor = Pos::new(0, 0);
    assert!(ws.active_mut().editor.insert_text("// dirty\n"));
    assert_eq!(ws.close_active(), CloseDecision::RefusedDirty);
    assert_eq!(ws.tabs().len(), 2);
}

#[test]
fn closing_the_last_tab_leaves_a_clean_untitled_tab() {
    let mut ws = Workspace::new();
    assert_eq!(ws.close_active(), CloseDecision::Closed);
    assert_eq!(ws.tabs().len(), 1);
    assert!(ws.active().editor.path.is_none());
    assert!(!ws.active().is_dirty());
}

#[test]
fn paths_are_a_snapshot_not_a_mutable_alias() {
    let t = Tmp::new();
    let a = t.file("a.rs", "a\n");
    let mut ws = Workspace::new();
    ws.open(&a).expect("打开");
    let paths = ws.paths();
    assert_eq!(paths.len(), ws.tabs().len());
    assert_eq!(paths[1].as_deref(), Some(a.as_path()));
}
