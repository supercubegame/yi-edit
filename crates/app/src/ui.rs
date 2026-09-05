//! Yi Edit UI。会话逻辑在 yi-edit-session，IME 事件在这里消费。
use std::path::PathBuf;
use std::sync::Arc;

use yi_edit_core::{highlight_line, Pos, SearchOptions, TokenKind};
use yi_edit_session::browser::{self, Listing};
use yi_edit_session::jump::JumpMap;
use yi_edit_session::{Editor, WELCOME};

use crate::ime_adapter::{AdapterEffect, ImeAdapter};
use crate::shot::Shot;
use crate::theme as th;

fn mono() -> egui::FontId { egui::FontId::monospace(th::FONT_SIZE) }
fn sans(size: f32) -> egui::FontId { egui::FontId::proportional(size) }

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

pub fn install_fonts(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/SFNSMono.ttf",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
        "C:\\Windows\\Fonts\\msyh.ttf",
        "C:\\Windows\\Fonts\\simhei.ttf",
    ];
    let found = CANDIDATES.iter().find_map(|p| std::fs::read(p).ok().filter(|b| b.len() > 4096).map(|b| (*p, b)));
    let Some((path, bytes)) = found else { eprintln!("FONT: 没找到可用的中文字体"); return; };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("cjk".into(), Arc::new(egui::FontData::from_owned(bytes)));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("cjk".into());
    }
    ctx.set_fonts(fonts);
    eprintln!("FONT: 使用 {path}");
}

pub fn install_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = th::BG;
    v.window_fill = th::CHROME;
    v.widgets.inactive.weak_bg_fill = th::CONTROL;
    v.widgets.inactive.bg_fill = th::CONTROL;
    v.widgets.hovered.weak_bg_fill = th::CONTROL_HOVER;
    v.widgets.active.weak_bg_fill = th::ACCENT;
    v.selection.bg_fill = th::ACCENT.gamma_multiply(0.5);
    ctx.set_visuals(v);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        let mut s = (*ctx.style_of(theme)).clone();
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        ctx.set_style_of(theme, s);
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
    editor_has_keys: bool,
    show_sidebar: bool,
    show_jump: bool,
    show_find: bool,
    show_hidden: bool,
    show_close_dialog: bool,
    force_close: bool,
    listing: Option<Listing>,
    listing_err: Option<String>,
    visible: (usize, usize),
    ime: ImeAdapter,
    ime_preedit: String,
    ime_active_range: Option<std::ops::Range<usize>>,
    shot: Shot,
}

impl YiEdit {
    pub fn new(arg: Option<PathBuf>) -> Self {
        let (ed, path_input) = match arg {
            Some(p) => match Editor::open(&p) {
                Ok(e) => (e, p.to_string_lossy().to_string()),
                Err(e) => { let mut e2 = Editor::empty(); e2.status = format!("打不开 {}：{e}", p.display()); (e2, p.to_string_lossy().to_string()) }
            },
            None => (Editor::empty(), String::new()),
        };
        let mut x = Self { ed, path_input, search: String::new(), replace: String::new(), case_sensitive: false, whole_word: false, hits: Vec::new(), hits_truncated: false, hit_idx: 0, scroll_to: None, focus_search: false, editor_has_keys: true, show_sidebar: true, show_jump: true, show_find: false, show_hidden: false, show_close_dialog: false, force_close: false, listing: None, listing_err: None, visible: (0, 0), ime: ImeAdapter::default(), ime_preedit: String::new(), ime_active_range: None, shot: Shot::from_env() };
        x.refresh_listing_for_current();
        x
    }

    fn opts(&self) -> SearchOptions { SearchOptions { case_sensitive: self.case_sensitive, whole_word: self.whole_word } }

    fn refresh_listing_for_current(&mut self) {
        let dir = self.ed.path.as_ref().and_then(|p| browser::dir_for(p)).or_else(|| std::env::current_dir().ok());
        if let Some(d) = dir { self.load_dir(&d); }
    }
    fn load_dir(&mut self, dir: &std::path::Path) {
        match browser::list_dir(dir, self.show_hidden) { Ok(l) => { self.listing = Some(l); self.listing_err = None; }, Err(e) => self.listing_err = Some(format!("读不了 {}：{e}", dir.display())) }
    }
    fn open_path(&mut self) { let p = PathBuf::from(self.path_input.trim()); if p.as_os_str().is_empty() { self.ed.status = "路径是空的".into(); } else { self.open_file(&p); } }
    fn open_file(&mut self, p: &std::path::Path) { match Editor::open(p) { Ok(e) => { self.ed.commit_undo_group(); self.ed = e; self.path_input = p.to_string_lossy().into(); self.hits.clear(); self.scroll_to = Some(0); self.refresh_listing_for_current(); }, Err(e) => self.ed.status = format!("打不开 {}：{e}", p.display()) } }

