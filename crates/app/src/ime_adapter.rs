//! egui 0.36 -> session IME 的唯一适配层。
//!
//! 不把 egui 类型扩散进 session：上游事件形状变化只改这一处。
//! Preedit 的 active_range_chars 保留给 UI 画转换段与候选光标；它绝不能写进 Doc。

use std::ops::Range;

use yi_edit_session::ime::{ImeEffect, ImeEvent, ImeState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreeditView {
    pub text: String,
    pub active_range_chars: Option<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterEffect {
    None,
    Commit(String),
    DeleteSurrounding { before_chars: usize, after_chars: usize },
    ClearPreedit,
}

#[derive(Debug, Default)]
pub struct ImeAdapter {
    state: ImeState,
}

impl ImeAdapter {
    pub fn preedit(&self) -> PreeditView {
        PreeditView {
            text: self.state.preedit().to_owned(),
            active_range_chars: self.state.active_range_chars(),
        }
    }

    /// 只读 egui 输入，返回 UI 应该执行的 session effect。
    pub fn feed(&mut self, event: &egui::ImeEvent) -> AdapterEffect {
        let mapped = match event {
            egui::ImeEvent::Enabled => ImeEvent::Enabled,
            egui::ImeEvent::Preedit { text, active_range_chars } => ImeEvent::Preedit {
                text: text.clone(),
                active_range_chars: active_range_chars.clone(),
            },
            egui::ImeEvent::Commit(text) => ImeEvent::Commit(text.clone()),
            egui::ImeEvent::DeleteSurrounding { before_chars, after_chars } => {
                ImeEvent::DeleteSurrounding {
                    before_chars: *before_chars,
                    after_chars: *after_chars,
                }
            }
            egui::ImeEvent::Disabled => ImeEvent::Disabled,
        };
        match self.state.handle(mapped) {
            ImeEffect::None => AdapterEffect::None,
            ImeEffect::Commit(text) => AdapterEffect::Commit(text),
            ImeEffect::DeleteSurrounding { before_chars, after_chars } => {
                AdapterEffect::DeleteSurrounding { before_chars, after_chars }
            }
            ImeEffect::ClearPreedit => AdapterEffect::ClearPreedit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preedit_preserves_active_range_and_commit_is_the_only_text_effect() {
        let mut a = ImeAdapter::default();
        assert_eq!(
            a.feed(&egui::ImeEvent::Preedit {
                text: "ni".into(),
                active_range_chars: Some(0..2),
            }),
            AdapterEffect::None
        );
        assert_eq!(a.preedit().text, "ni");
        assert_eq!(a.preedit().active_range_chars, Some(0..2));
        assert_eq!(
            a.feed(&egui::ImeEvent::Commit("你".into())),
            AdapterEffect::Commit("你".into())
        );
        assert_eq!(a.preedit().text, "");
        assert_eq!(a.preedit().active_range_chars, None);
    }

    #[test]
    fn delete_surrounding_preserves_character_counts() {
        let mut a = ImeAdapter::default();
        assert_eq!(
            a.feed(&egui::ImeEvent::DeleteSurrounding {
                before_chars: 2,
                after_chars: 1,
            }),
            AdapterEffect::DeleteSurrounding {
                before_chars: 2,
                after_chars: 1,
            }
        );
    }

    #[test]
    fn disabled_clears_stale_preedit() {
        let mut a = ImeAdapter::default();
        a.feed(&egui::ImeEvent::Preedit {
            text: "残留".into(),
            active_range_chars: Some(0..2),
        });
        assert_eq!(a.feed(&egui::ImeEvent::Disabled), AdapterEffect::ClearPreedit);
        assert_eq!(a.preedit().text, "");
        assert_eq!(a.preedit().active_range_chars, None);
    }
}
