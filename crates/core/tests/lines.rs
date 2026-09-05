use yi_edit_core::LineIndex;

/// 语料：含 LF、CRLF、空行、多字节字符、末尾无换行。
const DATA: &[u8] = b"line0\nline1\r\n\n\xe4\xb8\xad\xe6\x96\x87\nlast";

#[test]
fn empty_input_has_one_empty_line() {
    let idx = LineIndex::build(b"");
    assert_eq!(idx.line_count(), 1);
    assert_eq!(idx.line_span(0), Some(0..0));
    assert_eq!(idx.line_span(1), None);
}

#[test]
fn trailing_newline_creates_a_last_empty_line() {
    let idx = LineIndex::build(b"a\n");
    assert_eq!(idx.line_count(), 2);
    assert_eq!(idx.line_span(0), Some(0..2));
    assert_eq!(idx.line_span(1), Some(2..2));
}

#[test]
fn trim_eol_strips_lf_and_crlf() {
    let bytes: &[u8] = b"a\r\nb\n";
    let idx = LineIndex::build(bytes);
    let r0 = LineIndex::trim_eol(bytes, idx.line_span(0).unwrap());
    let r1 = LineIndex::trim_eol(bytes, idx.line_span(1).unwrap());
    assert_eq!(&bytes[r0], b"a");
    assert_eq!(&bytes[r1], b"b");
}

/// 承重：大文件模式的索引是分块喂的，分块结果必须逐个字节等于整块结果。
/// 对**所有**块大小跑一遍：只试一种块大小的话，恰好落在换行符上的那种切法永远不会被试到。
#[test]
fn chunked_build_equals_whole_build_for_every_chunk_size() {
    let whole = LineIndex::build(DATA);
    // 夹具自证：语料里真的有多行，否则下面的循环测的是空气。
    assert!(whole.line_count() >= 5, "语料行数不够：{}", whole.line_count());
    for chunk in 1..=DATA.len() {
        let mut starts = vec![0usize];
        let mut base = 0usize;
        for part in DATA.chunks(chunk) {
            LineIndex::extend(&mut starts, part, base);
            base += part.len();
        }
        let got = LineIndex::from_starts(starts, DATA.len());
        assert_eq!(got, whole, "块大小={chunk} 时分块索引与整块索引不一致");
    }
}

#[test]
fn line_of_offset_agrees_with_line_span() {
    let idx = LineIndex::build(DATA);
    for off in 0..DATA.len() {
        let line = idx.line_of_offset(off);
        let span = idx.line_span(line).expect("行号越界");
        assert!(
            span.contains(&off),
            "偏移 {off} 被归到第 {line} 行，但那一行的区间是 {span:?}"
        );
    }
}