    fn do_search(&mut self) { let (h, t) = self.ed.search(&self.search.clone(), self.opts()); self.hits = h; self.hits_truncated = t; self.hit_idx = 0; if let Some(p) = self.hits.first().copied() { self.ed.cursor = self.ed.clamp(p); self.ed.anchor = None; self.scroll_to = Some(p.line); } }
    fn goto_hit(&mut self, forward: bool) { if self.hits.is_empty() { return; } let n = self.hits.len(); self.hit_idx = if forward { (self.hit_idx + 1) % n } else { (self.hit_idx + n - 1) % n }; let p = self.hits[self.hit_idx]; self.ed.cursor = self.ed.clamp(p); self.ed.anchor = None; self.scroll_to = Some(p.line); self.ed.commit_undo_group(); }
    fn do_replace_all(&mut self) { match self.ed.replace_all(&self.search.clone(), &self.replace.clone(), self.opts()) { Ok(n) => { self.ed.status = format!("已替换 {n} 处"); self.do_search(); }, Err(e) => self.ed.status = format!("替换失败：{e}") } }

    fn insert_text(&mut self, text: &str) { if self.ed.is_huge() { self.ed.status = "大文件为只读模式：粘贴被拒绝了".into(); return; } if self.ed.insert_text(text) { self.ed.status = format!("已输入 {} 个字符", text.chars().count()); } }
    fn copy(&mut self, ctx: &egui::Context) { match self.ed.selected_text() { Some(t) => { ctx.copy_text(t.clone()); self.ed.status = format!("已复制 {} 个字符", t.chars().count()); }, None => self.ed.status = "没有选中任何内容".into() } }
    fn cut(&mut self, ctx: &egui::Context) { if self.ed.is_huge() { self.copy(ctx); self.ed.status = "大文件为只读模式：已复制，但没有剪掉".into(); return; } match self.ed.cut_selection() { Some(t) => { ctx.copy_text(t.clone()); self.ed.status = format!("已剪切 {} 个字符", t.chars().count()); }, None => self.ed.status = "没有选中任何内容".into() } }
    fn paste(&mut self, text: &str) { let t = text.replace("\r\n", "\n").replace('\r', "\n"); self.insert_text(&t); }

    fn delete_surrounding(&mut self, before: usize, after: usize) {
        if self.ed.is_huge() { return; }
        let mut from = self.ed.cursor;
        for _ in 0..before { from = self.ed.prev_pos(from); }
        let mut to = self.ed.cursor;
        for _ in 0..after { to = self.ed.next_pos(to); }
        if from != to { if let Some(d) = self.ed.doc_mut() { d.delete(from, to); self.ed.cursor = from; self.ed.anchor = None; self.ed.invalidate_states(from.line); } }
    }

    fn handle_ime(&mut self, ev: &egui::ImeEvent) {
        match self.ime.feed(ev) {
            AdapterEffect::None => {},
            AdapterEffect::Commit(t) => { self.insert_text(&t); self.ed.commit_undo_group(); },
            AdapterEffect::DeleteSurrounding { before_chars, after_chars } => self.delete_surrounding(before_chars, after_chars),
            AdapterEffect::ClearPreedit => { self.ime_preedit.clear(); self.ime_active_range = None; },
        }
        let p = self.ime.preedit();
        self.ime_preedit = p.text;
        self.ime_active_range = p.active_range_chars;
    }

