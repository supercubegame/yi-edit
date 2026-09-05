//! 编译进去的那份 UI。四个区域都在这里，而且都是真的：
//! 顶部工具栏 / 可开关的查找栏 / 中部（左侧文件面板 + 编辑区 + 右侧快速跳转面板）/ 底部状态栏。
//!
//! 上一版的问题不是代码不对，是 **PR 描述里写了四个区域而运行时只有三个**：
//! `jump.rs` 的断言全部通过，而它的输出从没被画到屏幕上 —— 一层没人走的逻辑
//! 配上一整套绿的断言，看起来比没写还像做完了。现在面板真的在画，
//! 而且截图检查器多了一个竖带检查盯着它（横带对「右侧整条没画」毫无意见）。

use std::path::PathBuf;
use std::sync::Arc;

use yi_edit_core::{highlight_line, Pos, SearchOptions, TokenKind};
use yi_edit_session::browser::{self, Listing};
use yi_edit_session::fontpick;
use yi_edit_session::jump::JumpMap;
use yi_edit_session::{Editor, WELCOME};

use crate::ime_adapter::{AdapterEffect, ImeAdapter};
use crate::shot::Shot;
use crate::theme as th;

fn mono() -> egui::FontId {
    egui::FontId::monospace(th::FONT_SIZE)
}

