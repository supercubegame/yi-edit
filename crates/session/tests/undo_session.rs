//! 会话层的撤销。这层存在的理由是两件副作用：光标要跳到被改动的地方，
//! 高亮缓存要从那行往后失效。两件忘了都不会报错：
//! 忘了前者，用户敲 Ctrl+Z 之后不知道改动发生在哪；
//! 忘了后者，块注释的颜色会停在撤销前的状态上。

use std::io::Write;

use yi_edit_session::Editor;

fn temp_file(tag: &str, body: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("yi-undo-session-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let path = dir.join("a.rs");
    let mut f = std::fs::File::create(&path).expect("建临时文件");
    f.write_all(body.as_bytes()).expect("写内容");
    path
}

#[test]
fn undo_moves_the_cursor_to_where_the_change_was() {
    let mut ed = Editor::empty();
    assert!(!ed.can_undo(), "刚打开就声称可撤销");
    ed.cursor = yi_edit_core::Pos::new(0, 0);
    assert!(ed.insert_text("abc"));
    let after = ed.cursor;
    assert!(ed.can_undo());

    // 光标先跑到别处，撤销必须把它拉回改动点。
    ed.cursor = yi_edit_core::Pos::new(0, 0);
    let back = ed.undo().expect("应该能撤销");
    assert_eq!(ed.cursor, back);
    assert_ne!(ed.cursor, after, "撤销后光标停在了插入末端，那个位置已经不存在了");
    assert!(ed.can_redo());

    let again = ed.redo().expect("应该能重做");
    assert_eq!(ed.cursor, again);
    assert_eq!(ed.cursor, after, "重做没回到插入末端");
}

/// 撤销必须带着高亮缓存一起失效。这一条是承重的：它不看颜色，看缓存长度，
/// 而缓存没截短的表现就是颜色停在旧状态上 —— 截图也看不出来。
#[test]
fn undo_invalidates_the_highlight_cache_from_the_changed_line() {
    let path = temp_file("states", "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\n");
    let mut ed = Editor::open(&path).expect("打开");
    let _ = ed.state_at(3);
    let warm = ed.cached_state_count();
    assert!(warm >= 4, "高亮缓存没暖起来（{warm}），这条断言就在测空气");

    ed.cursor = yi_edit_core::Pos::new(1, 0);
    assert!(ed.insert_text("/* 开个块注释 "));
    let _ = ed.state_at(3);
    let warm2 = ed.cached_state_count();
    assert!(warm2 >= 4);

    ed.undo().expect("应该能撤销");
    assert!(
        ed.cached_state_count() <= 2,
        "撤销之后高亮缓存还有 {} 项，第 1 行往后没有失效",
        ed.cached_state_count()
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("父目录"));
}

/// 大文件只读模式下撤销要返回 None，而不是 panic 也不是假装成功。
#[test]
fn undo_in_read_only_huge_mode_is_none_not_a_panic() {
    let path = temp_file("huge", "fn generated() {}\nfn more() {}\n");
    let mut ed = Editor::open_with_threshold(&path, 0).expect("打开");
    assert!(ed.is_huge());
    assert!(!ed.can_undo());
    assert!(!ed.can_redo());
    assert_eq!(ed.undo(), None);
    assert_eq!(ed.redo(), None);
    let _ = std::fs::remove_dir_all(path.parent().expect("父目录"));
}
