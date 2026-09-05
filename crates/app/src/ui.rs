//! 界面层。只负责画和收键，不放任何搜索/替换/高亮/跳转映射/剪贴板的真逻辑。
//!
//! 布局：顶部工具栏 + 查找栏，底部状态栏，中间三列（文件面板 / 编辑区 / 跳转面板）。
//! 不用 Panel 容器：eframe 0.36 的 `App::ui` 直接给一个覆盖整窗口的 `Ui`，
//! 自己分矩形反而少碰两个会随版本漂的 API。

use std::path::PathBuf;
use std::sync::Arc;

use yi_edit_core::{highlight_line, Pos, SearchOptions, TokenKind};
use yi_edit_session::browser::{self, Listing};
use yi_edit_session::jump::JumpMap;
use yi_edit_session::{Editor, WELCOME};

use crate::shot::Shot;
use crate::theme as th;

fn mono() -> egui::FontId {
    egui::FontId::monospace(th::FONT_SIZE)
}

fn sans(size: f32) -> egui::FontId {
    egui::FontId::proportional(size)
}

fn color_for(kind: TokenKind) -> egui::Color32 {
    match kind {
        TokenKind::Text => th::TEXT,
        TokenKind::Keyword => egui::Color32::from_rgb(0xff, 0x7a, 0xb2),
        TokenKind::Type => egui::Color32::from_rgb(0x5a, 0xc8, 0xb8),
        TokenKind::Str => egui::Color32::from_rgb(0xff, 0x9f, 0x6a),
        TokenKind::Number => egui::Color32::from_rgb(0xa8, 0xd8, 0x7a),
        TokenKind::Comment => egui::Color32::from_rgb(0x6c, 0x8c, 0x62),
        TokenKind::Punct => th::TEXT_DIM,
    }
}

/// 装中文字体。只接受 .ttf/.otf：字体集合（.ttc）在部分环境下会直接 panic，
/// 而一个启动就挂的编辑器比一个显示豆子块的编辑器糟得多。
/// 找不到也不假装成功：在 stderr 里大声说，CI 报告里会带上这一行。
pub fn install_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/SFNSMono.ttf",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
        "/usr/share/fonts/truetype/arphic/uming.ttf",
        "C:\\Windows\\Fonts\\msyh.ttf",
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

/// macOS 观感的控件样式。
pub fn install_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = th::BG;
    visuals.window_fill = th::CHROME;
    visuals.extreme_bg_color = egui::Color32::from_rgb(0x1a, 0x1a, 0x1c);
    visuals.widgets.inactive.weak_bg_fill = th::CONTROL;
    visuals.widgets.inactive.bg_fill = th::CONTROL;
    visuals.widgets.hovered.weak_bg_fill = th::CONTROL_HOVER;
    visuals.widgets.active.weak_bg_fill = th::ACCENT;
    visuals.selection.bg_fill = th::ACCENT.gamma_multiply(0.5);
    visuals.selection.stroke = egui::Stroke::new(1.0, th::TEXT);
    ctx.set_visuals(visuals);

    // egui 0.36 把 style 改成了按主题作用域：`style()` / `set_style()` 没了，
    // 换成 `style_of(theme)` / `set_style_of(theme, ..)`。两个主题都设，
    // 否则跟随系统主题时间距会突然变回默认值。
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let mut style = (*ctx.style_of(theme)).clone();
        style.spacing.button_padding = egui::vec2(10.0, 4.0);
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        ctx.set_style_of(theme, style);
    }
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
    /// 键盘归编辑区还是归输入框。**只能有一份真身**：帧末从 egui 的焦点状态
    /// 读一次，下一帧用。之前在 handle_keys 前重置成 true、再让四个控件各自
    /// 报一遍，结果是在路径框里敲字会同时往正文里插一份。
    editor_has_keys: bool,
    show_sidebar: bool,
    show_jump: bool,
    show_find: bool,
    show_hidden: bool,
    /// 关窗确认框。不拦的话直接叉掉就丢改动，那是数据丢失级的。
    show_close_dialog: bool,
    force_close: bool,
    listing: Option<Listing>,
    listing_err: Option<String>,
    /// 最近一帧的可见行区间，跳转面板用它画视口指示器。
    visible: (usize, usize),
    shot: Shot,
}

