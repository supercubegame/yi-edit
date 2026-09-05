//! 界面层。只负责画和收键，不放任何搜索/替换/高亮的真逻辑。
//!
//! 不用面板（Panel）：eframe 0.36 的 `App::ui` 直接给一个覆盖整窗口的 `Ui`，
//! 在它上面排版就够了。少用两个容易随版本漂的 API，换掉两个风险点。

use std::path::PathBuf;
use std::sync::Arc;

use yi_edit_core::{highlight_line, Pos, SearchOptions, TokenKind};

use crate::editor::{Editor, WELCOME};
use crate::shot::Shot;

const FONT_SIZE: f32 = 14.0;
/// 行号栏宽度。与下面 `LINE_NO_WIDTH` 耦合：改一个必须重算另一个，
/// 否则行号会和正文叠在一起（而这一点只有看截图才发现得了）。
const GUTTER: f32 = 70.0;
const LINE_NO_WIDTH: usize = 6;

fn mono() -> egui::FontId {
    egui::FontId::monospace(FONT_SIZE)
}

fn color_for(kind: TokenKind) -> egui::Color32 {
    match kind {
        TokenKind::Text => egui::Color32::from_rgb(220, 223, 228),
        TokenKind::Keyword => egui::Color32::from_rgb(197, 134, 192),
        TokenKind::Type => egui::Color32::from_rgb(78, 201, 176),
        TokenKind::Str => egui::Color32::from_rgb(206, 145, 120),
        TokenKind::Number => egui::Color32::from_rgb(181, 206, 168),
        TokenKind::Comment => egui::Color32::from_rgb(106, 153, 85),
        TokenKind::Punct => egui::Color32::from_rgb(160, 165, 175),
    }
}

/// 装中文字体。只接受 .ttf/.otf：字体集合（.ttc）在部分环境下会直接 panic，
/// 而一个启动就挂的编辑器比一个显示豆子块的编辑器糟得多。
/// 找不到也不假装成功：在 stderr 里大声说，CI 报告里会带上这一行。
pub fn install_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
        "/usr/share/fonts/truetype/arphic/uming.ttf",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\Deng.ttf",
    ];
    let mut found: Option<(&str, Vec<u8>)> = None;
    for path in CANDIDATES {
        if let Ok(bytes) = std::fs::read(path) {
            if bytes.len() > 4096 {
                found = Some((path, bytes));
                break;
            }
        }
    }
    let Some((path, bytes)) = found else {
        eprintln!("FONT: 没找到可用的中文字体，中文会显示为豆子块");
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "cjk".to_owned(),
        Arc::new(egui::FontData::from_owned(bytes)),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("cjk".to_owned());
    }
    ctx.set_fonts(fonts);
    eprintln!("FONT: 使用 {path}");
}

pub struct YiEdit {
    ed: Editor,
    path_input: String,
    search: String,
    replace: String,
    case_sensitive: bool,
    whole_word: bool,
    hits: Vec<Pos>,
    hits_truncated: bool,
    hit_idx: usize,
    scroll_to: Option<usize>,
    focus_search: bool,
    editor_has_keys: bool,
    shot: Shot,
}

impl YiEdit {
    pub fn new(arg: Option<PathBuf>) -> Self {
        let (ed, path_input) = match arg {
            Some(p) => match Editor::open(&p) {
                Ok(e) => {
                    let s = p.to_string_lossy().to_string();
                    (e, s)
                }
                Err(e) => {
                    let mut ed = Editor::empty();
                    ed.status = format!("打不开 {}：{e}", p.display());
                    (ed, p.to_string_lossy().to_string())
                }
            },
            None => (Editor::empty(), String::new()),
        };
        Self {
            ed,
            path_input,
            search: String::new(),
            replace: String::new(),
            case_sensitive: false,
            whole_word: false,
            hits: Vec::new(),
            hits_truncated: false,
            hit_idx: 0,
            scroll_to: None,
            focus_search: false,
            editor_has_keys: true,
            shot: Shot::from_env(),
        }
    }