fn sans(size: f32) -> egui::FontId {
    egui::FontId::proportional(size)
}

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
    replace: String,
    hits: Vec<Pos>,
    hit_index: usize,
    /// 命中是不是被上限截断了。静默截断与「就这么多」在界面上一模一样，所以要明写。
    truncated: bool,
    listing: Option<Listing>,
    show_sidebar: bool,
    show_find: bool,
    /// 编辑区当前的首行与可见行数，由 `show_rows` 回写；跳转面板的可见窗口指示器靠它。
    first_visible: usize,
    visible_rows: usize,
    /// 下一帧要滚到哪一行。
    scroll_to: Option<usize>,
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
            ed,
            path,
            search: String::new(),
            replace: String::new(),
            hits: Vec::new(),
            hit_index: 0,
            truncated: false,
            listing: None,
            show_sidebar: true,
            show_find: false,
            first_visible: 0,
            visible_rows: 1,
            scroll_to: None,
            ime: ImeAdapter::default(),
            preedit: String::new(),
            close_dialog: false,
            force_close: false,
            shot: Shot::from_env(),
        };
        out.refresh();
        out
    }

    fn refresh(&mut self) {
        let dir = self
            .ed
            .path
            .as_ref()
            .and_then(|p| browser::dir_for(p))
            .or_else(|| std::env::current_dir().ok());
        if let Some(dir) = dir {
            self.listing = browser::list_dir(&dir, false).ok();
        }
    }

    fn open_file(&mut self, path: &std::path::Path) {
        if let Ok(editor) = Editor::open(path) {
            self.ed.commit_undo_group();
            self.ed = editor;
            self.path = path.to_string_lossy().into();
            self.hits.clear();
            self.hit_index = 0;
            self.truncated = false;
            self.scroll_to = Some(0);
            self.refresh();
        }
    }

    fn insert(&mut self, text: &str) {
        if !self.ed.is_huge() {
            let _ = self.ed.insert_text(text);
            self.ensure_cursor_visible();
        }
    }

    fn ensure_cursor_visible(&mut self) {
        let line = self.ed.cursor.line;
        let last = self.first_visible + self.visible_rows.max(1);
        if line < self.first_visible || line + 1 >= last {
            self.scroll_to = Some(line.saturating_sub(self.visible_rows / 3));
        }
    }

    fn backspace(&mut self) {
        if self.ed.is_huge() {
            return;
        }
        if self.ed.selection().is_some() {
            let _ = self.ed.cut_selection();
            self.ensure_cursor_visible();
            return;
        }
        let to = self.ed.cursor;
        let from = self.ed.prev_pos(to);
        if from == to {
            return;
        }
        if let Some(doc) = self.ed.doc_mut() {
            doc.delete(from, to);
        }
        self.ed.cursor = from;
        self.ed.anchor = None;
        self.ed.invalidate_states(from.line);
        self.ensure_cursor_visible();
    }

    fn delete_forward(&mut self) {
        if self.ed.is_huge() {
            return;
        }
        if self.ed.selection().is_some() {
            let _ = self.ed.cut_selection();
            return;
        }
        let from = self.ed.cursor;
        let to = self.ed.next_pos(from);
        if from == to {
            return;
        }
        if let Some(doc) = self.ed.doc_mut() {
            doc.delete(from, to);
        }
        self.ed.cursor = from;
        self.ed.anchor = None;
        self.ed.invalidate_states(from.line);
    }

    /// 光标移动。带 Shift 时建立 / 延伸选区，不带时清选区。
    fn move_cursor(&mut self, key: egui::Key, shift: bool) {
        if shift {
            if self.ed.anchor.is_none() {
                self.ed.anchor = Some(self.ed.cursor);
            }
        } else {
            self.ed.anchor = None;
        }
        let cur = self.ed.cursor;
        let page = self.visible_rows.max(1);
        let next = match key {
            egui::Key::ArrowLeft => self.ed.prev_pos(cur),
            egui::Key::ArrowRight => self.ed.next_pos(cur),
            egui::Key::ArrowUp => Pos::new(cur.line.saturating_sub(1), cur.col),
            egui::Key::ArrowDown => Pos::new(cur.line + 1, cur.col),
            egui::Key::PageUp => Pos::new(cur.line.saturating_sub(page), cur.col),
            egui::Key::PageDown => Pos::new(cur.line + page, cur.col),
            egui::Key::Home => Pos::new(cur.line, 0),
            egui::Key::End => Pos::new(cur.line, self.ed.line(cur.line).len()),
            _ => cur,
        };
        self.ed.cursor = self.ed.clamp(next);
        // 光标一动就封口撤销组：否则在另一处敲的字会被归进上一个词的组里。
        self.ed.commit_undo_group();
        self.ensure_cursor_visible();
    }

    fn run_search(&mut self) {
        let (hits, truncated) = self.ed.search(&self.search, SearchOptions::exact());
        self.hits = hits;
        self.truncated = truncated;
        self.hit_index = 0;
        if let Some(first) = self.hits.first().copied() {
            self.ed.cursor = self.ed.clamp(first);
            self.ensure_cursor_visible();
        }
    }

    fn goto_hit(&mut self, forward: bool) {
        if self.hits.is_empty() {
            return;
        }
        let n = self.hits.len();
        self.hit_index = if forward {
            (self.hit_index + 1) % n
        } else {
            (self.hit_index + n - 1) % n
        };
        let pos = self.hits[self.hit_index];
        self.ed.cursor = self.ed.clamp(pos);
        self.ed.commit_undo_group();
        self.ensure_cursor_visible();
    }

    fn delete_surrounding(&mut self, before: usize, after: usize) {
        let mut from = self.ed.cursor;
        for _ in 0..before {
            from = self.ed.prev_pos(from);
        }
        let mut to = self.ed.cursor;
        for _ in 0..after {
            to = self.ed.next_pos(to);
        }
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
            AdapterEffect::Commit(text) => {
                self.insert(&text);
                self.ed.commit_undo_group();
            }
            AdapterEffect::DeleteSurrounding {
                before_chars,
                after_chars,
            } => self.delete_surrounding(before_chars, after_chars),
            AdapterEffect::ClearPreedit => self.preedit.clear(),
            AdapterEffect::None => {}
        }
        self.preedit = self.ime.preedit().text;
    }

    /// 键盘与剪贴板。
    ///
    /// **先问有没有控件拿着焦点。** 不问的话，在查找框里敲的每一个字会同时被写进文档，
    /// 而这个 bug 不会报错：用户搜完一次之后文件里多了一串垃圾。
    fn handle_events(&mut self, ctx: &egui::Context) {
        let editor_focused = ctx.memory(|m| m.focused()).is_none();
        for event in ctx.input(|i| i.events.clone()) {
            match event {
                egui::Event::Ime(event) if editor_focused => self.handle_ime(&event),
                egui::Event::Copy => {
                    if let Some(text) = self.ed.selected_text() {
                        ctx.copy_text(text);
                    }
                }
                egui::Event::Cut if editor_focused => {
                    if let Some(text) = self.ed.cut_selection() {
                        ctx.copy_text(text);
                    }
                }
                egui::Event::Paste(text) if editor_focused => {
                    self.insert(&text.replace("\r\n", "\n"))
                }
                egui::Event::Text(text) if editor_focused => self.insert(&text),
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => self.handle_key(key, modifiers, editor_focused),
                _ => {}
            }
        }
    }

    fn handle_key(&mut self, key: egui::Key, modifiers: egui::Modifiers, editor_focused: bool) {
        if modifiers.command {
            match key {
                egui::Key::S => {
                    if let Err(e) = self.ed.save() {
                        self.ed.status = format!("保存失败：{e}");
                    }
                }
                egui::Key::B => self.show_sidebar = !self.show_sidebar,
                egui::Key::F => self.show_find = !self.show_find,
                egui::Key::O => {
                    let path = PathBuf::from(self.path.clone());
                    self.open_file(&path);
                }
                egui::Key::A if editor_focused => self.ed.select_all(),
                egui::Key::Z if editor_focused => {
                    if modifiers.shift {
                        self.ed.redo();
                    } else {
                        self.ed.undo();
                    }
                    self.ensure_cursor_visible();
                }
                egui::Key::Y if editor_focused => {
                    self.ed.redo();
                    self.ensure_cursor_visible();
                }
                _ => {}
            }
            return;
        }
        match key {
            // 查找框里的 Enter 是「下一个匹配」；编辑区里的 Enter 是换行。
            egui::Key::Enter if !editor_focused => self.goto_hit(!modifiers.shift),
            egui::Key::Enter => self.insert("\n"),
            egui::Key::Tab if editor_focused => self.insert("    "),
            egui::Key::Backspace if editor_focused => self.backspace(),
            egui::Key::Delete if editor_focused => self.delete_forward(),
            egui::Key::Escape => self.show_find = false,
            egui::Key::ArrowLeft
            | egui::Key::ArrowRight
            | egui::Key::ArrowUp
            | egui::Key::ArrowDown
            | egui::Key::PageUp
            | egui::Key::PageDown
            | egui::Key::Home
            | egui::Key::End
                if editor_focused =>
            {
                self.move_cursor(key, modifiers.shift)
            }
            _ => {}
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, th::CHROME);
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Yi Edit").font(sans(14.0)).color(th::TEXT));
            if ui.button("侧栏").clicked() {
                self.show_sidebar = !self.show_sidebar;
            }
            if ui.button("查找").clicked() {
                self.show_find = !self.show_find;
            }
            if ui.button("打开").clicked() {
                let path = PathBuf::from(self.path.clone());
                self.open_file(&path);
            }
            if ui.button("保存").clicked() {
                if let Err(e) = self.ed.save() {
                    self.ed.status = format!("保存失败：{e}");
                }
            }
            if ui
                .add_enabled(self.ed.can_undo(), egui::Button::new("撤销"))
                .clicked()
            {
                self.ed.undo();
            }
            if ui
                .add_enabled(self.ed.can_redo(), egui::Button::new("重做"))
                .clicked()
            {
                self.ed.redo();
            }
            ui.add_sized(
                [360.0, 24.0],
                egui::TextEdit::singleline(&mut self.path).hint_text("文件路径"),
            );
            let bar = self.ed.status_bar();
            ui.label(egui::RichText::new(bar.name).font(sans(12.0)).color(th::TEXT_DIM));
        });
        ui.painter().hline(
            rect.x_range(),
            rect.max.y - 0.5,
            egui::Stroke::new(1.0, th::HAIRLINE),
        );
    }

    fn find_bar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, th::CHROME);
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("查找").font(sans(11.0)).color(th::TEXT_DIM));
            let changed = ui
                .add_sized([220.0, 22.0], egui::TextEdit::singleline(&mut self.search))
                .changed();
            if changed {
                self.run_search();
            }
            if ui.button("上一个").clicked() {
                self.goto_hit(false);
            }
            if ui.button("下一个").clicked() {
                self.goto_hit(true);
            }
            ui.label(egui::RichText::new("替换为").font(sans(11.0)).color(th::TEXT_DIM));
            ui.add_sized([180.0, 22.0], egui::TextEdit::singleline(&mut self.replace));
            if ui.button("全部替换").clicked() {
                match self
                    .ed
                    .replace_all(&self.search, &self.replace, SearchOptions::exact())
                {
                    Ok(n) => self.ed.status = format!("已替换 {n} 处"),
                    Err(e) => self.ed.status = format!("替换失败：{e}"),
                }
                self.run_search();
            }
            // 截断必须明写：静默截断与「就这么多命中」在界面上一模一样。
            let text = if self.hits.is_empty() {
                String::from("无命中")
            } else if self.truncated {
                format!(
                    "命中 {} 处（已达上限，结果不完整）/ 当前第 {}",
                    self.hits.len(),
                    self.hit_index + 1
                )
            } else {
                format!(
                    "命中 {} 处 / 当前第 {}",
                    self.hits.len(),
                    self.hit_index + 1
                )
            };
            ui.label(egui::RichText::new(text).font(sans(11.0)).color(th::TEXT_DIM));
        });
        ui.painter().hline(
            rect.x_range(),
            rect.max.y - 0.5,
            egui::Stroke::new(1.0, th::HAIRLINE),
        );
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, th::CHROME);
        ui.painter().vline(
            rect.max.x - 0.5,
            rect.y_range(),
            egui::Stroke::new(1.0, th::HAIRLINE),
        );
        ui.add_space(6.0);
        ui.label(egui::RichText::new("文件").font(sans(11.0)).color(th::TEXT_DIM));
        // 先 clone 快照：循环里要改 self.listing，而边迭代边改是借用冲突。
        let snapshot = self.listing.clone();
        let mut open = None;
        let mut change_dir = None;
        if let Some(listing) = snapshot {
            ui.label(
                egui::RichText::new(listing.dir.to_string_lossy())
                    .font(sans(10.0))
                    .color(th::TEXT_DIM),
            );
            let current = self.ed.path.clone();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in listing.entries {
                        let path = entry.path.clone();
                        let is_current = current.as_deref() == Some(path.as_path());
                        let label = if entry.is_dir {
                            format!("{}/", entry.name)
                        } else {
                            entry.name.clone()
                        };
                        let text = if is_current {
                            egui::RichText::new(label).color(th::ACCENT)
                        } else {
                            egui::RichText::new(label).color(th::TEXT)
                        };
                        if ui.add(egui::Button::new(text).frame(is_current)).clicked() {
                            if entry.is_dir {
                                change_dir = Some(path);
                            } else {
                                open = Some(path);
                            }
                        }
                    }
                });
        }
        if let Some(dir) = change_dir {
            self.listing = browser::list_dir(&dir, false).ok();
        }
        if let Some(file) = open {
            self.open_file(&file);
        }
    }

    /// 行内字节列 -> 像素 x。列先夹到字符边界，否则中文行上一切就 panic。
    fn x_of(ui: &egui::Ui, text: &str, col: usize, x0: f32) -> f32 {
        let mut c = col.min(text.len());
        while c > 0 && !text.is_char_boundary(c) {
            c -= 1;
        }
        x0 + ui
            .painter()
            .layout_no_wrap(text[..c].to_owned(), mono(), th::TEXT)
            .rect
            .width()
    }

    /// 像素 x -> 行内字节列。只在点击时跑，所以逐字符量宽度可以接受。
    fn col_at_x(ui: &egui::Ui, text: &str, x0: f32, target: f32) -> usize {
        let mut best = 0usize;
        for (i, _) in text.char_indices() {
            if Self::x_of(ui, text, i, x0) <= target {
                best = i;
            } else {
                return best;
            }
        }
        if Self::x_of(ui, text, text.len(), x0) <= target {
            best = text.len();
        }
        best
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        ui.painter().rect_filled(ui.max_rect(), 0.0, th::BG);
        ui.spacing_mut().item_spacing.y = 0.0;
        let row_h = ui
            .text_style_height(&egui::TextStyle::Monospace)
            .max(th::FONT_SIZE);
        let total = self.ed.line_count().max(1);
        let mut area = egui::ScrollArea::both().auto_shrink([false, false]);
        if let Some(line) = self.scroll_to.take() {
            area = area.vertical_scroll_offset(line as f32 * row_h);
        }
        let needle_len = self.search.len();
        let selection = self.ed.selection();
        area.show_rows(ui, row_h, total, |ui, rows| {
            self.first_visible = rows.start;
            self.visible_rows = rows.len().max(1);
            for row in rows {
                let text = self.ed.line(row);
                let state = self.ed.state_at(row);
                let (spans, _) = highlight_line(&text, self.ed.lang, state);
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width().max(700.0), row_h),
                    egui::Sense::click(),
                );
                let painter = ui.painter_at(rect);
                let x = rect.min.x + th::GUTTER_W;
                let is_cursor_row = row == self.ed.cursor.line;
                if is_cursor_row {
                    painter.rect_filled(rect, 0.0, th::CURRENT_LINE);
                }
                // 选区。跳行时末行以外都画到行尾。
                if let Some((a, b)) = selection {
                    if row >= a.line && row <= b.line {
                        let from = if row == a.line { a.col } else { 0 };
                        let to = if row == b.line { b.col } else { text.len() };
                        let x1 = Self::x_of(ui, &text, from, x);
                        let x2 = Self::x_of(ui, &text, to, x).max(x1 + 2.0);
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(x1, rect.min.y),
                                egui::pos2(x2, rect.max.y),
                            ),
                            0.0,
                            th::SELECTION,
                        );
                    }
                }
                // 搜索命中。
                if needle_len > 0 {
                    for hit in self.hits.iter().filter(|h| h.line == row) {
                        let x1 = Self::x_of(ui, &text, hit.col, x);
                        let x2 = Self::x_of(ui, &text, hit.col + needle_len, x).max(x1 + 2.0);
                        painter.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2(x1, rect.min.y),
                                egui::pos2(x2, rect.max.y),
                            ),
                            0.0,
                            th::MATCH,
                        );
                    }
                }
                let gutter_color = if is_cursor_row {
                    th::GUTTER_TEXT_ACTIVE
                } else {
                    th::GUTTER_TEXT
                };
                painter.text(
                    egui::pos2(x - 8.0, rect.min.y),
                    egui::Align2::RIGHT_TOP,
                    (row + 1).to_string(),
                    mono(),
                    gutter_color,
                );
                let mut job = egui::text::LayoutJob::default();
                for span in spans {
                    job.append(
                        &text[span.start..span.end],
                        0.0,
                        egui::TextFormat {
                            font_id: mono(),
                            color: token_color(span.kind),
                            ..Default::default()
                        },
                    );
                }
                painter.galley(
                    egui::pos2(x, rect.min.y),
                    ui.painter().layout_job(job),
                    th::TEXT,
                );
                if is_cursor_row {
                    let cx = Self::x_of(ui, &text, self.ed.cursor.col, x);
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(cx, rect.min.y + 1.0),
                            egui::vec2(2.0, row_h - 2.0),
                        ),
                        0.0,
                        th::CARET,
                    );
                    if !self.preedit.is_empty() {
                        painter.text(
                            egui::pos2(cx, rect.min.y),
                            egui::Align2::LEFT_TOP,
                            &self.preedit,
                            mono(),
                            th::ACCENT,
                        );
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::IMERect(
                                egui::Rect::from_min_size(
                                    egui::pos2(cx, rect.min.y),
                                    egui::vec2(2.0, row_h),
                                ),
                            ));
                    }
                }
                if response.clicked() {
                    if let Some(p) = response.interact_pointer_pos() {
                        let col = Self::col_at_x(ui, &text, x, p.x);
                        self.ed.cursor = Pos::new(row, col);
                        self.ed.anchor = None;
                        self.ed.commit_undo_group();
                    }
                }
            }
        });
    }

    /// 右侧快速跳转面板。坐标换算全走 `JumpMap`（整数 + 二分），
    /// 这里只负责画与派发点击。
    fn jump_panel(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, th::CHROME);
        painter.vline(
            rect.min.x + 0.5,
            rect.y_range(),
            egui::Stroke::new(1.0, th::HAIRLINE),
        );
        painter.text(
            egui::pos2(rect.center().x, rect.min.y + 2.0),
            egui::Align2::CENTER_TOP,
            "跳转",
            sans(10.0),
            th::TEXT_DIM,
        );
        let top = rect.min.y + 18.0;
        let usable = (rect.max.y - top).floor();
        let lines = self.ed.line_count();
        let Some(map) = JumpMap::new(usable.max(0.0) as u32, lines) else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "面板太短",
                sans(10.0),
                th::TEXT_DIM,
            );
            return;
        };
        let h = map.height_px();
        let bar_x = rect.min.x + 6.0;
        let bar_w = rect.width() - 12.0;

        // 内容缩略只对内存模式画：大文件模式下每帧扇全文是不可接受的，
        // 这条限制写在 docs/PITFALLS.md 里，不假装它不存在。
        if self.ed.is_huge() {
            for k in 0..=4u32 {
                let y = top + (h as f32) * (k as f32 / 4.0);
                let line = map.line_at((h.saturating_sub(1)) * k / 4);
                painter.hline(
                    bar_x..=bar_x + bar_w,
                    y,
                    egui::Stroke::new(1.0, th::HAIRLINE),
                );
                painter.text(
                    egui::pos2(bar_x, y + 1.0),
                    egui::Align2::LEFT_TOP,
                    format!("{}", line + 1),
                    sans(9.0),
                    th::TEXT_DIM,
                );
            }
        } else {
            for y in 0..h {
                let line = map.line_at(y);
                let len = self.ed.line(line).chars().count().min(80) as f32 / 80.0;
                if len <= 0.0 {
                    continue;
                }
                // 颜色随行长变化，于是缩略图看得出代码的形状而不是一堆同色短线。
                let shade = 90 + (len * 90.0) as u8;
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(bar_x, top + y as f32),
                        egui::vec2((bar_w * len).max(1.0), 1.0),
                    ),
                    0.0,
                    egui::Color32::from_rgb(shade, shade, shade + 4),
                );
            }
        }

        // 搜索命中。
        for hit in &self.hits {
            if let Some((band_top, band_bottom)) = map.line_band(hit.line) {
                let height = (band_bottom.saturating_sub(band_top)).max(1) as f32;
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(bar_x, top + band_top as f32),
                        egui::vec2(bar_w, height),
                    ),
                    0.0,
                    th::MATCH,
                );
            }
        }

        // 可见窗口指示器。至少一像素高，否则在百万行文件上它会完全消失。
        let (v_top, v_bottom) = map.viewport_band(self.first_visible, self.visible_rows);
        painter.rect_filled(
            egui::Rect::from_min_size(
                egui::pos2(rect.min.x + 2.0, top + v_top as f32),
                egui::vec2(
                    rect.width() - 4.0,
                    (v_bottom.saturating_sub(v_top)).max(1) as f32,
                ),
            ),
            th::RADIUS,
            egui::Color32::from_rgba_unmultiplied(10, 132, 255, 60),
        );
        // 当前行。
        if let Some((cur_top, _)) = map.line_band(self.ed.cursor.line) {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(rect.min.x + 2.0, top + cur_top as f32),
                    egui::vec2(rect.width() - 4.0, 2.0),
                ),
                0.0,
                th::CARET,
            );
        }

        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
        if let Some(p) = response.interact_pointer_pos() {
            let y = (p.y - top).max(0.0) as u32;
            let line = map.line_at(y.min(h.saturating_sub(1)));
            self.ed.cursor = self.ed.clamp(Pos::new(line, 0));
            self.scroll_to = Some(line.saturating_sub(self.visible_rows / 3));
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, th::CHROME);
        ui.painter().hline(
            rect.x_range(),
            rect.min.y + 0.5,
            egui::Stroke::new(1.0, th::HAIRLINE),
        );
        let bar = self.ed.status_bar();
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            for text in [bar.name.clone(), bar.position_text(), bar.size_text()] {
                ui.label(egui::RichText::new(text).font(sans(11.0)).color(th::TEXT));
                ui.add_space(4.0);
            }
            for badge in bar.badges() {
                ui.label(
                    egui::RichText::new(badge)
                        .font(sans(11.0))
                        .color(th::TEXT_DIM),
                );
            }
        });
    }
}