    fn backspace(&mut self) { if self.ed.is_huge() { return; } if self.ed.cut_selection().is_some() { return; } let cur=self.ed.cursor; let prev=self.ed.prev_pos(cur); if prev!=cur { if let Some(d)=self.ed.doc_mut(){d.delete(prev,cur); self.ed.cursor=prev; self.ed.invalidate_states(prev.line);} } }
    fn delete_forward(&mut self) { if self.ed.is_huge() { return; } if self.ed.cut_selection().is_some() { return; } let cur=self.ed.cursor; let next=self.ed.next_pos(cur); if next!=cur { if let Some(d)=self.ed.doc_mut(){d.delete(cur,next); self.ed.invalidate_states(cur.line);} } }
    fn move_cursor(&mut self, key: egui::Key, shift: bool) { self.ed.commit_undo_group(); if shift && self.ed.anchor.is_none(){self.ed.anchor=Some(self.ed.cursor);} if !shift{self.ed.anchor=None;} let c=self.ed.cursor; let last=self.ed.line_count().saturating_sub(1); let n=match key{egui::Key::ArrowLeft=>self.ed.prev_pos(c),egui::Key::ArrowRight=>self.ed.next_pos(c),egui::Key::ArrowUp=>Pos::new(c.line.saturating_sub(1),c.col),egui::Key::ArrowDown=>Pos::new((c.line+1).min(last),c.col),egui::Key::Home=>Pos::new(c.line,0),egui::Key::End=>Pos::new(c.line,self.ed.line(c.line).len()),egui::Key::PageUp=>Pos::new(c.line.saturating_sub(40),c.col),egui::Key::PageDown=>Pos::new((c.line+40).min(last),c.col),_=>c}; self.ed.cursor=self.ed.clamp(n); self.scroll_to=Some(self.ed.cursor.line); }
    fn undo(&mut self){if let Some(d)=self.ed.doc_mut(){if let Some(p)=d.undo(){self.ed.cursor=p;self.ed.anchor=None;self.ed.invalidate_states(0);self.scroll_to=Some(p.line);return;}} self.ed.status="没有可撤销的操作".into();}
    fn redo(&mut self){if let Some(d)=self.ed.doc_mut(){if let Some(p)=d.redo(){self.ed.cursor=p;self.ed.anchor=None;self.ed.invalidate_states(0);self.scroll_to=Some(p.line);return;}} self.ed.status="没有可重做的操作".into();}
    fn save(&mut self){if let Err(e)=self.ed.save(){self.ed.status=format!("保存失败：{e}");}}

    fn handle_keys(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for ev in events {
            match ev {
                egui::Event::Ime(ref ime) if self.editor_has_keys => self.handle_ime(ime),
                egui::Event::Copy if self.editor_has_keys => self.copy(ctx),
                egui::Event::Cut if self.editor_has_keys => self.cut(ctx),
                egui::Event::Paste(t) if self.editor_has_keys => self.paste(&t),
                egui::Event::Text(t) if self.editor_has_keys => self.insert_text(&t),
                egui::Event::Key { key, pressed:true, modifiers, .. } => {
                    if modifiers.command { match key { egui::Key::S=>self.save(), egui::Key::O=>self.open_path(), egui::Key::A if self.editor_has_keys=>self.ed.select_all(), egui::Key::F=>{self.show_find=true;self.focus_search=true;}, egui::Key::B=>self.show_sidebar=!self.show_sidebar, egui::Key::Z=>if modifiers.shift{self.redo()}else{self.undo()}, egui::Key::Y=>self.redo(), _=>{} } continue; }
                    if !self.editor_has_keys { continue; }
                    match key { egui::Key::Enter=>self.insert_text("\n"),egui::Key::Tab=>self.insert_text("    "),egui::Key::Backspace=>self.backspace(),egui::Key::Delete=>self.delete_forward(),egui::Key::ArrowLeft|egui::Key::ArrowRight|egui::Key::ArrowUp|egui::Key::ArrowDown|egui::Key::Home|egui::Key::End|egui::Key::PageUp|egui::Key::PageDown=>self.move_cursor(key,modifiers.shift), _=>{} }
                }
                _ => {}
            }
        }
    }