impl YiEdit {
    pub fn new(arg: Option<PathBuf>) -> Self {
        let (ed, path_input) = match arg {
            Some(p) => match Editor::open(&p) {
                Ok(e) => (e, p.to_string_lossy().to_string()),
                Err(e) => {
                    let mut ed = Editor::empty();
                    ed.status = format!("打不开 {}：{e}", p.display());
                    (ed, p.to_string_lossy().to_string())
                }
            },
            None => (Editor::empty(), String::new()),
        };
        let mut me = Self {
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
            show_sidebar: true,
            show_jump: true,
            show_find: false,
            show_hidden: false,
            show_close_dialog: false,
            force_close: false,
            listing: None,
            listing_err: None,
            visible: (0, 0),
            shot: Shot::from_env(),
        };
        me.refresh_listing_for_current();
        me
    }

    fn opts(&self) -> SearchOptions {
        SearchOptions {
            case_sensitive: self.case_sensitive,
            whole_word: self.whole_word,
        }
    }

    // ---------- 文件面板 ----------

    fn refresh_listing_for_current(&mut self) {
        let dir = self
            .ed
            .path
            .as_ref()
            .and_then(|p| browser::dir_for(p))
            .or_else(|| std::env::current_dir().ok());
        if let Some(d) = dir {
            self.load_dir(&d);
        }
    }

    fn load_dir(&mut self, dir: &std::path::Path) {
        match browser::list_dir(dir, self.show_hidden) {
            Ok(l) => {
                self.listing = Some(l);
                self.listing_err = None;
            }
            // 打不开的目录要明确说，不能静默给一个空面板（那与「目录是空的」一模一样）。
            Err(e) => self.listing_err = Some(format!("读不了 {}：{e}", dir.display())),
        }
    }

    fn open_path(&mut self) {
        let p = PathBuf::from(self.path_input.trim());
        if p.as_os_str().is_empty() {
            self.ed.status = String::from("路径是空的");
            return;
        }
        self.open_file(&p);
    }

    fn open_file(&mut self, p: &std::path::Path) {
        match Editor::open(p) {
            Ok(e) => {
                self.ed = e;
                self.path_input = p.to_string_lossy().to_string();
                self.hits.clear();
                self.hit_idx = 0;
                self.scroll_to = Some(0);
                self.refresh_listing_for_current();
            }
            Err(e) => self.ed.status = format!("打不开 {}：{e}", p.display()),
        }
    }

    // ---------- 搜索 ----------

