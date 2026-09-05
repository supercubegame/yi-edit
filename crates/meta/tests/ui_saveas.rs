//! 保存路径上两个会丢数据的形状，它们都不会报错。
//!
//! 1. 关窗对话框的「保存并退出」无条件退出：刚启动的文档没有路径，保存必定失败，
//!    而它照样关窗 —— 用户那一段字直接没了，而他点的按钮写着“保存”。
//! 2. 新建时静默丢掉未保存的文档。
//!
//! 两条都只能用结构断言守：它们在 GUI 事件循环里，而快闸门不编 GUI。
//! 结构断言验不了行为，但它至少能验「守卫还在不在」。
//!
//! **这份文件自己第一版就踩了那个坑：** 锢点拿的是按钮文字，而同一串文字在上方的
//! 文档注释里也出现过，于是切出来的“那一段”从注释一直延到几百行之后——
//! 负向那条仍然绿，但绿的理由变成了“跨度太大所以没命中”。
//! 修法：锢点一律用**代码形状**（`ui.button("…")`、`fn f(&mut self)`），
//! 并且先断言它在全文里**只出现一次**。

use yi_edit_meta as meta;

fn compiled_ui() -> String {
    let main = meta::read("crates/app/src/main.rs");
    let path = main
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("#[path = \"")
                .and_then(|s| s.strip_suffix("\"]"))
        })
        .unwrap_or("ui.rs");
    meta::read(&format!("crates/app/src/{path}"))
}

/// 从 `needle` 开始到下一个 `stop` 之前的那一段。
///
/// `needle` 必须在全文里只出现一次：否则切出来的跨度不是我以为的那一段，
/// 而一个跨度太大的区间会让负向断言免费通过。
fn segment(src: &str, needle: &str, stop: &str) -> String {
    let n = src.matches(needle).count();
    assert_eq!(
        n, 1,
        "锢点 {needle:?} 在全文里出现 {n} 次，切出来的跨度不可信"
    );
    let start = src.find(needle).expect("上面已经数过了");
    let rest = &src[start + needle.len()..];
    let end = rest.find(stop).unwrap_or(rest.len());
    let seg = rest[..end].to_string();
    assert!(
        !seg.is_empty(),
        "切出来的那一段是空的，后面每条断言都会免费通过"
    );
    seg
}

#[test]
fn save_and_quit_only_quits_when_the_save_really_succeeded() {
    let src = compiled_ui();
    let seg = segment(
        &src,
        "ui.button(\"保存并退出\")",
        "ui.button(\"不保存退出\")",
    );
    assert!(
        seg.contains("&& self.save_current()"),
        "「保存并退出」没拿保存结果做门禁，保存失败也会退（用户的字就没了）：{seg}"
    );
    // 负向：那一段里不得把保存的错误丢掉。
    assert!(
        !seg.contains("let _ = self.ed.save()"),
        "那一段里把保存的错误丢掉了，于是它又变成了无条件退出：{seg}"
    );
    // 跨度自证：这一段应该就是那一个按钮，而不是半个文件。
    assert!(
        seg.lines().count() <= 8,
        "切出来的那一段有 {} 行，跨度太大，负向断言会免费通过",
        seg.lines().count()
    );

    // 对照侧：“不保存退出”本来就应该无条件退。两侧一样的话，上面那条只是在匹配一个字符串。
    let discard = segment(
        &src,
        "ui.button(\"不保存退出\")",
        "ui.button(\"取消\")",
    );
    assert!(
        discard.contains("force_close = true") && !discard.contains("save_current()"),
        "「不保存退出」居然也要先保存，那两个按钮就没区别了：{discard}"
    );
}

/// 没路径时的 Ctrl+S 必须落到另存为，否则刚启动敲的字永远存不了。
#[test]
fn saving_without_a_path_falls_back_to_save_as() {
    let src = compiled_ui();
    let seg = segment(
        &src,
        "fn save_current(&mut self) -> bool",
        "fn save_as_typed(&mut self) -> bool",
    );
    assert!(
        seg.contains("self.ed.path.is_some()") && seg.contains("save_as("),
        "save_current 没处理「还没有路径」那个分支：{seg}"
    );
    assert!(
        seg.contains("Err(e)"),
        "save_current 没把失败告诉任何人：{seg}"
    );
}

/// 新建不得静默丢掉未保存的文档。
#[test]
fn new_file_refuses_to_discard_unsaved_work() {
    let src = compiled_ui();
    let seg = segment(&src, "fn new_file(&mut self)", "fn insert(&mut self");
    assert!(
        seg.contains("is_dirty()"),
        "新建没问过有没有未保存的修改：{seg}"
    );
    assert!(
        seg.contains("return"),
        "问了却没拦住，那与没问一样：{seg}"
    );
}

/// 会话层真的提供了这两个入口，而且另存为真的重算了语言。
/// 只守 UI 侧的话，一个什么都不做的 `save_as` 也能让上面那几条全绿。
#[test]
fn the_session_layer_backs_those_entry_points() {
    let src = meta::read("crates/session/src/lib.rs");
    for needle in ["pub fn new_file", "pub fn save_as", "pub fn save"] {
        assert!(
            !meta::hits_in_code(&src, needle).is_empty(),
            "会话层里没有 {needle}"
        );
    }
    let save_as = segment(
        &src,
        "pub fn save_as(&mut self, path: &Path)",
        "fn write_to(&mut self, path: &Path)",
    );
    assert!(
        save_as.contains("lang_from_path"),
        "另存为没重新认语言，另存成 .rs 之后颜色会一直停在旧语言上：{save_as}"
    );
    assert!(
        save_as.contains("states.clear()"),
        "语言变了却没丢高亮缓存：{save_as}"
    );
    let tests = meta::read("crates/session/tests/saveas.rs");
    assert!(
        tests.contains("save_as_on_a_read_only_huge_file_refuses_and_writes_nothing"),
        "只读模式另存为那条断言没了"
    );
    assert!(
        tests.contains("同语言另存"),
        "语言重算只验了一侧，一个每次保存都清空缓存的实现也能完美交差"
    );
}
