//! 多文件工作区模型。纯会话逻辑，不碰 GUI。
//!
//! UI 先有一份 `Editor`，一开第二个文件就会遇到三个不能靠按钮解决的问题：
//! 当前 tab、关闭顺序、以及脏 tab 不能被静默丢掉。把它们放在 session 层，
//! 快闸门才能直接断它们，而不是等用户点错才发现。

use std::io;
use std::path::{Path, PathBuf};

use crate::Editor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDecision {
    Closed,
    RefusedDirty,
    InvalidIndex,
}

pub struct Tab {
    pub title: String,
    pub editor: Editor,
}

impl Tab {
    fn from_editor(editor: Editor, fallback: &str) -> Self {
        let title = editor
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| fallback.to_string());
        Self { title, editor }
    }

    pub fn is_dirty(&self) -> bool { self.editor.is_dirty() }
}

pub struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
}

impl Workspace {
    pub fn new() -> Self {
        Self { tabs: vec![Tab::from_editor(Editor::empty(), "未命名")], active: 0 }
    }

    pub fn tabs(&self) -> &[Tab] { &self.tabs }
    pub fn active_index(&self) -> usize { self.active }
    pub fn active(&self) -> &Tab { &self.tabs[self.active] }
    pub fn active_mut(&mut self) -> &mut Tab { &mut self.tabs[self.active] }

    pub fn open(&mut self, path: &Path) -> io::Result<usize> {
        if let Some((i, _)) = self.tabs.iter().enumerate().find(|(_, tab)| {
            tab.editor.path.as_deref() == Some(path)
        }) {
            self.active = i;
            return Ok(i);
        }
        self.tabs.push(Tab::from_editor(Editor::open(path)?, "未命名"));
        self.active = self.tabs.len() - 1;
        Ok(self.active)
    }

    pub fn new_file(&mut self) -> usize {
        self.tabs.push(Tab::from_editor(Editor::empty(), "未命名"));
        self.active = self.tabs.len() - 1;
        self.active
    }

    pub fn activate(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() { return false; }
        self.active = index;
        true
    }

    fn remove_at(&mut self, index: usize) -> CloseDecision {
        if index >= self.tabs.len() { return CloseDecision::InvalidIndex; }
        self.tabs.remove(index);
        if self.tabs.is_empty() { self.tabs.push(Tab::from_editor(Editor::empty(), "未命名")); }
        if self.active > index { self.active -= 1; }
        else if self.active >= self.tabs.len() { self.active = self.tabs.len() - 1; }
        CloseDecision::Closed
    }

    /// 普通关闭：脏 tab 必须先走 UI 的保存/放弃/取消选择，不能静默丢内容。
    pub fn close(&mut self, index: usize) -> CloseDecision {
        if index >= self.tabs.len() { return CloseDecision::InvalidIndex; }
        if self.tabs[index].is_dirty() { return CloseDecision::RefusedDirty; }
        self.remove_at(index)
    }

    /// 明确的“放弃并关闭”。UI 只有在用户明确点放弃时才调用它。
    pub fn discard_close(&mut self, index: usize) -> CloseDecision {
        self.remove_at(index)
    }

    pub fn close_active(&mut self) -> CloseDecision { self.close(self.active) }
    pub fn discard_close_active(&mut self) -> CloseDecision { self.discard_close(self.active) }

    pub fn paths(&self) -> Vec<Option<PathBuf>> {
        self.tabs.iter().map(|tab| tab.editor.path.clone()).collect()
    }
}

impl Default for Workspace {
    fn default() -> Self { Self::new() }
}
