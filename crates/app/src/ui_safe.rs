use std::path::PathBuf;
use std::sync::Arc;

use yi_edit_core::{highlight_line, Pos, SearchOptions, TokenKind};
use yi_edit_session::browser::{self, Listing};
use yi_edit_session::fontpick;
use yi_edit_session::{Editor, WELCOME};

use crate::ime_adapter::{AdapterEffect, ImeAdapter};
use crate::shot::Shot;
use crate::theme as th;

fn mono() -> egui::FontId { egui::FontId::monospace(th::FONT_SIZE) }
fn sans(size: f32) -> egui::FontId { egui::FontId::proportional(size) }

fn token_color(kind: TokenKind) -> egui::Color32 {
    match kind {
        TokenKind::Text => th::TEXT,
        TokenKind::Keyword => egui::Color32::from_rgb(255, 122, 178),
        TokenKind::Type => egui::Color32::from_rgb(90, 200, 184),
        TokenKind::Str => egui::Color32::from_rgb(255, 159, 106),
        TokenKind::Number => egui::Color32::from_rgb(168, 216, 122),
        TokenKind::Comment => egui::Color32::from_rgb(108, 140, 98),
        TokenKind::Punct => th::TEXT_DIM,
    }
}

/// 字体候选表与覆盖判断全在 `yi_edit_session::fontpick` 里，这一层只负责装。
/// 在这里再写一张候选表的话就有两份真身，而只有一份有断言守着。
pub fn install_fonts(ctx: &egui::Context) -> bool {
    let (picked, rejects) = fontpick::pick(&fontpick::candidates(), fontpick::REQUIRED);
    let Some(picked) = picked else {
        // 少了中日韩字体的话整个界面是豆腐块，而那与「正常启动」在日志里长得一模一样。
        eprintln!("FONT: 本次没有装上任何覆盖中日韩字的字体（不是「字体正常」），界面会出豆腐块");
        for (path, why) in &rejects {
            eprintln!("FONT reject {} —— {why}", path.display());
        }
        return false;
    };
    let path = picked.path.clone();
    let index = picked.index;
    let mut data = egui::FontData::from_owned(picked.bytes);
    // 集合（.ttc）里第 0 张脸未必覆盖中文，不传 index 就等于没挑。
    data.index = index;
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("cjk".into(), Arc::new(data));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("cjk".into());
    }
    ctx.set_fonts(fonts);
    eprintln!("FONT: {} index={index}", path.display());
    true
}

pub fn install_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = th::BG;
    visuals.window_fill = th::CHROME;
    visuals.widgets.inactive.bg_fill = th::CONTROL;
    visuals.widgets.hovered.bg_fill = th::CONTROL_HOVER;
    visuals.widgets.active.bg_fill = th::ACCENT;
    ctx.set_visuals(visuals);
}

pub struct YiEdit {
    ed: Editor,
    path: String,
    search: String,
    hits: Vec<Pos>,
    listing: Option<Listing>,
    show_sidebar: bool,
    ime: ImeAdapter,
    preedit: String,
    close_dialog: bool,
    force_close: bool,
    shot: Shot,
}

impl YiEdit {
    pub fn new(arg: Option<PathBuf>) -> Self {
        let (ed, path) = match arg {
            Some(p) => match Editor::open(&p) {
                Ok(e) => (e, p.to_string_lossy().into()),
                Err(_) => (Editor::empty(), p.to_string_lossy().into()),
            },
            None => (Editor::empty(), String::new()),
        };
        let mut out = Self {
            ed, path, search: String::new(), hits: Vec::new(), listing: None,
            show_sidebar: true, ime: ImeAdapter::default(), preedit: String::new(),
            close_dialog: false, force_close: false, shot: Shot::from_env(),
        };
        out.refresh();
        out
    }

    fn refresh(&mut self) {
        let dir = self.ed.path.as_ref().and_then(|p| browser::dir_for(p))
            .or_else(|| std::env::current_dir().ok());
        if let Some(dir) = dir { self.listing = browser::list_dir(&dir, false).ok(); }
    }

    fn open_file(&mut self, path: &std::path::Path) {
        if let Ok(editor) = Editor::open(path) {
            self.ed.commit_undo_group();
            self.ed = editor;
            self.path = path.to_string_lossy().into();
            self.refresh();
        }
    }

    fn insert(&mut self, text: &str) {
        if !self.ed.is_huge() { let _ = self.ed.insert_text(text); }
    }

    fn delete_surrounding(&mut self, before: usize, after: usize) {
        let mut from = self.ed.cursor;
        for _ in 0..before { from = self.ed.prev_pos(from); }
        let mut to = self.ed.cursor;
        for _ in 0..after { to = self.ed.next_pos(to); }
        if from != to {
            if let Some(doc) = self.ed.doc_mut() {
                doc.delete(from, to);
                self.ed.cursor = from;
                self.ed.invalidate_states(from.line);
            }
        }
    }

