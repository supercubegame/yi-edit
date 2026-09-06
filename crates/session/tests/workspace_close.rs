use std::fs;
use std::path::PathBuf;

use yi_edit_core::Pos;
use yi_edit_session::workspace::{CloseDecision, Workspace};

struct Tmp(PathBuf);
impl Tmp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("yi-workspace-close-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn file(&self) -> PathBuf {
        let p = self.0.join("dirty.rs");
        fs::write(&p, "fn main() {}\n").unwrap();
        p
    }
}
impl Drop for Tmp { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

#[test]
fn normal_close_refuses_dirty_but_explicit_discard_closes() {
    let t = Tmp::new();
    let mut ws = Workspace::new();
    ws.open(&t.file()).unwrap();
    ws.active_mut().editor.cursor = Pos::new(0, 0);
    assert!(ws.active_mut().editor.insert_text("// dirty\n"));
    assert_eq!(ws.close_active(), CloseDecision::RefusedDirty);
    assert_eq!(ws.discard_close_active(), CloseDecision::Closed);
    assert_eq!(ws.tabs().len(), 1, "关闭最后 tab 后应保留干净未命名 tab");
    assert!(!ws.active().is_dirty());
}

#[test]
fn invalid_discard_index_is_still_rejected() {
    let mut ws = Workspace::new();
    assert_eq!(ws.discard_close(99), CloseDecision::InvalidIndex);
    assert_eq!(ws.tabs().len(), 1);
}
