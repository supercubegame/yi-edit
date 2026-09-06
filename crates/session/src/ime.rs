//! IME 预编辑状态。纯状态机，不碰 GUI、不碰系统时间。
//!
//! egui 0.36 的真实事件形状：Preedit 带 `text + active_range_chars`，另有
//! DeleteSurrounding。preedit 只是候选中的临时文本，不能写进 Doc；只有 commit
//! 才是一次真编辑，进入同一套 EditOp / undo group。

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImeEvent {
    Enabled,
    Preedit {
        text: String,
        active_range_chars: Option<Range<usize>>,
    },
    Commit(String),
    DeleteSurrounding {
        before_chars: usize,
        after_chars: usize,
    },
    Disabled,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImeState {
    preedit: String,
    active_range_chars: Option<Range<usize>>,
    enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImeEffect {
    None,
    Commit(String),
    DeleteSurrounding {
        before_chars: usize,
        after_chars: usize,
    },
    ClearPreedit,
}

impl ImeState {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn preedit(&self) -> &str {
        &self.preedit
    }
    pub fn active_range_chars(&self) -> Option<Range<usize>> {
        self.active_range_chars.clone()
    }

    pub fn handle(&mut self, event: ImeEvent) -> ImeEffect {
        match event {
            ImeEvent::Enabled => {
                self.enabled = true;
                ImeEffect::None
            }
            ImeEvent::Preedit {
                text,
                active_range_chars,
            } => {
                self.enabled = true;
                self.preedit = text;
                self.active_range_chars = active_range_chars;
                ImeEffect::None
            }
            ImeEvent::Commit(text) => {
                self.preedit.clear();
                self.active_range_chars = None;
                if text.is_empty() {
                    ImeEffect::None
                } else {
                    ImeEffect::Commit(text)
                }
            }
            ImeEvent::DeleteSurrounding {
                before_chars,
                after_chars,
            } => ImeEffect::DeleteSurrounding {
                before_chars,
                after_chars,
            },
            ImeEvent::Disabled => {
                self.enabled = false;
                self.preedit.clear();
                self.active_range_chars = None;
                ImeEffect::ClearPreedit
            }
        }
    }
}
