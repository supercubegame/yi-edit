//! 剪贴板与全选的断言。一个不能粘贴的编辑器不能交付，
//! 而这些逻辑卡在 UI 层里的话永远验不到。

use std::fs;
use std::path::PathBuf;

use yi_edit_core::Pos;
use yi_edit_session::Editor;

struct Tmp(PathBuf);

impl Tmp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("yi-clip-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("建临时目录");
        Self(dir)
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let p = self.0.join(name);
        fs::write(&p, text.as_bytes()).expect("写临时文件");
        p
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 含中文与 emoji：按字节切的话这份语料上会直接 panic。
const TEXT: &str = "abc中文def\n第二行\u{1f600}x\nthird line\n";

/// 承重：**交叉校验**。复制到的文本与剪掉的文本必须逐字节相同。
/// 两者是两个独立路径（一个拼行，一个走 Doc::delete），不一致也不会报错 ——
/// 用户看到的现象是「Ctrl+X 之后粘贴出来的不是刚才选的那段」。
#[test]
fn copy_and_cut_return_byte_identical_text() {
    let t = Tmp::new("cross");
    let cases = [
        (Pos::new(0, 0), Pos::new(0, 3)),
        (Pos::new(0, 3), Pos::new(0, 9)),
        (Pos::new(0, 0), Pos::new(1, 0)),
        (Pos::new(0, 4), Pos::new(2, 5)),
        (Pos::new(1, 0), Pos::new(3, 0)),
    ];
    let mut checked = 0usize;
    for (a, b) in cases {
        let p = t.file("a.txt", TEXT);
        let mut e1 = Editor::open(&p).expect("打开");
        e1.anchor = Some(a);
        e1.cursor = b;
        let copied = e1.selected_text().expect("有选区就应该有文本");

        let mut e2 = Editor::open(&p).expect("打开");
        e2.anchor = Some(a);
        e2.cursor = b;
        let cut = e2.cut_selection().expect("有选区就应该能剪");
        assert_eq!(copied, cut, "选区 {a:?}..{b:?} 复制与剪切不一致");
        assert!(!copied.is_empty());
        checked += 1;
    }
    assert!(checked >= 5, "只校了 {checked} 个选区，语料太稀");
}

/// 剪切之后剩下的文本 = 原文去掉那一段，且可撤销。
#[test]
fn cut_removes_exactly_the_selection_and_is_undoable() {
    let t = Tmp::new("cut");
    let p = t.file("a.txt", TEXT);
    let mut e = Editor::open(&p).expect("打开");
    e.anchor = Some(Pos::new(0, 3));
    e.cursor = Pos::new(0, 9);
    let cut = e.cut_selection().expect("能剪");
    let after = e.doc().unwrap().to_text();
    assert_eq!(after, TEXT.replacen(&cut, "", 1), "剪掉的不是选区那一段");
    assert!(e.doc().unwrap().is_dirty(), "剪切了却没标脏");

    e.doc_mut().unwrap().undo();
    assert_eq!(e.doc().unwrap().to_text(), TEXT, "剪切不可撤销");
}

/// 承重：粘贴走 EditOp，所以可撤销。
/// 不走的话用户粘错一大段之后敲 Ctrl+Z 会发现改不回去，而那不会报错。
#[test]
fn paste_goes_through_edit_ops_so_undo_works() {
    let t = Tmp::new("paste");
    let p = t.file("a.txt", TEXT);
    let mut e = Editor::open(&p).expect("打开");
    e.cursor = Pos::new(0, 3);
    assert!(e.insert_text("粘贴的内容\n第二行"), "粘贴应该成功");
    let after = e.doc().unwrap().to_text();
    assert_ne!(after, TEXT, "粘贴了却没改动内容，下面那条断言是空的");
    assert!(after.contains("粘贴的内容"), "粘贴的文本不在里面：{after}");

    while e.doc().unwrap().can_undo() {
        e.doc_mut().unwrap().undo();
    }
    assert_eq!(e.doc().unwrap().to_text(), TEXT, "全部撤销后没回到原文");
}

/// 有选区时粘贴 = 替换选区。全部撤销仍然要回到原文。
#[test]
fn paste_over_a_selection_replaces_it() {
    let t = Tmp::new("replace");
    let p = t.file("a.txt", TEXT);
    let mut e = Editor::open(&p).expect("打开");
    e.anchor = Some(Pos::new(0, 3));
    e.cursor = Pos::new(0, 9);
    let selected = e.selected_text().expect("有选区");
    assert!(e.insert_text("XY"), "粘贴应该成功");
    let after = e.doc().unwrap().to_text();
    assert!(!after.contains(&selected), "选区没被替掉：{after}");
    assert!(after.contains("XY"), "新内容没进去：{after}");
    assert!(e.selection().is_none(), "粘贴后应该没有选区了");

    while e.doc().unwrap().can_undo() {
        e.doc_mut().unwrap().undo();
    }
    assert_eq!(e.doc().unwrap().to_text(), TEXT, "全部撤销后没回到原文");
}

/// 只读模式：**复制得允许**（从大文件里拉一段是合法需求），
/// 但剪切与粘贴必须被拒，而且磁盘上一个字节都不能变。
/// 两侧都断：只验「拒绝写」的话，一个连复制都不给的编辑器也能完美交差。
#[test]
fn read_only_mode_allows_copy_but_refuses_cut_and_paste() {
    let t = Tmp::new("readonly");
    let p = t.file("big.txt", TEXT);
    // 阈值设成 0：不能为了验一条只读路径就真写 64MB。
    let mut e = Editor::open_with_threshold(&p, 0).expect("打开");
    assert!(e.is_huge(), "阈值设成 0 却没进只读模式");

    e.anchor = Some(Pos::new(0, 0));
    e.cursor = Pos::new(0, 3);
    assert_eq!(
        e.selected_text().as_deref(),
        Some("abc"),
        "只读模式下复制被拒了，而那是一个合法需求"
    );

    assert!(e.cut_selection().is_none(), "只读模式却能剪切");
    assert!(!e.insert_text("x"), "只读模式却能粘贴");
    assert_eq!(
        fs::read_to_string(&p).unwrap(),
        TEXT,
        "只读模式下磁盘内容被改了"
    );
}

/// 全选要真的选中全文：复制出来的字节必须等于文档全文。
/// 只断「锚点在 (0,0)」的话，一个光标停在第二行的实现也能通过。
#[test]
fn select_all_covers_the_whole_document() {
    let t = Tmp::new("selectall");
    let p = t.file("a.txt", TEXT);
    let mut e = Editor::open(&p).expect("打开");
    e.select_all();
    let all = e.selected_text().expect("全选后应该有选区");
    assert_eq!(all, e.doc().unwrap().to_text(), "全选没盖住全文");
    assert!(all.contains("third line"), "最后一行不在选区里：{all}");

    // 全选后粘贴 = 整文替换，且可撤销。
    assert!(e.insert_text("只剩这一行"), "粘贴应该成功");
    assert_eq!(e.doc().unwrap().to_text(), "只剩这一行");
    while e.doc().unwrap().can_undo() {
        e.doc_mut().unwrap().undo();
    }
    assert_eq!(e.doc().unwrap().to_text(), TEXT, "全选替换后撤销不回去");
}

/// 空选区 / 空粘贴 要安静失败，不能弄脏文档。
#[test]
fn empty_selection_and_empty_paste_are_no_ops() {
    let t = Tmp::new("empty");
    let p = t.file("a.txt", TEXT);
    let mut e = Editor::open(&p).expect("打开");
    assert!(e.selected_text().is_none(), "没选区却给出了文本");
    assert!(e.cut_selection().is_none(), "没选区却能剪切");
    assert!(!e.insert_text(""), "空粘贴应该是空操作");
    assert!(!e.doc().unwrap().is_dirty(), "一串空操作却把文档标脏了");
}

/// 脏标记：关窗拦截靠它，所以它得在会话层上可读，而不是在 UI 里自己猜。
#[test]
fn the_dirty_flag_is_readable_from_the_session() {
    let t = Tmp::new("dirty");
    let p = t.file("a.txt", TEXT);
    let mut e = Editor::open(&p).expect("打开");
    assert!(!e.is_dirty(), "刚打开就脏了");
    e.cursor = Pos::new(0, 0);
    assert!(e.insert_text("x"));
    assert!(e.is_dirty(), "改了却不脏，关窗时会静默丢改动");
    e.save().expect("保存");
    assert!(!e.is_dirty(), "保存后还脏，每次关窗都会弹对话框");

    // 只读模式永远不脏（没有未保存的修改）。
    let mut ro = Editor::open_with_threshold(&p, 0).expect("打开");
    assert!(!ro.is_dirty());
    assert!(!ro.insert_text("y"));
    assert!(!ro.is_dirty(), "只读模式却报脏");
}
