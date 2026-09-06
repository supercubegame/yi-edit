//! 状态栏的断言。它显示错一个数字不会报错，而用户会信它。

use yi_edit_core::{Eol, Lang, Pos};
use yi_edit_session::status::{char_column, selected_chars, StatusBar};

fn lines(text: &str) -> Vec<String> {
    text.split('\n').map(|s| s.to_string()).collect()
}

/// 列号按**字符**而不是字节。按字节的话，光标放在一个中文字后面会显示「第 4 列」。
#[test]
fn column_counts_characters_not_bytes() {
    let l = "a中文b";
    assert!(
        l.len() > l.chars().count(),
        "语料全是单字节，这条断言在测空气"
    );
    assert_eq!(char_column(l, 0), 1, "行首应该是第 1 列");
    assert_eq!(char_column(l, 1), 2, "a 后面是第 2 列");
    assert_eq!(char_column(l, 4), 3, "中 后面是第 3 列，不是第 5 列");
    assert_eq!(char_column(l, 7), 4, "文 后面是第 4 列");
    assert_eq!(char_column(l, 8), 5, "行尾是第 5 列");
    // 非字符边界与越界都要夹到合法位置，而不是 panic。
    assert_eq!(char_column(l, 2), 2, "非字符边界应该向左夹");
    assert_eq!(char_column(l, 999), 5, "越界应该夹到行尾");
}

#[test]
fn selection_char_count_handles_single_and_multi_line() {
    let ls = lines("a中文b\ncd\nef");
    assert_eq!(
        selected_chars(&ls, Pos::new(0, 0), Pos::new(0, 0)),
        0,
        "空选区应该是 0"
    );
    // 行内：从行首到 中 之后 = 2 个字符（a 与 中）。
    assert_eq!(selected_chars(&ls, Pos::new(0, 0), Pos::new(0, 4)), 2);
    // 反向选区结果相同（否则往上拖选会显示 0）。
    assert_eq!(selected_chars(&ls, Pos::new(0, 4), Pos::new(0, 0)), 2);
    // 跨行：行 0 剩下 4 个字符 + 换行 = 5，加行 1 的前 1 个字符 = 6。
    assert_eq!(selected_chars(&ls, Pos::new(0, 0), Pos::new(1, 1)), 6);
    // 跨两行以上：中间整行计入。
    assert_eq!(selected_chars(&ls, Pos::new(0, 8), Pos::new(2, 0)), 4);
}

fn bar(read_only: bool, dirty: bool, selected: Option<usize>) -> StatusBar {
    StatusBar {
        name: String::from("a.rs"),
        line: 12,
        column: 5,
        total_lines: 1138,
        total_bytes: 46621,
        selected_chars: selected,
        eol: Eol::Lf,
        lang: Lang::Rust,
        read_only,
        dirty,
    }
}

/// 只读与未保存不能同时出现：只读模式下根本没有未保存的修改，
/// 两个标签一起亮的话用户会以为自己的修改丢了。
#[test]
fn read_only_and_dirty_are_mutually_exclusive_badges() {
    let ro = bar(true, true, None).badges();
    assert!(ro.contains(&String::from("只读")), "只读标签没亮：{ro:?}");
    assert!(
        !ro.contains(&String::from("未保存")),
        "只读模式却亮了未保存：{ro:?}"
    );

    let d = bar(false, true, None).badges();
    assert!(d.contains(&String::from("未保存")), "未保存标签没亮：{d:?}");

    let clean = bar(false, false, None).badges();
    assert!(
        !clean.contains(&String::from("未保存")),
        "干净时却亮了未保存"
    );
    assert!(!clean.contains(&String::from("只读")), "可写时却亮了只读");

    // 行尾风格与语言总要有，否则状态栏就只剩一半信息。
    assert!(clean.contains(&String::from("LF")), "{clean:?}");
    assert!(clean.contains(&String::from("RUST")), "{clean:?}");

    let sel = bar(false, false, Some(42)).badges();
    assert!(
        sel.iter().any(|s| s.contains("42")),
        "有选区却没显示字符数：{sel:?}"
    );
}

#[test]
fn crlf_is_shown_as_crlf() {
    let mut b = bar(false, false, None);
    b.eol = Eol::Crlf;
    assert!(
        b.badges().contains(&String::from("CRLF")),
        "{:?}",
        b.badges()
    );
}

/// 行号列号从 1 开始（界面上行号从 1 起，内部从 0 起，这一层转换很容易差一）。
#[test]
fn position_and_size_text_are_human_readable() {
    let b = bar(false, false, None);
    assert_eq!(b.position_text(), "行 12 列 5");
    assert!(b.size_text().contains("1138 行"), "{}", b.size_text());
    assert!(b.size_text().contains("46621 字节"), "{}", b.size_text());

    let mut big = bar(false, false, None);
    big.total_bytes = 70 * 1024 * 1024;
    let s = big.size_text();
    assert!(s.contains("MB"), "大文件应该转成 MB：{s}");
    assert!(!s.contains("字节"), "转成 MB 之后不该再带精确字节数：{s}");
}
