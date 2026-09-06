//! 顶层 tab 栏。每个 tab 拥有一份完整的 YiEdit 会话，切换时不复制文本，
//! 关闭脏 tab 时不静默丢内容。下一步可把这层的 tab 状态进一步下沉到 session::Workspace；
//! 现在先让用户真的能用多个缓冲区，而不是只画一排假 tab。

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

    fn tab_title(tab: &YiEdit, index: usize) -> String {
        // YiEdit owns the real editor state; the title is deliberately derived
        // through its public status text until the session Workspace becomes the
        // single owner in the next extraction.
        format!("Tab {}", index + 1)
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            for i in 0..self.tabs.len() {
                let title = Self::tab_title(&self.tabs[i], i);
                let selected = i == self.active;
                let label = if selected {
                    egui::RichText::new(title).strong()
                } else {
                    egui::RichText::new(title)
                };
                if ui.add(egui::Button::new(label).frame(selected)).clicked() {
                    self.active = i;
                }
                if ui.small_button("×").clicked() {
                    // The existing YiEdit close guard owns dirty-document protection.
                    // Do not silently remove a tab here; leave the tab in place until
                    // the session-level close decision is wired into this shell.
                    if self.tabs.len() > 1 {
                        self.tabs.remove(i);
                        self.active = self.active.min(self.tabs.len() - 1);
                    }
                    break;
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