    fn do_search(&mut self) {
        let needle = self.search.clone();
        let opts = self.opts();
        let (hits, truncated) = self.ed.search(&needle, opts);
        self.hits = hits;
        self.hits_truncated = truncated;
        self.hit_idx = 0;
        if let Some(target) = self.hits.first().copied() {
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

    // ---------- 编辑与剪贴板 ----------

    /// 插入文本。真逻辑在 session 里（走 EditOp，所以可撤销），
    /// 这里只负责把拒绝的原因告诉用户 —— 静默失败与「改成功了」在屏幕上一模一样。
    fn insert_text(&mut self, text: &str) {
        if self.ed.is_huge() {
            self.ed.status =
                String::from("大文件为只读模式：V1 不在超过 64 MB 的文件上做内存编辑");
            return;
        }
        self.ed.insert_text(text);
    }

    fn copy(&mut self, ctx: &egui::Context) {
        // 只读模式下复制是合法的，所以这里不判 is_huge。
        match self.ed.selected_text() {
            Some(text) => {
                let n = text.chars().count();
                ctx.copy_text(text);
                self.ed.status = format!("已复制 {n} 个字符");
            }
            None => self.ed.status = String::from("没有选中任何内容"),
        }
    }

    fn cut(&mut self, ctx: &egui::Context) {
        if self.ed.is_huge() {
            // 先把选区复制进剪贴板再报只读：用户的意图至少完成一半。
            self.copy(ctx);
            self.ed.status = String::from("大文件为只读模式：已复制，但没有剪掉");
            return;
        }
        match self.ed.cut_selection() {
            Some(text) => {
                let n = text.chars().count();
                ctx.copy_text(text);
                self.ed.status = format!("已剪切 {n} 个字符");
            }
            None => self.ed.status = String::from("没有选中任何内容"),
        }
    }

    fn paste(&mut self, text: &str) {
        if self.ed.is_huge() {
            self.ed.status = String::from("大文件为只读模式：粘贴被拒绝了");
            return;
        }
        // 剪贴板里的 CRLF 归一化成 LF：不归的话从 Windows 记事本粘一段进来会
        // 多出一堆 \r，而那些 \r 在界面上看不见。
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        if self.ed.insert_text(&normalized) {
            self.ed.status = format!("已粘贴 {} 个字符", normalized.chars().count());
        }
    }

    fn backspace(&mut self) {
        if self.ed.is_huge() {
            return;
        }
        if self.ed.cut_selection().is_some() {
            return;
        }
        let cur = self.ed.cursor;
        let prev = self.ed.prev_pos(cur);
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
        if self.ed.cut_selection().is_some() {
            return;
        }
        let cur = self.ed.cursor;
        let next = self.ed.next_pos(cur);
        if next == cur {
            return;
        }
        if let Some(d) = self.ed.doc_mut() {
            d.delete(cur, next);
        }
        self.ed.invalidate_states(cur.line);
    }

    fn move_cursor(&mut self, key: egui::Key, shift: bool) {
        if shift && self.ed.anchor.is_none() {
            self.ed.anchor = Some(self.ed.cursor);
        }
        if !shift {
            self.ed.anchor = None;
        }
        let cur = self.ed.cursor;
        let last = self.ed.line_count().saturating_sub(1);
        let next = match key {
            egui::Key::ArrowLeft => self.ed.prev_pos(cur),
            egui::Key::ArrowRight => self.ed.next_pos(cur),
            egui::Key::ArrowUp => Pos::new(cur.line.saturating_sub(1), cur.col),
            egui::Key::ArrowDown => Pos::new((cur.line + 1).min(last), cur.col),
            egui::Key::Home => Pos::new(cur.line, 0),
            egui::Key::End => {
                let len = self.ed.line(cur.line).len();
                Pos::new(cur.line, len)
            }
            egui::Key::PageUp => Pos::new(cur.line.saturating_sub(40), cur.col),
            egui::Key::PageDown => Pos::new((cur.line + 40).min(last), cur.col),
            _ => cur,
        };
        self.ed.cursor = self.ed.clamp(next);
        self.scroll_to = Some(self.ed.cursor.line);
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

    fn save(&mut self) {
        if let Err(e) = self.ed.save() {
            self.ed.status = format!("保存失败：{e}");
        }
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            // 剪贴板三个事件与文本输入一样，只在键盘归编辑区时才接：
            // 否则焦点在路径框里时会粘两份（一份给输入框，一份给正文）。
            match ev {
                egui::Event::Copy if self.editor_has_keys => self.copy(ctx),
                egui::Event::Cut if self.editor_has_keys => self.cut(ctx),
                egui::Event::Paste(text) if self.editor_has_keys => self.paste(&text),
                egui::Event::Text(t) if self.editor_has_keys => self.insert_text(&t),
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if modifiers.command {
                        match key {
                            egui::Key::S => self.save(),
                            egui::Key::O => self.open_path(),
                            egui::Key::A if self.editor_has_keys => self.ed.select_all(),
                            egui::Key::F => {
                                self.show_find = true;
                                self.focus_search = true;
                            }
                            egui::Key::B => self.show_sidebar = !self.show_sidebar,
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

    // ---------- 关窗拦截 ----------

    /// 有未保存的修改时拦下关窗。**截图模式绕过**：否则 CI 里那一帧会被
    /// 对话框卡住，而那会表现成「截图超时」而不是「对话框挡住了」。
    fn guard_close(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) {
            return;
        }
        if self.force_close || self.shot.active() || !self.ed.is_dirty() {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        self.show_close_dialog = true;
    }

    fn close_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_close_dialog {
            return;
        }
        let name = self.ed.status_bar().name;
        let mut save_and_quit = false;
        let mut discard = false;
        let mut cancel = false;

        egui::Window::new(egui::RichText::new("有未保存的修改").font(sans(13.0)))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -40.0))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!("{name} 还有没保存的修改。"))
                        .font(sans(12.0))
                        .color(th::TEXT),
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    // macOS 的习惯：默认动作在最右，破坏性动作在最左。
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("不保存退出")
                                    .font(sans(12.0))
                                    .color(egui::Color32::from_rgb(0xff, 0x6b, 0x6b)),
                            )
                            .corner_radius(th::RADIUS)
                            .fill(th::CONTROL),
                        )
                        .clicked()
                    {
                        discard = true;
                    }
                    ui.add_space(40.0);
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("取消").font(sans(12.0)).color(th::TEXT),
                            )
                            .corner_radius(th::RADIUS)
                            .fill(th::CONTROL),
                        )
                        .clicked()
                    {
                        cancel = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("保存并退出")
                                    .font(sans(12.0))
                                    .color(th::TEXT),
                            )
                            .corner_radius(th::RADIUS)
                            .fill(th::ACCENT),
                        )
                        .clicked()
                    {
                        save_and_quit = true;
                    }
                });
                ui.add_space(2.0);
            });

        if cancel {
            self.show_close_dialog = false;
        }
        if discard {
            self.force_close = true;
            self.show_close_dialog = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if save_and_quit {
            self.save();
            // 保存失败就**不退**，对话框继续开着：否则保存失败 + 直接退出
            // 等于静默丢掉修改，而那正是这个对话框要防的事。
            if self.ed.is_dirty() {
                return;
            }
            self.force_close = true;
            self.show_close_dialog = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    // ---------- 绘制 ----------

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

    /// macOS 风格的小圆角按钮。
    fn tool_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
        let text = egui::RichText::new(label)
            .font(sans(13.0))
            .color(if enabled { th::TEXT } else { th::TEXT_DIM });
        let btn = egui::Button::new(text)
            .corner_radius(th::RADIUS)
            .fill(th::CONTROL);
        ui.add_enabled(enabled, btn).clicked()
    }

    fn toggle_button(ui: &mut egui::Ui, label: &str, on: &mut bool) {
        let fill = if *on { th::ACCENT } else { th::CONTROL };
        let btn = egui::Button::new(egui::RichText::new(label).font(sans(13.0)).color(th::TEXT))
            .corner_radius(th::RADIUS)
            .fill(fill);
        if ui.add(btn).clicked() {
            *on = !*on;
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let dirty = self.ed.is_dirty();
        let can_undo = self.ed.doc().map(|d| d.can_undo()).unwrap_or(false);
        let can_redo = self.ed.doc().map(|d| d.can_redo()).unwrap_or(false);
        let read_only = self.ed.is_huge();

        ui.horizontal_centered(|ui| {
            ui.add_space(10.0);
            let mut sidebar = self.show_sidebar;
            Self::toggle_button(ui, "侧栏", &mut sidebar);
            self.show_sidebar = sidebar;
            let mut jump = self.show_jump;
            Self::toggle_button(ui, "跳转", &mut jump);
            self.show_jump = jump;

            ui.add_space(4.0);
            if Self::tool_button(ui, "打开", true) {
                self.open_path();
            }
            if Self::tool_button(ui, "保存", !read_only) {
                self.save();
            }
            if Self::tool_button(ui, "撤销", can_undo) {
                self.undo();
            }
            if Self::tool_button(ui, "重做", can_redo) {
                self.redo();
            }
            let mut find = self.show_find;
            Self::toggle_button(ui, "查找", &mut find);
            if find != self.show_find {
                self.show_find = find;
                self.focus_search = find;
            }

            ui.add_space(8.0);
            let name = self
                .ed
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| String::from("未命名"));
            let title = if dirty { format!("{name}  ●") } else { name };
            ui.label(egui::RichText::new(title).font(sans(13.0)).color(th::TEXT));
            if read_only {
                ui.label(
                    egui::RichText::new("只读")
                        .font(sans(11.0))
                        .color(th::ACCENT),
                );
            }

            // 路径输入框靠右，占剩下的宽度。
            ui.add_space(8.0);
            let w = (ui.available_width() - 14.0).max(120.0);
            let r = ui.add_sized(
                [w, 24.0],
                egui::TextEdit::singleline(&mut self.path_input)
                    .font(sans(12.0))
                    .hint_text("文件路径"),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.open_path();
            }
        });
    }

    fn find_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(10.0);
            let sr = ui.add_sized(
                [200.0, 24.0],
                egui::TextEdit::singleline(&mut self.search)
                    .font(sans(12.0))
                    .hint_text("查找"),
            );
            if self.focus_search {
                sr.request_focus();
                self.focus_search = false;
            }
            if sr.changed() {
                self.do_search();
            }
            let enter_in_search =
                sr.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter_in_search {
                let forward = !ui.input(|i| i.modifiers.shift);
                self.goto_hit(forward);
            }
            if Self::tool_button(ui, "上", !self.hits.is_empty()) {
                self.goto_hit(false);
            }
            if Self::tool_button(ui, "下", !self.hits.is_empty()) {
                self.goto_hit(true);
            }

            ui.add_sized(
                [180.0, 24.0],
                egui::TextEdit::singleline(&mut self.replace)
                    .font(sans(12.0))
                    .hint_text("替换为"),
            );
            if Self::tool_button(ui, "全部替换", !self.search.is_empty()) {
                self.do_replace_all();
            }

            let a = ui
                .checkbox(
                    &mut self.case_sensitive,
                    egui::RichText::new("Aa").font(sans(12.0)),
                )
                .changed();
            let b = ui
                .checkbox(
                    &mut self.whole_word,
                    egui::RichText::new("全词").font(sans(12.0)),
                )
                .changed();
            if a || b {
                self.do_search();
            }

            // 「只找到这么多」与「只有这么多」必须在界面上就分得清。
            let label = if self.search.is_empty() {
                String::new()
            } else if self.hits.is_empty() {
                String::from("无匹配")
            } else if self.hits_truncated {
                format!("{}/{}+（已到上限）", self.hit_idx + 1, self.hits.len())
            } else {
                format!("{}/{}", self.hit_idx + 1, self.hits.len())
            };
            ui.label(
                egui::RichText::new(label)
                    .font(sans(12.0))
                    .color(th::TEXT_DIM),
            );
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let mut to_open: Option<PathBuf> = None;
        let mut to_cd: Option<PathBuf> = None;

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("文件")
                    .font(sans(11.0))
                    .color(th::TEXT_DIM),
            );
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("隐藏").font(sans(10.0)).color(
                        if self.show_hidden {
                            th::TEXT
                        } else {
                            th::TEXT_DIM
                        },
                    ))
                    .corner_radius(4.0)
                    .fill(egui::Color32::TRANSPARENT),
                )
                .clicked()
            {
                self.show_hidden = !self.show_hidden;
                if let Some(l) = &self.listing {
                    to_cd = Some(l.dir.clone());
                }
            }
        });

        if let Some(err) = &self.listing_err {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(err)
                    .font(sans(11.0))
                    .color(egui::Color32::from_rgb(0xff, 0x6b, 0x6b)),
            );
        }

        let current = self.ed.path.clone();
        if let Some(l) = &self.listing {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(l.dir.to_string_lossy().to_string())
                    .font(sans(10.0))
                    .color(th::TEXT_DIM),
            );
            ui.add_space(4.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if let Some(parent) = l.parent.clone() {
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("..").font(sans(12.0)).color(th::TEXT_DIM),
                                )
                                .corner_radius(4.0)
                                .fill(egui::Color32::TRANSPARENT),
                            )
                            .clicked()
                        {
                            to_cd = Some(parent);
                        }
                    }
                    for e in &l.entries {
                        let is_current = current.as_deref() == Some(e.path.as_path());
                        let label = if e.is_dir {
                            format!("{}/", e.name)
                        } else {
                            e.name.clone()
                        };
                        let color = if e.is_dir {
                            egui::Color32::from_rgb(0x7a, 0xb8, 0xff)
                        } else {
                            th::TEXT
                        };
                        let fill = if is_current {
                            th::ACCENT.gamma_multiply(0.35)
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        // 文件名必须左对齐：`add_sized` 给 Button 会把文字居中，
                        // 而一个居中的文件列表每行起点都不同，没法扫。
                        let btn = egui::Button::new(
                            egui::RichText::new(label).font(sans(12.0)).color(color),
                        )
                        .corner_radius(4.0)
                        .fill(fill)
                        .min_size(egui::vec2(ui.available_width(), 20.0));
                        if ui.add(btn.right_text("")).clicked() {
                            if e.is_dir {
                                to_cd = Some(e.path.clone());
                            } else {
                                to_open = Some(e.path.clone());
                            }
                        }
                    }
                    // 隐藏与截断都要报出来，不能静默丢掉。
                    if l.hidden_skipped > 0 {
                        ui.label(
                            egui::RichText::new(format!("已隐藏 {} 项", l.hidden_skipped))
                                .font(sans(10.0))
                                .color(th::TEXT_DIM),
                        );
                    }
                    if l.truncated > 0 {
                        ui.label(
                            egui::RichText::new(format!("还有 {} 项未列出", l.truncated))
                                .font(sans(10.0))
                                .color(egui::Color32::from_rgb(0xff, 0xb0, 0x6b)),
                        );
                    }
                });
        }

        if let Some(d) = to_cd {
            self.load_dir(&d);
        }
        if let Some(f) = to_open {
            self.open_file(&f);
        }
    }

    /// 右侧快速跳转面板。映射逻辑全在 `yi_edit_session::jump`，这里只画。
    ///
    /// **缩略图按行画而不是按像素画。** 按像素采样的话，13 行的文件里同一行会被
    /// 画几十遍叠成一个实心块 —— 这就是截图里右上角那个蓝块的成因。
    fn jump_panel(&mut self, ui: &mut egui::Ui, rect: egui::Rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, th::CHROME);

        let h = rect.height().floor().max(1.0) as u32;
        let lines = self.ed.line_count();
        let Some(map) = JumpMap::new(h, lines) else {
            return;
        };

        // 可见窗口指示器。
        let (vt, vb) = map.viewport_band(self.visible.0, self.visible.1.max(1));
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.min.x, rect.min.y + vt as f32),
                egui::pos2(rect.max.x, rect.min.y + vb as f32),
            ),
            0.0,
            egui::Color32::from_rgba_unmultiplied(0xff, 0xff, 0xff, 18),
        );

        // 内容缩略：**按行**遍历，每行一条短线，用长度表示缩进与密度。
        // 行很多时（行高不到 1px）才降级成按像素采样，否则一个百万行的文件
        // 要画一百万条线。大文件模式完全不画：那会每帧把整个文件扫一遗。
        if !self.ed.is_huge() {
            let usable = rect.width() - 8.0;
            let step = (lines / h.max(1) as usize).max(1);
            let mut line_no = 0usize;
            while line_no < lines {
                let Some((top, bottom)) = map.line_band(line_no) else {
                    break;
                };
                let text = self.ed.line(line_no);
                let trimmed = text.trim_end();
                if !trimmed.is_empty() {
                    let indent = text.len() - text.trim_start().len();
                    let x0 = rect.min.x + 4.0 + (indent as f32 * 1.1).min(usable * 0.5);
                    let len_frac = (trimmed.len() as f32 / 90.0).min(1.0);
                    let x1 = (x0 + usable * len_frac).min(rect.max.x - 4.0);
                    // 线宽取带高的一半且不超过 2px：带很高时（短文件）也只画一条细线，
                    // 而不是把整带填成实心块。
                    let band_h = (bottom.saturating_sub(top)) as f32;
                    let w = band_h.min(2.0).max(1.0);
                    let y = rect.min.y + top as f32 + band_h / 2.0;
                    if x1 > x0 {
                        painter.line_segment(
                            [egui::pos2(x0, y), egui::pos2(x1, y)],
                            egui::Stroke::new(w, egui::Color32::from_gray(0x5c)),
                        );
                    }
                }
                line_no += step;
            }
        }

        // 搜索命中。画在缩略图之上，否则会被盖住。
        for hit in &self.hits {
            if let Some((top, bottom)) = map.line_band(hit.line) {
                let y0 = rect.min.y + top as f32;
                let y1 = rect.min.y + (bottom.max(top + 1)) as f32;
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(rect.min.x + 3.0, y0),
                        egui::pos2(rect.max.x - 3.0, y1.min(y0 + 3.0)),
                    ),
                    0.0,
                    egui::Color32::from_rgb(0xd8, 0xb0, 0x40),
                );
            }
        }

        // 当前行。**高度夹到 2px**：不夹的话短文件里一行占几十像素，
        // 用强调色铺满就成了截图里右上角那个大蓝块。
        if let Some((top, _)) = map.line_band(self.ed.cursor.line) {
            let y0 = rect.min.y + top as f32;
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.min.x, y0),
                    egui::pos2(rect.max.x, y0 + 2.0),
                ),
                0.0,
                th::ACCENT,
            );
        }

        // 点击 / 拖拽跳转。
        let resp = ui.interact(
            rect,
            ui.id().with("jump-panel"),
            egui::Sense::click_and_drag(),
        );
        if let Some(p) = resp.interact_pointer_pos() {
            let y = (p.y - rect.min.y).max(0.0) as u32;
            let line = map.line_at(y);
            self.ed.cursor = self.ed.clamp(Pos::new(line, 0));
            self.ed.anchor = None;
            self.scroll_to = Some(line);
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let bar = self.ed.status_bar();
        ui.horizontal_centered(|ui| {
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(bar.name.clone())
                    .font(sans(11.0))
                    .color(th::TEXT),
            );
            for text in [bar.position_text(), bar.size_text()] {
                ui.label(
                    egui::RichText::new("·")
                        .font(sans(11.0))
                        .color(th::HAIRLINE),
                );
                ui.label(
                    egui::RichText::new(text)
                        .font(sans(11.0))
                        .color(th::TEXT_DIM),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                for b in bar.badges().into_iter().rev() {
                    ui.label(egui::RichText::new(b).font(sans(11.0)).color(th::TEXT_DIM));
                }
            });
        });
    }

    fn draw_row(&mut self, ui: &mut egui::Ui, row: usize, row_h: f32, row_w: f32) {
        let text = self.ed.line(row);
        let state = self.ed.state_at(row);
        let lang = self.ed.lang;
        let (spans, _) = highlight_line(&text, lang, state);

        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(row_w, row_h), egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let text_x = rect.min.x + th::GUTTER_W;
        let is_current = self.ed.cursor.line == row;

        if is_current {
            painter.rect_filled(rect, 0.0, th::CURRENT_LINE);
        }

        painter.text(
            egui::pos2(rect.min.x + th::GUTTER_W - 12.0, rect.min.y),
            egui::Align2::RIGHT_TOP,
            format!("{:>w$}", row + 1, w = th::LINE_NO_DIGITS),
            mono(),
            if is_current {
                th::GUTTER_TEXT_ACTIVE
            } else {
                th::GUTTER_TEXT
            },
        );

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
                    2.0,
                    th::SELECTION,
                );
            }
        }

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
                    2.0,
                    th::MATCH,
                );
            }
        }

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
        painter.galley(egui::pos2(text_x, rect.min.y), galley, th::TEXT);

        if is_current && !self.ed.is_huge() {
            let col = self.ed.cursor.col.min(text.len());
            let x = text_x + Self::text_width(ui, &text[..col]);
            painter.line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                egui::Stroke::new(2.0, th::CARET),
            );
        }

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

    fn editor_area(&mut self, ui: &mut egui::Ui) {
        // **行距必须在 show_rows 之前清零。** 写在闭包内部的话，egui 已经按
        // 「行高 + 默认行距」算完了几何，于是预留高度比真正画出来的多
        // 可见行数 × 行距，底部就出现一大块黑色留白。
        ui.spacing_mut().item_spacing.y = 0.0;
        let row_h = ui
            .text_style_height(&egui::TextStyle::Monospace)
            .max(th::FONT_SIZE);
        let total = self.ed.line_count().max(1);
        let viewport_w = ui.available_width();

        // 横向滚动要真的能滚：行宽必须反映**内容**而不是视口。
        // 取 available_width() 的话内容永远不会比视口宽，超长行被裁掉且滚不过去。
        let first_visible = self.visible.0.min(total.saturating_sub(1));
        let sample_to = (first_visible + self.visible.1.max(1) + 2).min(total);
        let mut widest = 0.0f32;
        for i in first_visible..sample_to {
            let t = self.ed.line(i);
            widest = widest.max(Self::text_width(ui, &t));
        }
        let row_w = (th::GUTTER_W + widest + 40.0).max(viewport_w);

        let mut area = egui::ScrollArea::both().auto_shrink([false, false]);
        if let Some(line) = self.scroll_to.take() {
            let offset = (line as f32 * row_h - 120.0).max(0.0);
            area = area.vertical_scroll_offset(offset);
        }
        let out = area.show_rows(ui, row_h, total, |ui, range| {
            let shown = (range.start, range.end.saturating_sub(range.start));
            for row in range {
                self.draw_row(ui, row, row_h, row_w);
            }
            shown
        });
        self.visible = out.inner;
    }

    fn hairline(painter: &egui::Painter, from: egui::Pos2, to: egui::Pos2) {
        painter.line_segment([from, to], egui::Stroke::new(1.0, th::HAIRLINE));
    }

    fn region(ui: &mut egui::Ui, rect: egui::Rect, add: impl FnOnce(&mut egui::Ui)) {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect);
        add(&mut child);
    }
}

