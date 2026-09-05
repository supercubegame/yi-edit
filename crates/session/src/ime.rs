//! IME 预编辑状态。纯状态机，不碰 GUI、不碰系统时间。
//!
//! 关键语义：preedit 只是候选中的临时文本，不能写进 Doc；只有 commit 才是一次
//! 真编辑，进入同一套 EditOp / undo group。否则中文输入法每次候选更新都会把
//! 半成品写进文档，Ctrl+Z 也会变成一串无法理解的碎片。

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImeEvent {
    Enabled,
    Preedit(String),
    Commit(String),
    Disabled,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImeState {
    preedit: String,
    enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImeEffect {
    None,
    Commit(String),
    ClearPreedit,
}

impl ImeState {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    /// 处理一条输入法事件。preedit 永远不产生 Commit effect。
    pub fn handle(&mut self, event: ImeEvent) -> ImeEffect {
        match event {
            ImeEvent::Enabled => {
                self.enabled = true;
                ImeEffect::None
            }
            ImeEvent::Preedit(text) => {
                self.enabled = true;
                self.preedit = text;
                ImeEffect::None
            }
            ImeEvent::Commit(text) => {
                self.preedit.clear();
                if text.is_empty() {
                    ImeEffect::None
                } else {
                    ImeEffect::Commit(text)
                }
            }
            ImeEvent::Disabled => {
                self.enabled = false;
                self.preedit.clear();
                ImeEffect::ClearPreedit
            }
        }
    }
}
