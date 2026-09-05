//! 保存路径上两个会丢数据的形状，它们都不会报错。
//!
//! 1. 关窗对话框的「保存并退出」无条件退出：刚启动的文档没有路径，保存必定失败，
//!    而它照样关窗 —— 用户那一段字直接没了，而他点的按钮写着“保存”。
//! 2. 新建时静默丢掉未保存的文档。
//!
//! 两条都只能用结构断言守：它们在 GUI 事件循环里，而快闸门不编 GUI。
//! 结构断言验不了行为，但它至少能验「守卫还在不在」。

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
/// 「某段里有没有 X」必须先把那段切出来：全文搜的话，别的按钮里的
/// `force_close` 会让这条断言误报，而误报会逐人去改产品迎合尺子。
fn segment(src: &str, needle: &str, stop: &str) -> String {
    let start = src
        .find(needle)
        .unwrap_or_else(|| panic!("编译进去的 UI 里找不到：{needle}"));
    let rest = &src[start + needle.len()..];
    let end = rest.find(stop).unwrap_or(rest.len());
    rest[..end].to_string()
}

#[test]
fn save_and_quit_only_quits_when_the_save_really_succeeded() {
    let src = compiled_ui();
    // “保存并退出”那一段，到下一个按钮为止。
    let seg = segment(&src, "保存并退出", "不保存退出");
    assert!(
        seg.contains("save_current()"),
        "「保存并退出」没走那个会报告成败的保存路径：{seg}"
    );
    assert!(
        seg.contains("&& self.save_current()"),
        "「保存并退出」没拿保存结果做门禁，保存失败也会退（用户的字就没了）：{seg}"
    );
    // 负向：那一段里不得出现把错误丢掉的写法。
    assert!(
        !seg.contains("let _ = self.ed.save()"),
        "那一段里把保存的错误丢掉了，于是它又变成了无条件退出：{seg}"
    );

    // 对照侧：“不保存退出”本来就应该无条件退。两侧一样的话，上面那条只是在匹配一个字符串。
    let discard = segment(&src, "不保存退出", "取消");
    assert!(
        discard.contains("force_close = true") && !discard.contains("save_current()"),
        "「不保存退出」居然也要先保存，那两个按钮就没区别了：{discard}"
    );
}

/// 没路径时的 Ctrl+S 必须落到另存为，否则刚启动敲的字永远存不了。
#[test]
fn saving_without_a_path_falls_back_to_save_as() {
    let src = compiled_ui();
    let seg = segment(&src, "fn save_current", "fn save_as_typed");
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
    let seg = segment(&src, "fn new_file", "fn insert");
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
    let save_as = segment(&src, "pub fn save_as", "fn write_to");
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