    fn handle_ime(&mut self, event: &egui::ImeEvent) {
        match self.ime.feed(event) {
            AdapterEffect::Commit(text) => { self.insert(&text); self.ed.commit_undo_group(); }
            AdapterEffect::DeleteSurrounding { before_chars, after_chars } => {
                self.delete_surrounding(before_chars, after_chars);
            }
            AdapterEffect::ClearPreedit => self.preedit.clear(),
            AdapterEffect::None => {}
        }
        self.preedit = self.ime.preedit().text;
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        for event in ctx.input(|i| i.events.clone()) {
            match event {
                egui::Event::Ime(event) => self.handle_ime(&event),
                egui::Event::Copy => { if let Some(text) = self.ed.selected_text() { ctx.copy_text(text); } }
                egui::Event::Cut => { if let Some(text) = self.ed.cut_selection() { ctx.copy_text(text); } }
                egui::Event::Paste(text) => self.insert(&text.replace("\r\n", "\n")),
                egui::Event::Text(text) => self.insert(&text),
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    if modifiers.command && key == egui::Key::A { self.ed.select_all(); }
                    else if modifiers.command && key == egui::Key::S { let _ = self.ed.save(); }
                }
                _ => {}
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(egui::RichText::new("Yi Edit").font(sans(14.0)));
            if ui.button("侧栏").clicked() { self.show_sidebar = !self.show_sidebar; }
            if ui.button("保存").clicked() { let _ = self.ed.save(); }
            ui.add_sized([300.0, 24.0], egui::TextEdit::singleline(&mut self.path));
            if ui.button("查找").clicked() {
                self.hits = self.ed.search(&self.search, SearchOptions::exact()).0;
            }
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("文件").font(sans(11.0)));
        let snapshot = self.listing.clone();
        let mut open = None;
        let mut change_dir = None;
        if let Some(listing) = snapshot {
            ui.label(listing.dir.to_string_lossy());
            for entry in listing.entries {
                let path = entry.path.clone();
                let label = if entry.is_dir { format!("{}/", entry.name) } else { entry.name };
                if ui.button(label).clicked() {
                    if entry.is_dir { change_dir = Some(path); } else { open = Some(path); }
                }
            }
        }
        if let Some(dir) = change_dir { self.listing = browser::list_dir(&dir, false).ok(); }
        if let Some(file) = open { self.open_file(&file); }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        ui.painter().rect_filled(ui.max_rect(), 0.0, th::BG);
        ui.spacing_mut().item_spacing.y = 0.0;
        let row_h = ui.text_style_height(&egui::TextStyle::Monospace).max(th::FONT_SIZE);
        let total = self.ed.line_count().max(1);
        egui::ScrollArea::both().auto_shrink([false, false]).show_rows(ui, row_h, total, |ui, rows| {
            for row in rows {
                let text = self.ed.line(row);
                let state = self.ed.state_at(row);
                let (spans, _) = highlight_line(&text, self.ed.lang, state);
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(700.0), row_h), egui::Sense::click());
                let painter = ui.painter_at(rect);
                let x = rect.min.x + th::GUTTER_W;
                painter.text(egui::pos2(x - 8.0, rect.min.y), egui::Align2::RIGHT_TOP,
                    (row + 1).to_string(), mono(), th::TEXT_DIM);
                let mut job = egui::text::LayoutJob::default();
                for span in spans {
                    job.append(&text[span.start..span.end], 0.0, egui::TextFormat {
                        font_id: mono(), color: token_color(span.kind), ..Default::default()
                    });
                }
                painter.galley(egui::pos2(x, rect.min.y), ui.painter().layout_job(job), th::TEXT);
                if row == self.ed.cursor.line && !self.preedit.is_empty() {
                    let prefix = text[..self.ed.cursor.col.min(text.len())].to_owned();
                    let cx = x + ui.painter().layout_no_wrap(prefix, mono(), th::TEXT).rect.width();
                    painter.text(egui::pos2(cx, rect.min.y), egui::Align2::LEFT_TOP,
                        &self.preedit, mono(), th::ACCENT);
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::IMERect(
                        egui::Rect::from_min_size(egui::pos2(cx, rect.min.y), egui::vec2(2.0, row_h))));
                }
                if response.clicked() { self.ed.commit_undo_group(); }
            }
        });
    }
}

impl eframe::App for YiEdit {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shot.tick(&ctx);
        self.handle_events(&ctx);

        // Keep the three bands explicit: the editor receives all remaining height.
        let full = ui.available_size();
        let toolbar_h = 36.0;
        let status_h = 28.0;
        let body_h = (full.y - toolbar_h - status_h).max(1.0);
        ui.allocate_ui_with_layout(egui::vec2(full.x, toolbar_h),
            egui::Layout::left_to_right(egui::Align::Center), |ui| self.toolbar(ui));
        ui.allocate_ui_with_layout(egui::vec2(full.x, body_h),
            egui::Layout::left_to_right(egui::Align::Min), |ui| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, th::BG);
                if self.show_sidebar {
                    let sidebar_w = th::SIDEBAR_W.min(ui.available_width().max(1.0));
                    ui.allocate_ui_with_layout(egui::vec2(sidebar_w, body_h),
                        egui::Layout::top_down(egui::Align::Min), |ui| self.sidebar(ui));
                }
                let editor_w = ui.available_width().max(1.0);
                ui.allocate_ui_with_layout(egui::vec2(editor_w, body_h),
                    egui::Layout::top_down(egui::Align::Min), |ui| self.editor(ui));
            });
        ui.allocate_ui_with_layout(egui::vec2(full.x, status_h),
            egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(self.ed.status_bar().position_text());
            });

        if ctx.input(|i| i.viewport().close_requested()) && !self.force_close && !self.shot.active() && self.ed.is_dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_dialog = true;
        }
        if self.close_dialog {
            egui::Window::new("有未保存的修改").show(&ctx, |ui| {
                if ui.button("保存并退出").clicked() { let _ = self.ed.save(); self.force_close = true; ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                if ui.button("不保存退出").clicked() { self.force_close = true; ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
                if ui.button("取消").clicked() { self.close_dialog = false; }
            });
        }
    }
}

#[allow(dead_code)]
const _WELCOME_IS_SHARED: &str = WELCOME;