    fn guard_close(&mut self, ctx: &egui::Context) { if ctx.input(|i|i.viewport().close_requested()) && !self.force_close && !self.shot.active() && self.ed.is_dirty(){ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);self.show_close_dialog=true;} }
    fn close_dialog(&mut self, ctx: &egui::Context) { if !self.show_close_dialog{return;} egui::Window::new("有未保存的修改").collapsible(false).resizable(false).show(ctx,|ui|{ui.label("文件还有未保存的修改");ui.horizontal(|ui|{if ui.button("不保存退出").clicked(){self.force_close=true;self.show_close_dialog=false;ctx.send_viewport_cmd(egui::ViewportCommand::Close);}if ui.button("取消").clicked(){self.show_close_dialog=false;}if ui.button("保存并退出").clicked(){self.save();if !self.ed.is_dirty(){self.force_close=true;self.show_close_dialog=false;ctx.send_viewport_cmd(egui::ViewportCommand::Close);}}});});}

    fn text_width(ui:&egui::Ui,s:&str)->f32{if s.is_empty(){0.0}else{ui.painter().layout_no_wrap(s.to_owned(),mono(),egui::Color32::WHITE).rect.width()}}
    fn col_from_x(ui:&egui::Ui,text:&str,x:f32)->usize{let mut a=0.0;for(i,ch)in text.char_indices(){let w=Self::text_width(ui,&text[i..i+ch.len_utf8()]);if x<a+w/2.0{return i;}a+=w;}text.len()}
    fn tool_button(ui:&mut egui::Ui,label:&str,enabled:bool)->bool{ui.add_enabled(enabled,egui::Button::new(egui::RichText::new(label).font(sans(13.0)).color(if enabled{th::TEXT}else{th::TEXT_DIM})).corner_radius(th::RADIUS).fill(th::CONTROL)).clicked()}
    fn toggle_button(ui:&mut egui::Ui,label:&str,on:&mut bool){let b=egui::Button::new(egui::RichText::new(label).font(sans(13.0)).color(th::TEXT)).corner_radius(th::RADIUS).fill(if *on{th::ACCENT}else{th::CONTROL});if ui.add(b).clicked(){*on=!*on;}}

    fn toolbar(&mut self,ui:&mut egui::Ui){let dirty=self.ed.is_dirty();let ro=self.ed.is_huge();ui.horizontal_centered(|ui|{ui.add_space(10.0);let mut s=self.show_sidebar;Self::toggle_button(ui,"侧栏",&mut s);self.show_sidebar=s;let mut j=self.show_jump;Self::toggle_button(ui,"跳转",&mut j);self.show_jump=j;ui.add_space(4.0);if Self::tool_button(ui,"打开",true){self.open_path();}if Self::tool_button(ui,"保存",!ro){self.save();}if Self::tool_button(ui,"撤销",self.ed.doc().map(|d|d.can_undo()).unwrap_or(false)){self.undo();}if Self::tool_button(ui,"重做",self.ed.doc().map(|d|d.can_redo()).unwrap_or(false)){self.redo();}let mut f=self.show_find;Self::toggle_button(ui,"查找",&mut f);if f!=self.show_find{self.show_find=f;self.focus_search=f;}ui.add_space(8.0);let n=self.ed.path.as_ref().and_then(|p|p.file_name()).map(|n|n.to_string_lossy().to_string()).unwrap_or_else(||"未命名".into());ui.label(egui::RichText::new(if dirty{format!("{n} ●")}else{n}).font(sans(13.0)).color(th::TEXT));if ro{ui.label(egui::RichText::new("只读").font(sans(11.0)).color(th::ACCENT));}ui.add_space(8.0);ui.add_sized([(ui.available_width()-14.0).max(120.0),24.0],egui::TextEdit::singleline(&mut self.path_input).font(sans(12.0)).hint_text("文件路径"));});}
    fn find_bar(&mut self,ui:&mut egui::Ui){ui.horizontal_centered(|ui|{ui.add_space(10.0);let sr=ui.add_sized([200.0,24.0],egui::TextEdit::singleline(&mut self.search).font(sans(12.0)).hint_text("查找"));if self.focus_search{sr.request_focus();self.focus_search=false;}if sr.changed(){self.do_search();}if Self::tool_button(ui,"上",!self.hits.is_empty()){self.goto_hit(false);}if Self::tool_button(ui,"下",!self.hits.is_empty()){self.goto_hit(true);}ui.add_sized([180.0,24.0],egui::TextEdit::singleline(&mut self.replace).font(sans(12.0)).hint_text("替换为"));if Self::tool_button(ui,"全部替换",!self.search.is_empty()){self.do_replace_all();}let a=ui.checkbox(&mut self.case_sensitive,"Aa").changed();let b=ui.checkbox(&mut self.whole_word,"全词").changed();if a||b{self.do_search();}ui.label(egui::RichText::new(if self.hits_truncated{format!("{}+",self.hits.len())}else{self.hits.len().to_string()}).font(sans(12.0)).color(th::TEXT_DIM));});}
    fn sidebar(&mut self,ui:&mut egui::Ui){ui.label(egui::RichText::new("文件").font(sans(11.0)).color(th::TEXT_DIM));let current=self.ed.path.clone();if let Some(l)=&self.listing{ui.label(egui::RichText::new(l.dir.to_string_lossy()).font(sans(10.0)).color(th::TEXT_DIM));egui::ScrollArea::vertical().show(ui,|ui|{for e in &l.entries{let fill=if current.as_deref()==Some(e.path.as_path()){th::ACCENT.gamma_multiply(0.35)}else{egui::Color32::TRANSPARENT};let b=egui::Button::new(egui::RichText::new(if e.is_dir{format!("{}/",e.name)}else{e.name.clone()}).font(sans(12.0)).color(if e.is_dir{egui::Color32::from_rgb(0x7a,0xb8,0xff)}else{th::TEXT})).fill(fill).min_size(egui::vec2(ui.available_width(),20.0));if ui.add(b).clicked(){if e.is_dir{self.load_dir(&e.path)}else{self.open_file(&e.path)}}}});}}
    fn jump_panel(&mut self,ui:&mut egui::Ui,rect:egui::Rect){let p=ui.painter_at(rect);p.rect_filled(rect,0.0,th::CHROME);let h=rect.height().floor().max(1.0)as u32;let Some(m)=JumpMap::new(h,self.ed.line_count())else{return};let(us,ue)=m.viewport_band(self.visible.0,self.visible.1.max(1));p.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.min.x,rect.min.y+us as f32),egui::pos2(rect.max.x,rect.min.y+ue as f32)),0.0,egui::Color32::from_rgba_unmultiplied(255,255,255,18));if !self.ed.is_huge(){let step=(self.ed.line_count()/h.max(1)as usize).max(1);let mut line_no=0;while line_no<self.ed.line_count(){if let Some((t,b))=m.line_band(line_no){let text=self.ed.line(line_no);if !text.trim().is_empty(){let y=rect.min.y+t as f32+(b-t)as f32/2.0;p.line_segment([egui::pos2(rect.min.x+3.0,y),egui::pos2(rect.max.x-3.0,y)],egui::Stroke::new(((b-t)as f32).min(2.0).max(1.0),egui::Color32::from_gray(92)));}}line_no+=step;}}if let Some((t,_))=m.line_band(self.ed.cursor.line){p.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.min.x,rect.min.y+t as f32),egui::pos2(rect.max.x,rect.min.y+t as f32+2.0)),0.0,th::ACCENT);}let r=ui.interact(rect,ui.id().with("jump"),egui::Sense::click_and_drag());if let Some(pos)=r.interact_pointer_pos(){self.ed.commit_undo_group();self.ed.cursor=self.ed.clamp(Pos::new(m.line_at((pos.y-rect.min.y).max(0.0)as u32),0));self.scroll_to=Some(self.ed.cursor.line);}}
    fn status_bar(&mut self,ui:&mut egui::Ui){let b=self.ed.status_bar();ui.horizontal_centered(|ui|{ui.add_space(12.0);ui.label(format!("{} · {} · {}",b.name,b.position_text(),b.size_text()));ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{for x in b.badges().into_iter().rev(){ui.label(x);}});});}

    fn draw_row(&mut self,ui:&mut egui::Ui,row:usize,row_h:f32,row_w:f32){let text=self.ed.line(row);let(spans,_)=highlight_line(&text,self.ed.lang,self.ed.state_at(row));let(rect,resp)=ui.allocate_exact_size(egui::vec2(row_w,row_h),egui::Sense::click_and_drag());let paint=ui.painter_at(rect);let text_x=rect.min.x+th::GUTTER_W;if self.ed.cursor.line==row{paint.rect_filled(rect,0.0,th::CURRENT_LINE);}paint.text(egui::pos2(text_x-12.0,rect.min.y),egui::Align2::RIGHT_TOP,format!("{:>w$}",row+1,w=th::LINE_NO_DIGITS),mono(),th::GUTTER_TEXT);let mut job=egui::text::LayoutJob::default();for s in &spans{job.append(&text[s.start..s.end],0.0,egui::TextFormat{font_id:mono(),color:color_for(s.kind),..Default::default()});}paint.galley(egui::pos2(text_x,rect.min.y),ui.painter().layout_job(job),th::TEXT);if let Some(p)=resp.interact_pointer_pos(){self.ed.commit_undo_group();self.ed.cursor=self.ed.clamp(Pos::new(row,Self::col_from_x(ui,&text,p.x-text_x)));self.ed.anchor=None;}}
    fn editor_area(&mut self,ui:&mut egui::Ui){ui.spacing_mut().item_spacing.y=0.0;let rh=ui.text_style_height(&egui::TextStyle::Monospace).max(th::FONT_SIZE);let total=self.ed.line_count().max(1);let vw=ui.available_width();let row_w=vw.max(th::GUTTER_W+500.0);let mut a=egui::ScrollArea::both().auto_shrink([false,false]);if let Some(l)=self.scroll_to.take(){a=a.vertical_scroll_offset((l as f32*rh-120.0).max(0.0));}let out=a.show_rows(ui,rh,total,|ui,r|{let x=(r.start,r.end-r.start);for row in r{self.draw_row(ui,row,rh,row_w);}x});self.visible=out.inner;}
    fn hairline(p:&egui::Painter,a:egui::Pos2,b:egui::Pos2){p.line_segment([a,b],egui::Stroke::new(1.0,th::HAIRLINE));}
    fn region(ui:&mut egui::Ui,r:egui::Rect,f:impl FnOnce(&mut egui::Ui)){let mut c=ui.new_child(egui::UiBuilder::new().max_rect(r).layout(egui::Layout::top_down(egui::Align::Min)));c.set_clip_rect(r);f(&mut c);}
}

