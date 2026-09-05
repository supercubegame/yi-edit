use yi_edit_core::{find_all, SearchOptions, StreamSearcher};

fn stream(input: &[u8], needle: &[u8], opts: SearchOptions, chunk: usize) -> Vec<usize> {
    let mut s = StreamSearcher::new(needle, opts).expect("模式非空");
    let mut out = Vec::new();
    for part in input.chunks(chunk.max(1)) {
        out.extend(s.feed(part));
    }
    out.extend(s.finish());
    out
}

const CASES: &[(&str, &str)] = &[
    ("foo bar foo baz foo", "foo"),
    ("aaaa", "aa"),
    ("abcabcabc", "cab"),
    ("foo foobar foo_x foo", "foo"),
    ("中文测试中文中文", "中文"),
    ("no hits here", "zzz"),
    ("edge", "edge"),
    ("a\nb\na\nb", "b\na"),
    ("Foo FOO foo", "foo"),
];

fn opts_matrix() -> Vec<SearchOptions> {
    vec![
        SearchOptions::exact(),
        SearchOptions {
            case_sensitive: false,
            whole_word: false,
        },
        SearchOptions {
            case_sensitive: true,
            whole_word: true,
        },
    ]
}

/// 承重断言：任意块大小下，流式搜索的匹配偏移序列必须与整缓冲区搜索完全相同。
/// 这是「大文件搜索不会在块边界上静默漏匹配」的唯一依据。
#[test]
fn stream_search_equals_whole_buffer_for_every_chunk_size() {
    let mut total = 0usize;
    for (hay, needle) in CASES {
        for opts in opts_matrix() {
            let want = find_all(hay.as_bytes(), needle.as_bytes(), opts);
            for chunk in 1..=hay.len().max(1) {
                let got = stream(hay.as_bytes(), needle.as_bytes(), opts, chunk);
                assert_eq!(
                    got, want,
                    "块大小={chunk} hay={hay:?} needle={needle:?} opts={opts:?}"
                );
            }
            total += want.len();
        }
    }
    assert!(total > 20, "语料总匹配数只有 {total}，这批断言在测空气");
}

#[test]
fn hit_count_matches_the_returned_offsets() {
    let mut s = StreamSearcher::new(b"ab", SearchOptions::exact()).unwrap();
    let mut n = 0usize;
    for part in b"ab ab ab".chunks(3) {
        n += s.feed(part).len();
    }
    n += s.finish().len();
    assert_eq!(n, 3);
    assert_eq!(s.hits(), 3);
}

#[test]
fn empty_needle_is_refused_instead_of_looping() {
    assert!(StreamSearcher::new(b"", SearchOptions::exact()).is_none());
}
