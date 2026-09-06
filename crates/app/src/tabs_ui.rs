//! 顶层 tab 栏。每个 tab 拥有一份完整的 YiEdit 会话，切换时不复制文本。
//!
//! 关闭按钮暂不画：YiEdit 的 dirty 状态还需要下沉到 session::Workspace，
//! 现在画关闭按钮会诱导静默丢内容。先把安全的切换与新建接上，
//! 下一轮再接三选一脏关闭对话框。

use std::path::PathBuf;

use crate::ui::YiEdit;

pub struct TabsUi {
    tabs: Vec<YiEdit>,
    active: usize,
}

impl TabsUi {
    pub fn new(arg: Option<PathBuf>) -> Self {
        Self {
            tabs: vec![YiEdit::new(arg)],
            active: 0,
        }
    }

    fn tab_title(index: usize) -> String {
        format!("Tab {}", index + 1)
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            for i in 0..self.tabs.len() {
                let selected = i == self.active;
                let label = if selected {
                    egui::RichText::new(Self::tab_title(i)).strong()
                } else {
                    egui::RichText::new(Self::tab_title(i))
                };
                if ui.add(egui::Button::new(label).frame(selected)).clicked() {
                    self.active = i;
                }
            }
            if ui.button("+").clicked() {
                self.tabs.push(YiEdit::new(None));
                self.active = self.tabs.len() - 1;
            }
        });
    }
}

impl eframe::App for TabsUi {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.tab_bar(ui);
        eframe::App::ui(&mut self.tabs[self.active], ui, frame);
    }
}
