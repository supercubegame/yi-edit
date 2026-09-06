//! 文本截短的列宽与信息保留测试。
use yi_edit_core::elide::{char_columns, columns, elide_path, elide_start, ELLIPSIS};

const WIN_PATH: &str = "E:\\WJSN\\2025\\260902\\yi-edit-v1.1.0-windows-x86_64\\README.md";
const CJK_PATH: &str = "D:\\项目\\测试目录\\中文文件名.md";

#[test]
fn wide_characters_really_count_as_two_columns() {
    for c in ['\u{4e2d}', '\u{6587}', '\u{3042}', '\u{ff21}', '\u{d55c}'] {
        assert_eq!(char_columns(c), 2, "{c} 没被当成宽字符");
    }
    for c in ['a', 'Z', '0', '/', '\\', '.', '-', ' ', ELLIPSIS] {
        assert_eq!(char_columns(c), 1, "{c} 被当成了宽字符");
    }
    assert_eq!(columns("ab"), 2);
    assert_eq!(columns("中文"), 4);
    assert_eq!(columns(""), 0);
}

#[test]
fn the_column_budget_is_never_exceeded() {
    let samples = [WIN_PATH, CJK_PATH, "short.rs", "中文", "", "a"];
    let mut checked = 0usize;
    for s in samples {
        for budget in 0..48usize {
            for out in [elide_start(s, budget), elide_path(s, budget)] {
                assert!(
                    columns(&out) <= budget,
                    "预算 {budget} 列却输出了 {} 列：{out:?}",
                    columns(&out)
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 500, "只验了 {checked} 个组合，语料太稀");
}

#[test]
fn text_that_fits_is_returned_untouched() {
    for s in [WIN_PATH, CJK_PATH, "short.rs", "中文"] {
        let need = columns(s);
        assert_eq!(elide_start(s, need), s);
        assert_eq!(elide_start(s, need + 10), s);
        assert_eq!(elide_path(s, need), s);
        assert!(!elide_start(s, need).contains(ELLIPSIS));
    }
}

#[test]
fn the_tail_survives_and_the_head_is_what_gets_cut() {
    let out = elide_start(WIN_PATH, 20);
    assert!(out.starts_with(ELLIPSIS));
    let kept = out.strip_prefix(ELLIPSIS).unwrap();
    assert!(WIN_PATH.ends_with(kept));
    assert!(!kept.is_empty());
}

#[test]
fn the_file_name_is_kept_whole_whenever_it_fits() {
    let budget = columns("README.md") + 1;
    let out = elide_path(WIN_PATH, budget);
    assert!(out.ends_with("README.md"), "文件名没保住：{out:?}");
    assert!(columns(&out) <= budget);
    let tiny = elide_path(WIN_PATH, 6);
    assert!(columns(&tiny) <= 6);
    assert!(tiny.ends_with(".md"));
}

/// 对照实验：按字符数给中文路径预算真的会溢出，按列宽不会。
#[test]
fn a_char_count_budget_really_would_have_overflowed() {
    let budget = 12usize;
    let by_chars: String = CJK_PATH
        .chars()
        .rev()
        .take(budget)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    assert_eq!(by_chars.chars().count(), budget);
    assert!(columns(&by_chars) > budget);
    assert!(columns(&elide_path(CJK_PATH, budget)) <= budget);
}

#[test]
fn edges_do_not_panic_and_eliding_twice_changes_nothing() {
    assert_eq!(elide_start(WIN_PATH, 0), "");
    assert_eq!(elide_path(WIN_PATH, 0), "");
    assert_eq!(elide_start(WIN_PATH, 1), ELLIPSIS.to_string());
    assert_eq!(elide_start("", 10), "");
    for budget in [4usize, 12, 20, 33] {
        let once = elide_path(WIN_PATH, budget);
        assert_eq!(elide_path(&once, budget), once);
        let once_cjk = elide_path(CJK_PATH, budget);
        assert_eq!(elide_path(&once_cjk, budget), once_cjk);
    }
}

#[test]
fn a_wide_character_is_never_half_admitted() {
    let out = elide_start("中文中文", 2);
    assert!(columns(&out) <= 2);
    let out3 = elide_start("中文中文", 3);
    assert_eq!(columns(&out3), 3);
    assert!(out3.ends_with('文'));
}