impl eframe::App for YiEdit {
    /// eframe 0.36 的入口：拿到的是一个覆盖整窗口的 `Ui`，不再是 `&Context`。
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shot.tick(&ctx);
        self.guard_close(&ctx);
        self.handle_keys(&ctx);

        let full = ui.available_rect_before_wrap();
        let painter = ui.painter().clone();
        painter.rect_filled(full, 0.0, th::BG);

        // ---- 顶部：工具栏（+ 可选查找栏）----
        let toolbar = egui::Rect::from_min_size(full.min, egui::vec2(full.width(), th::TOOLBAR_H));
        painter.rect_filled(toolbar, 0.0, th::CHROME);
        Self::hairline(
            &painter,
            egui::pos2(full.min.x, toolbar.max.y),
            egui::pos2(full.max.x, toolbar.max.y),
        );
        Self::region(ui, toolbar, |ui| self.toolbar(ui));

        let mut top = toolbar.max.y;
        if self.show_find {
            let find = egui::Rect::from_min_size(
                egui::pos2(full.min.x, top),
                egui::vec2(full.width(), th::FINDBAR_H),
            );
            painter.rect_filled(find, 0.0, th::CHROME);
            Self::hairline(
                &painter,
                egui::pos2(full.min.x, find.max.y),
                egui::pos2(full.max.x, find.max.y),
            );
            Self::region(ui, find, |ui| self.find_bar(ui));
            top = find.max.y;
        }