    fn opts(&self) -> SearchOptions {
        SearchOptions {
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
        }
    }

    fn open_path(&mut self) {
        let p = PathBuf::from(self.path_input.trim());
        if p.as_os_str().is_empty() {
            self.ed.status = String::from("路径是空的");
            return;
        }
        match Editor::open(&p) {
            Ok(e) => {
                self.ed = e;
                self.hits.clear();
                self.hit_idx = 0;
                self.scroll_to = Some(0);
            }
            Err(e) => self.ed.status = format!("打不开 {}：{e}", p.display()),
        }
    }

    fn do_search(&mut self) {
        let needle = self.search.clone();
        let opts = self.opts();
        let (hits, truncated) = self.ed.search(&needle, opts);
        self.hits = hits;
        self.hits_truncated = truncated;
        self.hit_idx = 0;
        if !self.hits.is_empty() {
            let target = self.hits[0];
            self.ed.cursor = self.ed.clamp(target);
            self.ed.anchor = None;
            self.scroll_to = Some(target.line);
        }
    }

    fn goto_hit(&mut self, forward: bool) {
        if self.hits.is_empty() {
            return;
        }
        let n = self.hits.len();
        self.hit_idx = if forward {
            (self.hit_idx + 1) % n
        } else {
            (self.hit_idx + n - 1) % n
        };
        let target = self.hits[self.hit_idx];
        self.ed.cursor = self.ed.clamp(target);
        self.ed.anchor = None;
        self.scroll_to = Some(target.line);
    }

    fn do_replace_all(&mut self) {
        let needle = self.search.clone();
        let repl = self.replace.clone();
        let opts = self.opts();
        match self.ed.replace_all(&needle, &repl, opts) {
            Ok(n) => {
                self.ed.status = format!("已替换 {n} 处");
                self.do_search();
            }
            Err(e) => self.ed.status = format!("替换失败：{e}"),
        }
    }

    fn delete_selection(&mut self) -> bool {
        let Some((a, b)) = self.ed.selection() else {
            return false;
        };
        if let Some(d) = self.ed.doc_mut() {
            d.delete(a, b);
            self.ed.cursor = a;
            self.ed.anchor = None;
            self.ed.invalidate_states(a.line);
            return true;
        }
        false
    }

    fn insert_text(&mut self, text: &str) {
        if self.ed.is_huge() {
            self.ed.status =
                String::from("大文件为只读模式：V1 不在超过 64 MB 的文件上做内存编辑");
            return;
        }
        self.delete_selection();
        let at = self.ed.cursor;
        if let Some(d) = self.ed.doc_mut() {
            let end = d.insert(at, text);
            self.ed.cursor = end;
            self.ed.anchor = None;
            self.ed.invalidate_states(at.line);
        }
    }

    fn backspace(&mut self) {
        if self.ed.is_huge() {
            return;
        }
        if self.delete_selection() {
            return;
        }
        let cur = self.ed.cursor;
        let prev = self.prev_pos(cur);
        if prev == cur {
            return;
        }
        if let Some(d) = self.ed.doc_mut() {
            d.delete(prev, cur);
        }
        self.ed.cursor = prev;
        self.ed.invalidate_states(prev.line);
    }

    fn delete_forward(&mut self) {
        if self.ed.is_huge() {
            return;
        }
        if self.delete_selection() {
            return;
        }
        let cur = self.ed.cursor;
        let next = self.next_pos(cur);
        if next == cur {
            return;
        }
        if let Some(d) = self.ed.doc_mut() {
            d.delete(cur, next);
        }
        self.ed.invalidate_states(cur.line);
    }