impl eframe::App for YiEdit {
    fn ui(&mut self,ui:&mut egui::Ui,_:&mut eframe::Frame){let ctx=ui.ctx().clone();self.shot.tick(&ctx);self.guard_close(&ctx);self.handle_keys(&ctx);let full=ui.available_rect_before_wrap();let p=ui.painter().clone();p.rect_filled(full,0.0,th::BG);let top=full.min.y;let tool=egui::Rect::from_min_size(full.min,egui::vec2(full.width(),th::TOOLBAR_H));p.rect_filled(tool,0.0,th::CHROME);Self::hairline(&p,egui::pos2(full.min.x,tool.max.y),egui::pos2(full.max.x,tool.max.y));Self::region(ui,tool,|u|self.toolbar(u));let mut body_top=tool.max.y;if self.show_find{let f=egui::Rect::from_min_size(egui::pos2(full.min.x,body_top),egui::vec2(full.width(),th::FINDBAR_H));p.rect_filled(f,0.0,th::CHROME);Self::region(ui,f,|u|self.find_bar(u));body_top=f.max.y;}let status=egui::Rect::from_min_size(egui::pos2(full.min.x,full.max.y-th::STATUS_H),egui::vec2(full.width(),th::STATUS_H));p.rect_filled(status,0.0,th::CHROME);Self::region(ui,status,|u|self.status_bar(u));let body=egui::Rect::from_min_max(egui::pos2(full.min.x,body_top),egui::pos2(full.max.x,status.min.y));let mut l=body.min.x;let mut rr=body.max.x;if self.show_sidebar{let s=egui::Rect::from_min_max(egui::pos2(l,body.min.y),egui::pos2(l+th::SIDEBAR_W,body.max.y));p.rect_filled(s,0.0,th::CHROME);Self::region(ui,s,|u|self.sidebar(u));l=s.max.x+1.0;}if self.show_jump{let j=egui::Rect::from_min_max(egui::pos2(rr-th::JUMP_W,body.min.y),egui::pos2(rr,body.max.y));self.jump_panel(ui,j);rr=j.min.x-1.0;}Self::region(ui,egui::Rect::from_min_max(egui::pos2(l,body.min.y),egui::pos2(rr,body.max.y)),|u|self.editor_area(u));self.close_dialog(&ctx);self.editor_has_keys=ctx.memory(|m|m.focused()).is_none()&&!self.show_close_dialog;}
}

#[allow(dead_code)]
const _WELCOME_IS_SHARED:&str=WELCOME;