        // ---- 底部：状态栏 ----
        let status = egui::Rect::from_min_size(
            egui::pos2(full.min.x, full.max.y - th::STATUS_H),
            egui::vec2(full.width(), th::STATUS_H),
        );
        painter.rect_filled(status, 0.0, th::CHROME);
        Self::hairline(
            &painter,
            egui::pos2(full.min.x, status.min.y),
            egui::pos2(full.max.x, status.min.y),
        );
        Self::region(ui, status, |ui| self.status_bar(ui));

        // ---- 中间三列 ----
        let body = egui::Rect::from_min_max(
            egui::pos2(full.min.x, top),
            egui::pos2(full.max.x, status.min.y),
        );
        let mut left = body.min.x;
        let mut right = body.max.x;

        if self.show_sidebar {
            let side = egui::Rect::from_min_max(
                egui::pos2(left, body.min.y),
                egui::pos2(left + th::SIDEBAR_W, body.max.y),
            );
            painter.rect_filled(side, 0.0, th::CHROME);
            Self::hairline(
                &painter,
                egui::pos2(side.max.x, body.min.y),
                egui::pos2(side.max.x, body.max.y),
            );
            Self::region(ui, side, |ui| self.sidebar(ui));
            left = side.max.x + 1.0;
        }

        if self.show_jump {
            let jump = egui::Rect::from_min_max(
                egui::pos2(right - th::JUMP_W, body.min.y),
                egui::pos2(right, body.max.y),
            );
            Self::hairline(
                &painter,
                egui::pos2(jump.min.x, body.min.y),
                egui::pos2(jump.min.x, body.max.y),
            );
            self.jump_panel(ui, jump);
            right = jump.min.x - 1.0;
        }

        let center = egui::Rect::from_min_max(
            egui::pos2(left, body.min.y),
            egui::pos2(right, body.max.y),
        );
        Self::region(ui, center, |ui| self.editor_area(ui));

        self.close_dialog(&ctx);

        // 焦点只读一次，帧末读，下一帧用。这是 `editor_has_keys` 的**唯一**写入点（除了初始化）。
        self.editor_has_keys = ctx.memory(|m| m.focused()).is_none() && !self.show_close_dialog;
    }
}

/// 欢迎文本只有一份，在 session 里。这里引一下只为了证明没有第二份拄本。
#[allow(dead_code)]
const _WELCOME_IS_SHARED: &str = WELCOME;
