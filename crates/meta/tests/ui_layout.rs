//! 界面布局里的耦合参数，以及「那个交付阻断项真的接上了」。
//! 这些数字改一个必须重算另一个，而写在注释里的那句「改一个必须重算另一个」
//! 自己会腐化。
//!
//! 本文件里的扫描器已经踩过两次同一个坑：全文找子串会撞上无关表达式。

use yi_edit_meta as meta;

fn theme_const(name: &str) -> f64 {
    let src = meta::read("crates/app/src/theme.rs");
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with('#') || t.starts_with("//") { continue; }
        let Some(rest) = t.strip_prefix("pub const ") else { continue; };
        let Some((decl, value)) = rest.split_once('=') else { continue; };
        let ident = decl.split(':').next().unwrap_or_default().trim();
        if ident != name { continue; }
        return value.trim().trim_end_matches(';').trim().parse().unwrap_or_else(|_| panic!("{name} 不是纯数字"));
    }
    panic!("找不到 {name}");
}

fn fn_body<'a>(src: &'a str, name: &str) -> &'a str {
    let after = src.split(name).nth(1).unwrap_or_else(|| panic!("找不到 {name}"));
    after.split("\n    fn ").next().unwrap_or(after)
}

fn assignments_to(src: &str, field: &str) -> Vec<String> {
    let needle = format!("{field} =");
    meta::hits_in_code(src, &needle).into_iter().filter(|(_, text)| {
        let bytes = text.as_bytes(); let mut from = 0;
        while let Some(rel) = text[from..].find(&needle) {
            let eq = from + rel + needle.len() - 1;
            match bytes.get(eq + 1) { Some(b'>') | Some(b'=') => from = eq + 1, _ => return true }
        }
        false
    }).map(|(line,text)| format!("{line}: {text}")).collect()
}

fn thumbnail_loop(body: &str) -> Option<&str> {
    let start = body.find("while line_no < lines")?;
    let rest = &body[start..];
    let end = rest.find("line_no += step;")?;
    Some(&rest[..end])
}

#[test]
fn scanner_selftests() {
    assert_eq!(assignments_to("fn f(){ self.flag = true; }", "flag").len(), 1);
    assert!(assignments_to("match e { E::A if self.flag => g(), _ => {} }", "flag").is_empty());
    assert_eq!(assignments_to("E::A if self.flag => { self.flag = false; }", "flag").len(), 1);
    assert!(!meta::hits_in_code(thumbnail_loop("while line_no < lines { let x=map.line_at(y); line_no += step;").unwrap(), "line_at").is_empty());
    assert!(meta::hits_in_code(thumbnail_loop("while line_no < lines { let x=map.line_band(line_no); line_no += step;").unwrap(), "line_at").is_empty());
}

#[test]
fn layout_and_delivery_invariants() {
    let gutter=theme_const("GUTTER_W"); let digits=theme_const("LINE_NO_DIGITS"); let font=theme_const("FONT_SIZE"); let needed=digits*font*0.65+14.0;
    assert!(gutter>=needed && gutter<=needed*2.0 && digits>=6.0);
    let main=meta::read("crates/app/src/main.rs"); let min_w=main.lines().find(|l|l.contains("with_min_inner_size")).and_then(|l|{let s=l.find('[')?+1;let e=l[s..].find(',')?+s;l[s..e].trim().parse::<f64>().ok()}).unwrap(); let min_h=main.lines().find(|l|l.contains("with_min_inner_size")).and_then(|l|{let s=l.find(',')?+1;let e=l[s..].find(']')?+s;l[s..e].trim().parse::<f64>().ok()}).unwrap();
    assert!(theme_const("SIDEBAR_W")+theme_const("JUMP_W")+gutter<min_w/2.0);
    assert!(theme_const("TOOLBAR_H")+theme_const("FINDBAR_H")+theme_const("STATUS_H")<min_h/2.0);
    let src=meta::read("crates/app/src/ui_runtime.rs");
    assert!(src.contains("Event::Ime") && src.contains("IMERect") && src.contains("fn ime"));
    assert!(!meta::hits_in_code(fn_body(&src,"fn draw_row"),"available_width").is_empty() || src.contains("ui.available_width"));
    assert!(src.contains("while line_no<self.ed.line_count()"));
    let main=meta::read("crates/app/src/main.rs"); assert!(main.contains("ui_runtime.rs"));
}

#[test]
fn clipboard_close_and_focus_are_wired() {
    let src=meta::read("crates/app/src/ui_runtime.rs");
    for x in ["Event::Copy","Event::Cut","Event::Paste","select_all","close_requested","CancelClose","shot.active()"] { assert!(!meta::hits_in_code(&src,x).is_empty(),"缺少 {x}"); }
}
