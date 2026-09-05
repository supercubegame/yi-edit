use yi_edit_core::{Doc, Eol, Pos, SearchOptions, MAX_UNDO};

/// 确定性伪随机（纯核心不得用未播种的随机，测试也不得）。
/// 同一个种子必然跑出同一串操作，失败才能重现。
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() as usize) % n
        }
    }
}

#[test]
fn insert_within_and_across_lines() {
    let mut d = Doc::from_text("hello\nworld");
    let end = d.insert(Pos::new(0, 5), ", yi");
    assert_eq!(end, Pos::new(0, 9));
    assert_eq!(d.to_text(), "hello, yi\nworld");

    let end2 = d.insert(Pos::new(1, 0), "a\nb\n");
    assert_eq!(d.to_text(), "hello, yi\na\nb\nworld");
    assert_eq!(end2, Pos::new(3, 0));
}

#[test]
fn delete_within_and_across_lines() {
    let mut d = Doc::from_text("one\ntwo\nthree");
    let removed = d.delete(Pos::new(0, 1), Pos::new(2, 2));
    assert_eq!(removed, "ne\ntwo\nth");
    assert_eq!(d.to_text(), "oree");
}

#[test]
fn clamp_snaps_to_char_boundary() {
    let d = Doc::from_text("中文abc");
    // "中" 占 3 字节，所以 col=1/2 都在字符中间。
    assert_eq!(d.clamp(Pos::new(0, 1)), Pos::new(0, 0));
    assert_eq!(d.clamp(Pos::new(0, 2)), Pos::new(0, 0));
    assert_eq!(d.clamp(Pos::new(0, 3)), Pos::new(0, 3));
    assert_eq!(d.clamp(Pos::new(99, 99)), Pos::new(0, "中文abc".len()));
}

#[test]
fn crlf_survives_a_round_trip() {
    let d = Doc::from_text("a\r\nb\r\n");
    assert_eq!(d.eol(), Eol::Crlf);
    assert_eq!(d.line_count(), 3);
    assert_eq!(d.to_text(), "a\r\nb\r\n");
}

#[test]
fn replace_all_is_case_insensitive_and_undo_restores_the_original_case() {
    let mut d = Doc::from_text("Foo foo FOO");
    let n = d.replace_all(
        "foo",
        "bar",
        SearchOptions {
            case_sensitive: false,
            whole_word: false,
        },
    );
    assert_eq!(n, 3);
    assert_eq!(d.to_text(), "bar bar bar");
    while d.can_undo() {
        d.undo();
    }
    // 这一条抓过一个真 bug：撤销时记的是搜索词而不是真正被删的文本，
    // 于是大小写不敏感替换撤销之后会把 Foo/FOO 都变成 foo。
    assert_eq!(d.to_text(), "Foo foo FOO");
}

#[test]
fn multiline_needle_finds_nothing_in_the_in_memory_editor() {
    // 已知限制，写成断言而不是口头承诺：跨行替换走文件级的流式路径。
    let d = Doc::from_text("a\nb");
    assert!(d.find_all("a\nb", SearchOptions::exact()).is_empty());
}

/// 承重：一串随机编辑全部撤销之后，必须逐字节回到原文。
/// 两条夹具前提自证：操作数小于撤销栈上限（否则测的是丢弃策略）；
/// 以及这串操作真的改动了文本（否则它是一条永远为真的断言）。
#[test]
fn undo_all_returns_to_the_original_text() {
    let original = "fn main() {\n    let x = 1;\n}\n中文行\n";
    let ops = 400usize;
    assert!(
        ops < MAX_UNDO,
        "操作数必须小于 MAX_UNDO，否则测的是丢弃策略"
    );

    let mut d = Doc::from_text(original);
    let mut rng = Lcg(0x5eed_1234);
    let snippets = ["x", "\n", "  ", "中", "let ", "();"];
    for _ in 0..ops {
        let line = rng.below(d.line_count());
        let col = rng.below(d.line(line).len() + 1);
        if rng.next() % 3 == 0 {
            let l2 = rng.below(d.line_count());
            let c2 = rng.below(d.line(l2).len() + 1);
            d.delete(Pos::new(line, col), Pos::new(l2, c2));
        } else {
            let s = snippets[rng.below(snippets.len())];
            d.insert(Pos::new(line, col), s);
        }
    }
    assert_ne!(
        d.to_text(),
        original,
        "这串操作没改动任何东西，后面那条断言是空的"
    );
    assert!(d.undo_depth() <= MAX_UNDO);

    while d.can_undo() {
        d.undo();
    }
    assert_eq!(d.to_text(), original, "全部撤销之后没回到原文");
}

#[test]
fn redo_replays_what_undo_took_back() {
    let mut d = Doc::from_text("abc");
    d.insert(Pos::new(0, 3), "def");
    d.delete(Pos::new(0, 0), Pos::new(0, 1));
    let after = d.to_text();
    d.undo();
    d.undo();
    assert_eq!(d.to_text(), "abc");
    d.redo();
    d.redo();
    assert_eq!(d.to_text(), after);
}

#[test]
fn dirty_flag_tracks_saves() {
    let mut d = Doc::from_text("x");
    assert!(!d.is_dirty());
    d.insert(Pos::new(0, 1), "y");
    assert!(d.is_dirty());
    d.mark_saved();
    assert!(!d.is_dirty());
}