    /// 向前一个**字符**（不是一个字节）。按字节走的话一敲左箭头就能把中文切开。
    fn prev_pos(&mut self, p: Pos) -> Pos {
        if p.col > 0 {
            let line = self.ed.line(p.line);
            let mut c = p.col - 1;
            while c > 0 && !line.is_char_boundary(c) {
                c -= 1;
            }
            return Pos::new(p.line, c);
        }
        if p.line == 0 {
            return p;
        }
        let prev_len = self.ed.line(p.line - 1).len();
        Pos::new(p.line - 1, prev_len)
    }

    fn next_pos(&mut self, p: Pos) -> Pos {
        let line = self.ed.line(p.line);
        if p.col < line.len() {
            let mut c = p.col + 1;
            while c < line.len() && !line.is_char_boundary(c) {
                c += 1;
            }
            return Pos::new(p.line, c);
        }
        if p.line + 1 >= self.ed.line_count() {
            return p;
        }
        Pos::new(p.line + 1, 0)
    }

    fn move_cursor(&mut self, key: egui::Key, shift: bool) {
        if shift && self.ed.anchor.is_none() {
            self.ed.anchor = Some(self.ed.cursor);
        }
        if !shift {
            self.ed.anchor = None;
        }
        let cur = self.ed.cursor;
        let next = match key {
            egui::Key::ArrowLeft => self.prev_pos(cur),
            egui::Key::ArrowRight => self.next_pos(cur),
            egui::Key::ArrowUp => Pos::new(cur.line.saturating_sub(1), cur.col),
            egui::Key::ArrowDown => Pos::new(
                (cur.line + 1).min(self.ed.line_count().saturating_sub(1)),
                cur.col,
            ),
            egui::Key::Home => Pos::new(cur.line, 0),
            egui::Key::End => {
                let len = self.ed.line(cur.line).len();
                Pos::new(cur.line, len)
            }
            egui::Key::PageUp => Pos::new(cur.line.saturating_sub(40), cur.col),
            egui::Key::PageDown => Pos::new(
                (cur.line + 40).min(self.ed.line_count().saturating_sub(1)),
                cur.col,
            ),
            _ => cur,
        };
        self.ed.cursor = self.ed.clamp(next);
        self.scroll_to = Some(self.ed.cursor.line);
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Text(t) if self.editor_has_keys => self.insert_text(&t),
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if modifiers.command {
                        match key {
                            egui::Key::S => {
                                if let Err(e) = self.ed.save() {
                                    self.ed.status = format!("保存失败：{e}");
                                }
                            }
                            egui::Key::O => self.open_path(),
                            egui::Key::F => self.focus_search = true,
                            egui::Key::Z => {
                                if modifiers.shift {
                                    self.redo();
                                } else {
                                    self.undo();
                                }
                            }
                            egui::Key::Y => self.redo(),
                            _ => {}
                        }
                        continue;
                    }
                    if !self.editor_has_keys {
                        continue;
                    }
                    match key {
                        egui::Key::Enter => self.insert_text("\n"),
                        egui::Key::Tab => self.insert_text("    "),
                        egui::Key::Backspace => self.backspace(),
                        egui::Key::Delete => self.delete_forward(),
                        egui::Key::ArrowLeft
                        | egui::Key::ArrowRight
                        | egui::Key::ArrowUp
                        | egui::Key::ArrowDown
                        | egui::Key::Home
                        | egui::Key::End
                        | egui::Key::PageUp
                        | egui::Key::PageDown => self.move_cursor(key, modifiers.shift),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    fn undo(&mut self) {
        if let Some(d) = self.ed.doc_mut() {
            if let Some(p) = d.undo() {
                self.ed.cursor = p;
                self.ed.anchor = None;
                self.ed.invalidate_states(0);
                self.scroll_to = Some(p.line);
                return;
            }
        }
        self.ed.status = String::from("没有可撤销的操作");
    }

    fn redo(&mut self) {
        if let Some(d) = self.ed.doc_mut() {
            if let Some(p) = d.redo() {
                self.ed.cursor = p;
                self.ed.anchor = None;
                self.ed.invalidate_states(0);
                self.scroll_to = Some(p.line);
                return;
            }
        }
        self.ed.status = String::from("没有可重做的操作");
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let mut any_focus = false;
        ui.horizontal(|ui| {
            ui.label("Yi Edit");
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.path_input)
                    .desired_width(360.0)
                    .hint_text("文件路径"),
            );
            any_focus |= r.has_focus();
            if ui.button("打开").clicked() {
                self.open_path();
            }
            if ui.button("保存").clicked() {
                if let Err(e) = self.ed.save() {
                    self.ed.status = format!("保存失败：{e}");
                }
            }
            let dirty = self.ed.doc().map(|d| d.is_dirty()).unwrap_or(false);
            if dirty {
                ui.colored_label(egui::Color32::from_rgb(230, 180, 80), "● 未保存");
            }
            if self.ed.is_huge() {
                ui.colored_label(egui::Color32::from_rgb(120, 180, 240), "大文件只读");
            }
        });
        ui.horizontal(|ui| {
            let sr = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .desired_width(220.0)
                    .hint_text("查找"),
            );
            any_focus |= sr.has_focus();
            if self.focus_search {
                sr.request_focus();
                self.focus_search = false;
            }
            if sr.changed() {
                self.do_search();
            }
            if sr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let forward = !ui.input(|i| i.modifiers.shift);
                self.goto_hit(forward);
            }
            if ui.button("上一个").clicked() {
                self.goto_hit(false);
            }
            if ui.button("下一个").clicked() {
                self.goto_hit(true);
            }
            let rr = ui.add(
                egui::TextEdit::singleline(&mut self.replace)
                    .desired_width(220.0)
                    .hint_text("替换为"),
            );
            any_focus |= rr.has_focus();
            if ui.button("全部替换").clicked() {
                self.do_replace_all();
            }
            let a = ui.checkbox(&mut self.case_sensitive, "区分大小写").changed();
            let b = ui.checkbox(&mut self.whole_word, "全词匹配").changed();
            if a || b {
                self.do_search();
            }
            let label = if self.hits.is_empty() {
                String::from("无匹配")
            } else if self.hits_truncated {
                // 「只找到这么多」与「只有这么多」必须在界面上就分得清。
                format!("{}/{}+（已到上限）", self.hit_idx + 1, self.hits.len())
            } else {
                format!("{}/{}", self.hit_idx + 1, self.hits.len())
            };
            ui.label(label);
        });
        ui.label(
            egui::RichText::new(&self.ed.status)
                .color(egui::Color32::from_gray(150))
                .size(12.0),
        );
        self.editor_has_keys = !any_focus;
    }

    /// 排版宽度走 `Painter`：`Context::fonts` 给的是不可变引用，而排版需要可变。
    fn text_width(ui: &egui::Ui, s: &str) -> f32 {
        if s.is_empty() {
            return 0.0;
        }
        ui.painter()
            .layout_no_wrap(s.to_owned(), mono(), egui::Color32::WHITE)
            .rect
            .width()
    }

    fn col_from_x(ui: &egui::Ui, text: &str, x_rel: f32) -> usize {
        let mut acc = 0.0f32;
        for (i, ch) in text.char_indices() {
            let w = Self::text_width(ui, &text[i..i + ch.len_utf8()]);
            if x_rel < acc + w / 2.0 {
                return i;
            }
            acc += w;
        }
        text.len()
    }

    fn draw_row(&mut self, ui: &mut egui::Ui, row: usize, row_h: f32) {
        let text = self.ed.line(row);
        let state = self.ed.state_at(row);
        let lang = self.ed.lang;
        let (spans, _) = highlight_line(&text, lang, state);

        let width = ui.available_width().max(400.0);
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(width, row_h), egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let text_x = rect.min.x + GUTTER;

        // 行号
        painter.text(
            egui::pos2(rect.min.x + 4.0, rect.min.y),
            egui::Align2::LEFT_TOP,
            format!("{:>w$}", row + 1, w = LINE_NO_WIDTH),
            mono(),
            egui::Color32::from_gray(95),
        );

        // 选区
        if let Some((a, b)) = self.ed.selection() {
            if row >= a.line && row <= b.line {
                let from = if row == a.line { a.col } else { 0 };
                let to = if row == b.line { b.col } else { text.len() };
                let x0 = text_x + Self::text_width(ui, &text[..from.min(text.len())]);
                let x1 = text_x + Self::text_width(ui, &text[..to.min(text.len())]);
                let x1 = if row < b.line { x1 + 6.0 } else { x1 };
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x0, rect.min.y),
                        egui::pos2(x1.max(x0 + 2.0), rect.max.y),
                    ),
                    0.0,
                    egui::Color32::from_rgb(38, 79, 120),
                );
            }
        }

        // 匹配高亮
        if !self.search.is_empty() {
            for hit in self.hits.iter().filter(|p| p.line == row) {
                if hit.col > text.len() {
                    continue;
                }
                let end = (hit.col + self.search.len()).min(text.len());
                let x0 = text_x + Self::text_width(ui, &text[..hit.col]);
                let x1 = text_x + Self::text_width(ui, &text[..end]);
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x0, rect.min.y),
                        egui::pos2(x1.max(x0 + 2.0), rect.max.y),
                    ),
                    0.0,
                    egui::Color32::from_rgb(90, 80, 40),
                );
            }
        }

        // 正文
        let mut job = egui::text::LayoutJob::default();
        for s in &spans {
            job.append(
                &text[s.start..s.end],
                0.0,
                egui::TextFormat {
                    font_id: mono(),
                    color: color_for(s.kind),
                    ..Default::default()
                },
            );
        }
        let galley = ui.painter().layout_job(job);
        painter.galley(
            egui::pos2(text_x, rect.min.y),
            galley,
            egui::Color32::from_gray(220),
        );

        // 光标
        if self.ed.cursor.line == row && !self.ed.is_huge() {
            let col = self.ed.cursor.col.min(text.len());
            let x = text_x + Self::text_width(ui, &text[..col]);
            painter.line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                egui::Stroke::new(2.0, egui::Color32::from_rgb(240, 200, 100)),
            );
        }

        // 鼠标
        if let Some(p) = resp.interact_pointer_pos() {
            let col = Self::col_from_x(ui, &text, p.x - text_x);
            let target = Pos::new(row, col);
            let shift = ui.input(|i| i.modifiers.shift);
            if resp.drag_started() || (resp.clicked() && !shift) {
                self.ed.anchor = Some(target);
            } else if resp.dragged() && self.ed.anchor.is_none() {
                self.ed.anchor = Some(self.ed.cursor);
            }
            self.ed.cursor = self.ed.clamp(target);
        }
    }

    fn body(&mut self, ui: &mut egui::Ui) {
        let row_h = ui
            .text_style_height(&egui::TextStyle::Monospace)
            .max(FONT_SIZE);
        let total = self.ed.line_count().max(1);
        let mut area = egui::ScrollArea::both().auto_shrink([false, false]);
        if let Some(line) = self.scroll_to.take() {
            let offset = (line as f32 * row_h - 120.0).max(0.0);
            area = area.vertical_scroll_offset(offset);
        }
        area.show_rows(ui, row_h, total, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in range {
                self.draw_row(ui, row, row_h);
            }
        });
    }
}

impl eframe::App for YiEdit {
    /// eframe 0.36 的入口：拿到的是一个覆盖整窗口的 `Ui`，不再是 `&Context`。
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shot.tick(&ctx);
        self.handle_keys(&ctx);
        self.top_bar(ui);
        ui.separator();
        self.body(ui);
    }
}

/// 欢迎文本只有一份，在 editor 里。这里引一下只为了证明没有第二份拄本。
#[allow(dead_code)]
const _WELCOME_IS_SHARED: &str = WELCOME;