impl eframe::App for YiEdit {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shot.tick(&ctx);
        self.handle_events(&ctx);

        // 各区域的矩形显式算出来：编辑区拿到的是**剩下的全部**高度，
        // 这正是上一轮底部留白那个 bug 的根因。
        let full = ui.available_size();
        let find_h = if self.show_find { th::FINDBAR_H } else { 0.0 };
        let body_h = (full.y - th::TOOLBAR_H - find_h - th::STATUS_H).max(1.0);
        ui.allocate_ui_with_layout(
            egui::vec2(full.x, th::TOOLBAR_H),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| self.toolbar(ui),
        );
        if self.show_find {
            ui.allocate_ui_with_layout(
                egui::vec2(full.x, th::FINDBAR_H),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| self.find_bar(ui),
            );
        }
        ui.allocate_ui_with_layout(
            egui::vec2(full.x, body_h),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, th::BG);
                if self.show_sidebar {
                    let w = th::SIDEBAR_W.min(ui.available_width().max(1.0));
                    ui.allocate_ui_with_layout(
                        egui::vec2(w, body_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.sidebar(ui),
                    );
                }
                let jump_w = th::JUMP_W.min((ui.available_width() - 40.0).max(0.0));
                let editor_w = (ui.available_width() - jump_w).max(1.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(editor_w, body_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.editor(ui),
                );
                if jump_w > 0.0 {
                    ui.allocate_ui_with_layout(
                        egui::vec2(jump_w, body_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.jump_panel(ui),
                    );
                }
            },
        );
        ui.allocate_ui_with_layout(
            egui::vec2(full.x, th::STATUS_H),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| self.status_bar(ui),
        );

        if ctx.input(|i| i.viewport().close_requested())
            && !self.force_close
            && !self.shot.active()
            && self.ed.is_dirty()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_dialog = true;
        }
        if self.close_dialog {
            egui::Window::new("有未保存的修改").show(&ctx, |ui| {
                if ui.button("保存并退出").clicked() {
                    let _ = self.ed.save();
                    self.force_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("不保存退出").clicked() {
                    self.force_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button("取消").clicked() {
                    self.close_dialog = false;
                }
            });
        }
    }
}

#[allow(dead_code)]
const _WELCOME_IS_SHARED: &str = WELCOME;
