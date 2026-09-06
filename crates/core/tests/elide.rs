//! 缩略的断言。
//!
//! 两条承重的：
//! 1. **缩完之后绝不能超过上限**（模糊扫过所有上限值）。超了不会报错，
//!    只是侧栏照样被挤压 —— 也就是这个模块要修的那个问题原封不动地回来。
//! 2. **多字节不能切开**。按字节切一刀直接 panic，而 Windows 路径里很容易有中文目录名。
//!
//! 双向：装得下时必须原样返回（不加省略号），装不下时必须真的缩了。
//! 只验一侧的话，一个无条件加省略号的实现、或一个什么都不做的实现，都能完美交差。

use yi_edit_core::elide::{elide_middle, elide_path, fit_chars, last_segment, ELLIPSIS};

/// 语料里必须真的有多字节路径，否则那条字符边界断言在测空气。
fn corpora() -> Vec<String> {
    vec![
        String::from("E:\\WJSN\\2025\\260902\\yi-edit-v1.1.0-windows-x86_64"),
        String::from("/home/user/项目/超长目录名称/子目录/文件.rs"),
        String::from("短"),
        String::new(),
        String::from("/"),
        String::from("a/"),
        String::from("中文中文中文中文"),
        String::from("no-separators-at-all-but-quite-long-indeed"),
    ]
}

#[test]
fn the_corpus_really_contains_multibyte_paths() {
    let multibyte = corpora()
        .iter()
        .filter(|s| s.len() > s.chars().count())
        .count();
    assert!(
        multibyte >= 2,
        "语料里只有 {multibyte} 个多字节串，字符边界那几条断言在测空气"
    );
}

/// **模糊不变量：输出字符数永远不超上限，且不 panic。**
/// 扫过所有上限值（包括 0 与 1，以及比原串还长的）。
#[test]
fn the_result_never_exceeds_the_limit_and_never_panics() {
    let mut checked = 0usize;
    for s in corpora() {
        let n = s.chars().count();
        for max in 0..n + 6 {
            for (label, got) in [
                ("elide_middle", elide_middle(&s, max)),
                ("elide_path", elide_path(&s, max)),
            ] {
                let len = got.chars().count();
                assert!(
                    len <= max,
                    "{label}({s:?}, {max}) 给出 {len} 个字符：{got:?} —— 超过上限不会报错，只会把侧栏挤压回去"
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 200, "只扫了 {checked} 种组合，语料或上限范围缩水了");
}

/// 双向：装得下原样返回；装不下真的缩了并带省略号。
#[test]
fn it_only_elides_when_it_has_to_and_really_elides_when_it_does() {
    for s in corpora() {
        let n = s.chars().count();
        // 装得下：原样，不能凭空多一个省略号。
        for max in [n, n + 1, n + 5] {
            assert_eq!(elide_middle(&s, max), s, "装得下却改了内容（中间省略）");
            assert_eq!(elide_path(&s, max), s, "装得下却改了内容（路径省略）");
        }
        // 装不下：必须变短并带省略号。空串永远装得下，跳过。
        if n == 0 {
            continue;
        }
        for max in 1..n {
            for (label, got) in [
                ("elide_middle", elide_middle(&s, max)),
                ("elide_path", elide_path(&s, max)),
            ] {
                assert!(
                    got.chars().count() < n,
                    "{label}({s:?}, {max}) 没有变短：{got:?}"
                );
                assert!(
                    got.contains(ELLIPSIS),
                    "{label}({s:?}, {max}) 缩了却没有省略号，用户会以为那就是完整路径：{got:?}"
                );
            }
        }
    }
}

/// 结果必须是「原串的一个前缀 + 省略号 + 原串的一个后缀」。
///
/// 这一条比「不 panic」强：一个把多字节字符切坏并用 U+FFFD 补上的实现不会 panic，
/// 但屏幕上会多出一个菱形问号。
#[test]
fn the_pieces_come_verbatim_from_the_original() {
    for s in corpora() {
        let n = s.chars().count();
        if n < 3 {
            continue;
        }
        for max in 2..n {
            let got = elide_middle(&s, max);
            let (head, tail) = got
                .split_once(ELLIPSIS)
                .unwrap_or_else(|| panic!("{got:?} 里没有省略号"));
            assert!(s.starts_with(head), "头部不是原串的前缀：{head:?}");
            assert!(s.ends_with(tail), "尾部不是原串的后缀：{tail:?}");
            assert!(
                !got.contains('\u{fffd}'),
                "结果里出现了替换字符，说明把多字节字符切坏了：{got:?}"
            );
        }
    }
}

#[test]
fn the_last_segment_is_what_a_human_would_call_it() {
    assert_eq!(last_segment("E:\\a\\b\\name"), Some("name"));
    assert_eq!(last_segment("/a/b/文件.rs"), Some("文件.rs"));
    assert_eq!(last_segment("bare"), None, "没有分隔符时不应该当成末段");
    assert_eq!(last_segment("/a/b/"), None, "末尾是分隔符时没有末段");
    assert_eq!(last_segment(""), None);
    // 多字节分隔符边上：紧跟在中文后面的分隔符不能把偏移算错。
    assert_eq!(last_segment("/项目/子目录"), Some("子目录"));
}

/// 路径缩略优先保末段；末段自己都装不下时退回中间省略（而不是超出上限）。
#[test]
fn a_path_keeps_its_last_segment_when_that_fits() {
    let p = "E:\\WJSN\\2025\\260902\\yi-edit-v1.1.0-windows-x86_64";
    let got = elide_path(p, 40);
    assert_eq!(got.chars().count(), 40);
    assert!(
        got.ends_with("yi-edit-v1.1.0-windows-x86_64"),
        "末段没保住，而末段恰好是看文件列表的人最需要的那一段：{got:?}"
    );
    // 对照：中间省略会把末段也切掉一半。两者不同才说明「优先保末段」真的生效了。
    assert_ne!(
        elide_middle(p, 40),
        got,
        "路径缩略与中间省略结果一样，那前者就是一层装饰"
    );

    // 末段太长：退回中间省略，但仍不得超过上限。
    let long_tail = format!("/a/{}", "x".repeat(50));
    let got = elide_path(&long_tail, 10);
    assert_eq!(got.chars().count(), 10, "退回路径上超了上限：{got:?}");
    assert!(got.contains(ELLIPSIS));
}

/// 估算函数只用来防挤压，不保证像素级对齐。但它不得在边界上发疯。
#[test]
fn the_fit_estimate_is_monotonic_and_safe_at_the_edges() {
    assert_eq!(fit_chars(0.0, 13.0), 0);
    assert_eq!(fit_chars(-100.0, 13.0), 0, "负宽度没归零");
    assert_eq!(fit_chars(100.0, 0.0), 0, "字号为零时除了零");
    assert_eq!(fit_chars(100.0, -13.0), 0, "负字号没归零");
    let a = fit_chars(200.0, 13.0);
    let b = fit_chars(400.0, 13.0);
    assert!(a > 0, "200px / 13pt 算出了零个字符，侧栏会变成一堆省略号");
    assert!(b > a, "宽度变大而能装的字符数没变多：{a} -> {b}");
    // 同一个宽度下，字号越大能装的越少。
    assert!(fit_chars(200.0, 26.0) < a, "字号变大而装得不变少");
}
