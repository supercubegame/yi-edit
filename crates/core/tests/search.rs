use yi_edit_core::search::is_word_byte;
use yi_edit_core::{find_all, find_before, find_from, SearchOptions};

/// 第二把尺子：朴素的逐位置比较。故意不复用任何被测代码，
/// 否则 BMH 的跳表写错时两边会一起错，而断言依旧全绿。
fn naive_find_all(hay: &[u8], needle: &[u8], opts: SearchOptions) -> Vec<usize> {
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > hay.len() {
        return out;
    }
    let mut i = 0usize;
    while i + needle.len() <= hay.len() {
        let win = &hay[i..i + needle.len()];
        let eq = if opts.case_sensitive {
            win == needle
        } else {
            win.eq_ignore_ascii_case(needle)
        };
        let bounded = !opts.whole_word
            || ((i == 0 || !is_word_byte(hay[i - 1]))
                && (i + needle.len() >= hay.len() || !is_word_byte(hay[i + needle.len()])));
        if eq && bounded {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

const CASES: &[(&str, &str)] = &[
    ("hello world hello", "hello"),
    ("HeLLo WORLD hello", "hello"),
    ("aaaa", "aa"),
    ("abcabcabc", "cab"),
    ("foo foobar barfoo foo_bar foo", "foo"),
    ("中文abc中文测试中文", "中文"),
    ("混搭mixed内容MIXED尾巴", "mixed"),
    ("", "x"),
    ("short", "muchlongerneedle"),
    ("line1\nline2\nline1", "line1"),
];

const OPTS: &[SearchOptions] = &[
    SearchOptions {
        case_sensitive: true,
        whole_word: false,
    },
    SearchOptions {
        case_sensitive: false,
        whole_word: false,
    },
    SearchOptions {
        case_sensitive: true,
        whole_word: true,
    },
    SearchOptions {
        case_sensitive: false,
        whole_word: true,
    },
];

#[test]
fn bmh_agrees_with_a_second_independent_implementation() {
    let mut total_hits = 0usize;
    for (hay, needle) in CASES {
        for opts in OPTS {
            let got = find_all(hay.as_bytes(), needle.as_bytes(), *opts);
            let want = naive_find_all(hay.as_bytes(), needle.as_bytes(), *opts);
            assert_eq!(got, want, "hay={hay:?} needle={needle:?} opts={opts:?}");
            total_hits += got.len();
        }
    }
    // 空断言守卫：如果语料里一个匹配都没有，上面那堆 assert_eq 全是 0 == 0。
    assert!(
        total_hits > 20,
        "语料总匹配数太少（{total_hits}），这批断言在测空气"
    );
}

#[test]
fn matches_do_not_overlap() {
    assert_eq!(find_all(b"aaa", b"aa", SearchOptions::exact()), vec![0]);
    assert_eq!(find_all(b"aaaa", b"aa", SearchOptions::exact()), vec![0, 2]);
}

#[test]
fn whole_word_rejects_partial_hits() {
    let opts = SearchOptions {
        case_sensitive: true,
        whole_word: true,
    };
    assert_eq!(find_all(b"foo foobar", b"foo", opts), vec![0]);
    assert_eq!(find_all(b"foo_bar", b"foo", opts), Vec::<usize>::new());
}

/// 在合法 UTF-8 里找合法 UTF-8，匹配位置必须落在字符边界上。
/// 不测这一条的话，一个跨字符的匹配会在切片时 panic，而不是默默算错。
#[test]
fn matches_land_on_char_boundaries() {
    let hay = "中文abc中文测试中文";
    for needle in ["中文", "abc", "测试"] {
        let hits = find_all(hay.as_bytes(), needle.as_bytes(), SearchOptions::exact());
        assert!(
            !hits.is_empty(),
            "needle={needle} 没找到，这条断言会变成空的"
        );
        for p in hits {
            assert!(hay.is_char_boundary(p), "匹配偏移 {p} 不在字符边界上");
            assert!(
                hay.is_char_boundary(p + needle.len()),
                "匹配末端 {} 不在字符边界上",
                p + needle.len()
            );
        }
    }
}

#[test]
fn find_from_and_find_before_are_consistent_with_find_all() {
    let hay = b"ab ab ab ab";
    let all = find_all(hay, b"ab", SearchOptions::exact());
    assert_eq!(all, vec![0, 3, 6, 9]);
    assert_eq!(find_from(hay, b"ab", 1, SearchOptions::exact()), Some(3));
    assert_eq!(find_before(hay, b"ab", 9, SearchOptions::exact()), Some(6));
    assert_eq!(find_before(hay, b"ab", 0, SearchOptions::exact()), None);
}

#[test]
fn empty_needle_finds_nothing_instead_of_looping_forever() {
    assert_eq!(
        find_all(b"abc", b"", SearchOptions::exact()),
        Vec::<usize>::new()
    );
    assert_eq!(find_from(b"abc", b"", 0, SearchOptions::exact()), None);
}
