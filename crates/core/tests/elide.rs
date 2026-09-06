//! 文本截短。
//!
//! 承重的一条是**列宽预算真的成立**：返回值的列宽永远 `<= budget`。
//! 而只验这一条的话，一个**永远返回空串**的实现也能完美交差，
//! 所以另一侧同样要断：装得下的文本必须**原样返回**，且文件名要保住。
//!
//! 还有一条对照实验盯着「为什么按列而不是按字符数」：同一段中文路径，
//! 按字符数给预算的话会真的溢出一倍——否则「所以改成了列宽」只是一句
//! 无从验证的声明。

use yi_edit_core::elide::{char_columns, columns, elide_path, elide_start, ELLIPSIS};

/// 真实截图里那段路径（就是它挤成了三行）。
const WIN_PATH: &str = "E:\\WJSN\\2025\\260902\\yi-edit-v1.1.0-windows-x86_64\\README.md";
const CJK_PATH: &str = "D:\\\u9879\u76ee\\\u6d4b\u8bd5\u76ee\u5f55\\\u4e2d\u6587\u6587\u4ef6\u540d.md";

#[test]
fn wide_characters_really_count_as_two_columns() {
    for c in ['\u4e2d', '\u6587', '\u3042', '\uff21', '\ud55c'] {
        assert_eq!(char_columns(c), 2, "{c} 没被当成宽字符");
    }
    for c in ['a', 'Z', '0', '/', '\\', '.', '-', ' ', ELLIPSIS] {
        assert_eq!(char_columns(c), 1, "{c} 被当成了宽字符");
    }
    assert_eq!(columns("ab"), 2);
    assert_eq!(columns("\u4e2d\u6587"), 4);
    assert_eq!(columns(""), 0);
}

/// 预算永不溢出。对四种语料 × 每一个预算逐个验，而不是抽几个好看的。
#[test]
fn the_column_budget_is_never_exceeded() {
    let samples = [WIN_PATH, CJK_PATH, "short.rs", "\u4e2d\u6587", "", "a"];
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

/// 对照侧：装得下就不能动。
/// 不验这一条的话，一个永远返回空串（或永远返回省略号）的实现也能过。
#[test]
fn text_that_fits_is_returned_untouched() {
    for s in [WIN_PATH, CJK_PATH, "short.rs", "\u4e2d\u6587"] {
        let need = columns(s);
        assert_eq!(elide_start(s, need), s, "刚刚装得下却被动了");
        assert_eq!(elide_start(s, need + 10), s, "预算宽裕却被动了");
        assert_eq!(elide_path(s, need), s);
        assert!(
            !elide_start(s, need).contains(ELLIPSIS),
            "没超预算却加了省略号"
        );
    }
}

/// 截短时尾部原样保留：返回值去掉省略号之后必须是原串的后缀。
/// 从尾部切的话，侧栏会变成一列长得一模一样的目录前缀 —— 那比不截还难用。
#[test]
fn the_tail_survives_and_the_head_is_what_gets_cut() {
    let out = elide_start(WIN_PATH, 20);
    assert!(out.starts_with(ELLIPSIS), "省略号不在开头：{out:?}");
    let kept = out.strip_prefix(ELLIPSIS).expect("上面已经断过");
    assert!(
        WIN_PATH.ends_with(kept),
        "保留的部分不是原串的后缀：{kept:?}"
    );
    assert!(!kept.is_empty(), "预算够却一个字也没留");
}

/// 路径优先保文件名：只要文件名加省略号装得下，它就必须完整出现。
#[test]
fn the_file_name_is_kept_whole_whenever_it_fits() {
    let budget = columns("README.md") + 1;
    let out = elide_path(WIN_PATH, budget);
    assert!(
        out.ends_with("README.md"),
        "文件名没保住：{out:?}"
    );
    assert!(columns(&out) <= budget);

    // 连文件名都装不下时，仍然保尾（扩展名比前缀有用）。
    let tiny = elide_path(WIN_PATH, 6);
    assert!(columns(&tiny) <= 6);
    assert!(tiny.ends_with(".md"), "极端预算下连扩展名都没保住：{tiny:?}");
}

/// **对照实验：按字符数给预算会真的溢出。**
/// 没有这一条的话，「所以改成了列宽」只是一句无从验证的声明。
#[test]
fn a_char_count_budget_really_would_have_overflowed() {
    let budget = 12usize;
    // 按字符数切（错的做法）：取最后 budget 个字符。
    let by_chars: String = CJK_PATH
        .chars()
        .rev()
        .take(budget)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    assert_eq!(by_chars.chars().count(), budget);
    assert!(
        columns(&by_chars) > budget,
        "这段语料里宽字符不够多，对照实验没比到东西（{} 列）",
        columns(&by_chars)
    );
    // 按列切（对的做法）：不溢出。
    assert!(columns(&elide_path(CJK_PATH, budget)) <= budget);
}

/// 边界都不得 panic，而且幂等：已经截过的串再截一次不变。
#[test]
fn edges_do_not_panic_and_eliding_twice_changes_nothing() {
    assert_eq!(elide_start(WIN_PATH, 0), "");
    assert_eq!(elide_path(WIN_PATH, 0), "");
    assert_eq!(elide_start(WIN_PATH, 1), ELLIPSIS.to_string());
    assert_eq!(elide_start("", 10), "");

    for budget in [4usize, 12, 20, 33] {
        let once = elide_path(WIN_PATH, budget);
        let twice = elide_path(&once, budget);
        assert_eq!(once, twice, "幂等不成立：预算 {budget}");
        let once_cjk = elide_path(CJK_PATH, budget);
        assert_eq!(elide_path(&once_cjk, budget), once_cjk, "中文下幂等不成立");
    }
}

/// 宽字符边界：只剩一列而下一个是宽字符时，宁可少放也不能超。
#[test]
fn a_wide_character_is_never_half_admitted() {
    // 省略号 1 列 + 一个宽字符 2 列 = 3；预算 2 时只能放下省略号。
    let out = elide_start("\u4e2d\u6587\u4e2d\u6587", 2);
    assert!(columns(&out) <= 2, "{out:?} 占了 {} 列", columns(&out));
    let out3 = elide_start("\u4e2d\u6587\u4e2d\u6587", 3);
    assert_eq!(columns(&out3), 3, "预算 3 列时应该能放下省略号加一个宽字符");
    assert!(out3.ends_with('\u6587'));
}
