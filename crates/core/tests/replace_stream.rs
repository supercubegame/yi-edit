use yi_edit_core::{
    find_all, replace_all, ReplaceError, SearchOptions, StreamReplacer, MAX_PATTERN_LEN,
};

fn stream(
    input: &[u8],
    needle: &[u8],
    repl: &[u8],
    opts: SearchOptions,
    chunk: usize,
) -> (Vec<u8>, usize) {
    let mut r = StreamReplacer::new(needle, repl, opts).expect("模式合法");
    let mut out = Vec::new();
    for part in input.chunks(chunk.max(1)) {
        out.extend_from_slice(&r.feed(part));
    }
    out.extend_from_slice(&r.finish());
    (out, r.count())
}

struct Case {
    hay: &'static str,
    needle: &'static str,
    repl: &'static str,
    opts: SearchOptions,
}

fn cases() -> Vec<Case> {
    let exact = SearchOptions::exact();
    let ci = SearchOptions {
        case_sensitive: false,
        whole_word: false,
    };
    let ww = SearchOptions {
        case_sensitive: true,
        whole_word: true,
    };
    vec![
        Case {
            hay: "foo bar foo baz foo",
            needle: "foo",
            repl: "XX",
            opts: exact,
        },
        Case {
            hay: "aaaa",
            needle: "aa",
            repl: "b",
            opts: exact,
        },
        Case {
            hay: "abcabcabc",
            needle: "cab",
            repl: "",
            opts: exact,
        },
        Case {
            hay: "Foo FOO foo",
            needle: "foo",
            repl: "bar",
            opts: ci,
        },
        Case {
            hay: "foo foobar foo_x foo",
            needle: "foo",
            repl: "Q",
            opts: ww,
        },
        Case {
            hay: "中文测试中文",
            needle: "中文",
            repl: "英文",
            opts: exact,
        },
        Case {
            hay: "no hits here",
            needle: "zzz",
            repl: "!",
            opts: exact,
        },
        Case {
            hay: "edge",
            needle: "edge",
            repl: "EDGE!",
            opts: exact,
        },
        Case {
            hay: "a\nb\na\nb",
            needle: "b\na",
            repl: "-",
            opts: exact,
        },
    ]
}

/// 夹具自证：语料里真的存在「一个匹配横跨两个块」的情形。
/// 不先证这一条的话，下面那条「每种块大小都相等」可能全程根本没碰到边界情形。
#[test]
fn fixture_really_contains_boundary_straddling_matches() {
    let mut straddled = 0usize;
    for c in cases() {
        let hits = find_all(c.hay.as_bytes(), c.needle.as_bytes(), c.opts);
        for p in hits {
            for k in 1..c.hay.len().max(2) {
                if p / k != (p + c.needle.len() - 1) / k {
                    straddled += 1;
                    break;
                }
            }
        }
    }
    assert!(
        straddled >= 5,
        "只有 {straddled} 个跨块匹配，语料没真正压到边界"
    );
}

/// 承重断言：流式替换对**任意块大小**的输出，必须逐字节等于整缓冲区替换的输出，
/// 替换计数也必须相等。少替一个不会报错，只会得到一个看起来很正常的错文件。
#[test]
fn stream_equals_whole_buffer_for_every_chunk_size() {
    for c in cases() {
        let (want, want_n) = replace_all(
            c.hay.as_bytes(),
            c.needle.as_bytes(),
            c.repl.as_bytes(),
            c.opts,
        );
        for chunk in 1..=c.hay.len().max(1) {
            let (got, got_n) = stream(
                c.hay.as_bytes(),
                c.needle.as_bytes(),
                c.repl.as_bytes(),
                c.opts,
                chunk,
            );
            assert_eq!(
                String::from_utf8_lossy(&got),
                String::from_utf8_lossy(&want),
                "块大小={chunk} hay={:?} needle={:?} opts={:?}",
                c.hay,
                c.needle,
                c.opts
            );
            assert_eq!(
                got_n, want_n,
                "块大小={chunk} 的替换计数不一致 hay={:?} needle={:?}",
                c.hay, c.needle
            );
        }
    }
}

#[test]
fn feed_then_finish_handles_data_shorter_than_the_pattern() {
    let (got, n) = stream(b"ab", b"abcdef", b"x", SearchOptions::exact(), 1);
    assert_eq!(got, b"ab");
    assert_eq!(n, 0);
}

/// 过长 / 空模式必须报错，而不是静默截断或者无限循环。
#[test]
fn illegal_patterns_are_refused_loudly() {
    assert_eq!(
        StreamReplacer::new(b"", b"x", SearchOptions::exact()).err(),
        Some(ReplaceError::EmptyPattern)
    );
    let long = vec![b'a'; MAX_PATTERN_LEN + 1];
    assert_eq!(
        StreamReplacer::new(&long, b"x", SearchOptions::exact()).err(),
        Some(ReplaceError::PatternTooLong {
            len: MAX_PATTERN_LEN + 1,
            max: MAX_PATTERN_LEN
        })
    );
}
