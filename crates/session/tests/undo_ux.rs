//! 会话层的撤销体验。
//!
//! core 那层验的是分组规则本身；这里验的是**上层真的按那些规则用它** ——
//! 两件事不一样：分组写得再对，上层忘了封口也一样难用。

use std::fs;
use std::path::PathBuf;

use yi_edit_core::Pos;
use yi_edit_session::Editor;

struct Tmp(PathBuf);

impl Tmp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("yi-undoux-{}-{tag}", std::process::id()));
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

const TEXT: &str = "one two three\n中文一行\nlast\n";

fn depth(e: &Editor) -> usize {
    e.doc().map(|d| d.undo_depth()).unwrap_or(0)
}

/// 通过会话层敲字（每次一个字符，就像 UI 事件循环）。
fn type_through_session(e: &mut Editor, text: &str) {
    for ch in text.chars() {
        assert!(e.insert_text(&ch.to_string()), "插入 {ch:?} 失败了");
    }
}

/// 承重：**有选区时粘贴 = 一个撤销组**。
///
/// 两个算子各自一组的话，敲一下 Ctrl+Z 会停在一个用户从没见过的中间状态
//  （选区已删、新内容未插）。那不会报错，只会让人以为编辑器坏了。
#[test]
fn pasting_over_a_selection_is_one_undo_step() {
    let t = Tmp::new("replace");
    let p = t.file("a.txt", TEXT);
    let mut e = Editor::open(&p).expect("打开");
    e.anchor = Some(Pos::new(0, 4));
    e.cursor = Pos::new(0, 7);
    let selected = e.selected_text().expect("有选区");
    assert_eq!(selected, "two", "夹具选错了，后面的断言测不到替换");

    let before = depth(&e);
    assert!(e.insert_text("TWO二"), "粘贴失败");
    assert_eq!(
        depth(&e) - before,
        1,
        "替换选区分成了 {} 组",
        depth(&e) - before
    );
    assert!(e.doc().unwrap().to_text().contains("TWO二"));

    e.doc_mut().unwrap().undo();
    assert_eq!(
        e.doc().unwrap().to_text(),
        TEXT,
        "一下撤销没回到选区原样（停在了中间状态）"
    );
}

/// 通过会话层敲一个词也是一步。
/// core 那层已经验过分组规则，这里验的是会话层没把它拆成一堆小操作。
#[test]
fn typing_a_word_through_the_session_is_one_undo_step() {
    let t = Tmp::new("typing");
    let p = t.file("a.txt", "");
    let mut e = Editor::open(&p).expect("打开");
    type_through_session(&mut e, "hello");
    assert_eq!(e.doc().unwrap().to_text(), "hello");
    assert_eq!(depth(&e), 1, "敲一个词分成了 {} 组", depth(&e));
    e.doc_mut().unwrap().undo();
    assert_eq!(e.doc().unwrap().to_text(), "");
}

/// 封口接口得真的起作用（UI 在光标移动 / 失焦 / 点击时调它）。
/// 两侧都断：不调时必须合并，调了必须不合并 —— 只验一侧的话，
/// 一个什么都不做的空函数也能交差。
#[test]
fn commit_undo_group_really_closes_the_group() {
    let t = Tmp::new("commit");
    let p = t.file("a.txt", "");

    let mut merged = Editor::open(&p).expect("打开");
    type_through_session(&mut merged, "ab");
    type_through_session(&mut merged, "cd");
    assert_eq!(depth(&merged), 1, "不封口时应该合并成一组");

    let mut split = Editor::open(&p).expect("打开");
    type_through_session(&mut split, "ab");
    split.commit_undo_group();
    type_through_session(&mut split, "cd");
    assert_eq!(depth(&split), 2, "封口之后却还是合并了，commit_undo_group 是空函数");

    assert_eq!(
        merged.doc().unwrap().to_text(),
        split.doc().unwrap().to_text(),
        "封口只能改变撤销步数，不能改变文本"
    );
}

/// 只读模式下封口不能 panic（没有可编辑文档也要能安全调）。
/// UI 在每次光标移动时都会调它，而光标在只读文件里也会动。
#[test]
fn committing_in_read_only_mode_is_safe() {
    let t = Tmp::new("readonly");
    let p = t.file("big.txt", TEXT);
    let mut e = Editor::open_with_threshold(&p, 0).expect("打开");
    assert!(e.is_huge());
    e.commit_undo_group();
    e.commit_undo_group();
    assert!(!e.is_dirty(), "只读模式下封口却把文档标脏了");
}

/// 退格跑与接下来的输入不能搅在一组：否则撤销时拿不到中间状态。
#[test]
fn a_backspace_run_then_typing_are_separate_steps() {
    let t = Tmp::new("mix");
    let p = t.file("a.txt", "abcdef");
    let mut e = Editor::open(&p).expect("打开");
    e.cursor = Pos::new(0, 6);
    for _ in 0..3 {
        let cur = e.cursor;
        let prev = e.prev_pos(cur);
        e.doc_mut().unwrap().delete(prev, cur);
        e.cursor = prev;
    }
    assert_eq!(e.doc().unwrap().to_text(), "abc");
    assert_eq!(depth(&e), 1, "三下退格分成了多组");

    type_through_session(&mut e, "XY");
    assert_eq!(depth(&e), 2, "退格与输入被合成了一组");
    e.doc_mut().unwrap().undo();
    assert_eq!(e.doc().unwrap().to_text(), "abc", "拿不到退格之后的中间状态");
    e.doc_mut().unwrap().undo();
    assert_eq!(e.doc().unwrap().to_text(), "abcdef");
}
