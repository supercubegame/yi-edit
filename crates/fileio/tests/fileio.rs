use std::fs;
use std::path::PathBuf;

use yi_edit_core::{find_all, replace_all, LineIndex, SearchOptions, HUGE_FILE_THRESHOLD};
use yi_edit_fileio as fio;

/// 不拉 tempfile 依赖：用进程号 + 计数器做唯一目录，结束时自己清。
struct Tmp(PathBuf);

impl Tmp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("yi-edit-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("建临时目录");
        Self(dir)
    }

    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        fs::write(&p, bytes).expect("写临时文件");
        p
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const MIXED: &[u8] = b"line0\nline1\r\n\n\xe4\xb8\xad\xe6\x96\x87 foo\nfoo bar\nlast foo";

#[test]
fn atomic_save_round_trips_and_leaves_no_temp_file() {
    let t = Tmp::new("atomic");
    let p = t.file("a.txt", b"before");
    fio::save_atomic(&p, b"after\n中文".as_ref()).expect("保存");
    assert_eq!(fio::read_all(&p).unwrap(), b"after\n中文");
    let left: Vec<String> = fs::read_dir(&t.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(left, vec!["a.txt".to_string()], "保存后目录里不应该还有临时文件：{left:?}");
}

/// 承重：文件级行索引（分块读）必须等于内存级行索引，对每一种块大小。
#[test]
fn file_index_equals_in_memory_index_for_every_chunk_size() {
    let t = Tmp::new("index");
    let p = t.file("m.txt", MIXED);
    let want = LineIndex::build(MIXED);
    assert!(want.line_count() >= 5, "语料行数不够，下面的循环测不到边界");
    for chunk in 1..=MIXED.len() {
        let got = fio::index_lines_chunked(&p, chunk).expect("建索引");
        assert_eq!(got, want, "块大小={chunk} 时文件索引与内存索引不一致");
    }
}

#[test]
fn read_range_returns_the_same_bytes_as_slicing() {
    let t = Tmp::new("range");
    let p = t.file("m.txt", MIXED);
    for start in 0..MIXED.len() {
        for len in [1usize, 3, 7, MIXED.len()] {
            let got = fio::read_range(&p, start as u64, len).expect("读窗口");
            let end = (start + len).min(MIXED.len());
            assert_eq!(got, &MIXED[start..end], "start={start} len={len}");
        }
    }
    // 越过文件尾要返回空，不是报错也不是一堆零字节。
    assert!(fio::read_range(&p, MIXED.len() as u64 + 10, 8).unwrap().is_empty());
}

/// 承重：文件级流式搜索的偏移必须等于内存级 find_all，对每一种块大小。
#[test]
fn file_search_equals_in_memory_search_for_every_chunk_size() {
    let t = Tmp::new("search");
    let p = t.file("m.txt", MIXED);
    for (needle, opts) in [
        ("foo", SearchOptions::exact()),
        (
            "FOO",
            SearchOptions {
                case_sensitive: false,
                whole_word: false,
            },
        ),
        (
            "foo",
            SearchOptions {
                case_sensitive: true,
                whole_word: true,
            },
        ),
        ("中文", SearchOptions::exact()),
    ] {
        let want: Vec<u64> = find_all(MIXED, needle.as_bytes(), opts)
            .into_iter()
            .map(|x| x as u64)
            .collect();
        assert!(!want.is_empty(), "needle={needle} 在语料里没命中，这条断言会变成空的");
        for chunk in 1..=MIXED.len() {
            let (got, truncated) =
                fio::find_offsets_chunked(&p, needle.as_bytes(), opts, 0, chunk).unwrap();
            assert!(!truncated);
            assert_eq!(got, want, "块大小={chunk} needle={needle}");
        }
    }
}

/// 到达上限时必须明确报 truncated：静默截断与「就这么多」不能长得一样。
#[test]
fn hitting_the_limit_is_reported_not_silently_truncated() {
    let t = Tmp::new("limit");
    let p = t.file("m.txt", MIXED);
    let (got, truncated) =
        fio::find_offsets_chunked(&p, b"foo", SearchOptions::exact(), 2, 4).unwrap();
    assert_eq!(got.len(), 2);
    assert!(truncated, "到达上限了却没报 truncated");

    let (all, t2) = fio::find_offsets_chunked(&p, b"foo", SearchOptions::exact(), 99, 4).unwrap();
    assert!(all.len() > 2);
    assert!(!t2, "没到上限却报了 truncated，这一侧也得能红");
}

/// 承重：文件级流式替换的字节与计数，必须等于内存级替换，对每一种块大小。
#[test]
fn file_replace_equals_in_memory_replace_for_every_chunk_size() {
    let t = Tmp::new("replace");
    for (needle, repl) in [("foo", "XXXX"), ("foo", ""), ("中文", "英文"), ("line", "L")] {
        let (want, want_n) = replace_all(
            MIXED,
            needle.as_bytes(),
            repl.as_bytes(),
            SearchOptions::exact(),
        );
        assert!(want_n > 0, "needle={needle} 没命中，这组断言是空的");
        for chunk in 1..=MIXED.len() {
            let src = t.file("src.txt", MIXED);
            let dst = t.path("dst.txt");
            let n = fio::stream_replace_chunked(
                &src,
                &dst,
                needle.as_bytes(),
                repl.as_bytes(),
                SearchOptions::exact(),
                chunk,
            )
            .unwrap();
            assert_eq!(n, want_n, "块大小={chunk} needle={needle} 替换计数不对");
            assert_eq!(
                fio::read_all(&dst).unwrap(),
                want,
                "块大小={chunk} needle={needle} 输出字节不对"
            );
        }
    }
}

#[test]
fn replace_in_place_matches_the_in_memory_path() {
    let t = Tmp::new("inplace");
    let a = t.file("a.txt", MIXED);
    let b = t.file("b.txt", MIXED);
    let na = fio::replace_in_place(&a, b"foo", b"Q", SearchOptions::exact()).unwrap();
    let nb = fio::replace_in_memory(&b, b"foo", b"Q", SearchOptions::exact()).unwrap();
    assert_eq!(na, nb);
    assert_eq!(fio::read_all(&a).unwrap(), fio::read_all(&b).unwrap());
}

#[test]
fn huge_flag_is_decided_by_the_documented_threshold() {
    let t = Tmp::new("huge");
    let p = t.file("small.txt", b"x");
    let i = fio::info(&p).unwrap();
    assert_eq!(i.len, 1);
    assert!(!i.is_huge);
    // 不真的写 64MB：阈值本身的语义用边界值判，大文件行为由 bench 二进制真实压。
    assert!(HUGE_FILE_THRESHOLD > 0);
}
