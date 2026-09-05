//! 撤销分组的断言。
//!
//! 为什么这一组值得单独一个文件：「每个字符一步」与「按输入组一步」在
//! **最终文本上完全一样**，只在撤销步数上不同。也就是说分组坏掉不会报错，
//! 只会让用户为了撤销一个词敲二十下 Ctrl+Z。

use yi_edit_core::{advance, Doc, Pos, MAX_GROUP_CHARS, MAX_UNDO};

/// 模拟敲字：一个字符一次 insert，就像 UI 事件循环那样。
fn type_text(d: &mut Doc, at: Pos, text: &str) -> Pos {
    let mut p = at;
    for ch in text.chars() {
        p = d.insert(p, &ch.to_string());
    }
    p
}

#[test]
fn advance_agrees_with_the_document_for_single_and_multi_line_text() {
    // advance 是纯函数，而 insert 真的改文档：两者算出的末端必须一致。
    // 不一致的话，合并时的连续性判断就会静默失效（永远不合并，或者错误合并）。
    for text in ["a", "abc", "中文", "a\nb", "\n", "x\ny\nz", "中\n文字"] {
        let mut d = Doc::from_text("head\ntail\n");
        let at = Pos::new(0, 2);
        let real = d.insert(at, text);
        assert_eq!(
            advance(at, text),
            real,
            "advance 与文档对 {text:?} 的末端不一致"
        );
    }
}

/// 承重：敲一个词 = 一下撤销。
/// 夹具先自证那个词真的多于一个字符 —— 否则这条断言在测空气。
#[test]
fn typing_a_word_is_one_undo_step() {
    let word = "function";
    assert!(
        word.chars().count() > 1,
        "语料只有一个字符，分组与不分组看不出区别"
    );

    let mut d = Doc::from_text("");
    type_text(&mut d, Pos::new(0, 0), word);
    assert_eq!(d.to_text(), word);
    assert_eq!(
        d.undo_depth(),
        1,
        "敲 {} 个字符却分成了多组",
        word.chars().count()
    );

    d.undo();
    assert_eq!(d.to_text(), "", "一下撤销没把整个词撤掉");
    assert!(!d.can_undo());
}

/// 空白与换行分组：敲 "let x" 应该是三组（let / 空格 / x）。
/// 这就是词级撤销：一下 Ctrl+Z 退一个词，而不是退整行。
#[test]
fn whitespace_and_newline_break_the_group() {
    let mut d = Doc::from_text("");
    type_text(&mut d, Pos::new(0, 0), "let x");
    assert_eq!(d.undo_depth(), 3, "let / 空格 / x 应该是三组");
    d.undo();
    assert_eq!(d.to_text(), "let ", "一下撤销应该只退掉最后一个词");
    d.undo();
    assert_eq!(d.to_text(), "let");
    d.undo();
    assert_eq!(d.to_text(), "");

    let mut d2 = Doc::from_text("");
    type_text(&mut d2, Pos::new(0, 0), "ab\ncd");
    assert_eq!(d2.undo_depth(), 3, "ab / 换行 / cd 应该是三组");
}

/// 光标移开之后再敲，不能归进同一组。
/// 两条路径都验：显式封口，以及位置不连续（上层忘了封口时的兵库）。
#[test]
fn moving_the_cursor_or_committing_starts_a_new_group() {
    // 显式封口（上层在光标移动时调）。
    let mut d = Doc::from_text("");
    type_text(&mut d, Pos::new(0, 0), "ab");
    d.commit_undo_group();
    type_text(&mut d, Pos::new(0, 2), "cd");
    assert_eq!(d.undo_depth(), 2, "显式封口之后却合并了");
    d.undo();
    assert_eq!(d.to_text(), "ab");

    // 位置不连续：即使上层忘了封口，也不能把两处的输入搅在一起。
    let mut d2 = Doc::from_text("xxxx");
    type_text(&mut d2, Pos::new(0, 0), "ab");
    type_text(&mut d2, Pos::new(0, 5), "cd");
    assert_eq!(d2.undo_depth(), 2, "两处不相邻的输入被合成了一组");
}

/// 连续退格合成一组，且一下撤销全部恢复。
#[test]
fn a_run_of_backspaces_is_one_undo_step() {
    let mut d = Doc::from_text("hello");
    let mut p = Pos::new(0, 5);
    for _ in 0..5 {
        let prev = Pos::new(0, p.col - 1);
        d.delete(prev, p);
        p = prev;
    }
    assert_eq!(d.to_text(), "");
    assert_eq!(d.undo_depth(), 1, "五下退格却分成了多组");
    d.undo();
    assert_eq!(d.to_text(), "hello", "一下撤销没把五个字符全恢复");
}

/// 连续向后删除（Delete 键）也合成一组。
/// 这是与退格相反的拼接方向，写错的表现是撤销后字符逆序。
#[test]
fn a_run_of_forward_deletes_is_one_undo_step_and_keeps_order() {
    let mut d = Doc::from_text("abcde");
    let at = Pos::new(0, 0);
    for _ in 0..5 {
        d.delete(at, Pos::new(0, 1));
    }
    assert_eq!(d.to_text(), "");
    assert_eq!(d.undo_depth(), 1, "五下 Delete 却分成了多组");
    d.undo();
    assert_eq!(d.to_text(), "abcde", "撤销后字符逆序了（拼接方向写反了）");
}

/// 退格与插入不能互相合并：两个方向相反的操作搅在一组里，
/// 撤销时的中间状态就没法表达了。
#[test]
fn typing_and_deleting_never_merge() {
    let mut d = Doc::from_text("");
    type_text(&mut d, Pos::new(0, 0), "ab");
    d.delete(Pos::new(0, 1), Pos::new(0, 2));
    assert_eq!(d.to_text(), "a");
    assert_eq!(d.undo_depth(), 2, "输入与删除被合成了一组");
    d.undo();
    assert_eq!(d.to_text(), "ab");
    d.undo();
    assert_eq!(d.to_text(), "");
}

/// 粘贴（多字符插入）自成一组，且不会被后续敲字吸进去。
#[test]
fn a_paste_is_its_own_group() {
    let mut d = Doc::from_text("");
    d.insert(Pos::new(0, 0), "一大段粘贴的内容\n第二行");
    assert_eq!(d.undo_depth(), 1);
    let p = Pos::new(1, d.line(1).len());
    type_text(&mut d, p, "xy");
    assert_eq!(d.undo_depth(), 2, "粘贴与之后的敲字被合成了一组");
    d.undo();
    assert!(
        d.to_text().ends_with("第二行"),
        "撤销只应该退掉后敲的两个字"
    );
    d.undo();
    assert_eq!(d.to_text(), "", "一下撤销没把整个粘贴撤掉");
}

/// 保存是一个语义边界：保存前后的输入不应该被一下 Ctrl+Z 一起撤掉。
#[test]
fn saving_closes_the_group() {
    let mut d = Doc::from_text("");
    type_text(&mut d, Pos::new(0, 0), "ab");
    d.mark_saved();
    assert!(!d.is_dirty());
    type_text(&mut d, Pos::new(0, 2), "cd");
    assert_eq!(
        d.undo_depth(),
        2,
        "保存没有封口，保存前后的输入被合成了一组"
    );
    d.undo();
    assert_eq!(d.to_text(), "ab", "撤销应该回到保存时的状态");
}

/// **全部替换是一个组**：一下 Ctrl+Z 全部撤掉。
/// 每处一组的话，替了两千处就要敲四千下。
#[test]
fn replace_all_is_a_single_undo_step() {
    let mut lines = String::new();
    for i in 0..50 {
        lines.push_str(&format!("line{i} needle here\n"));
    }
    let mut d = Doc::from_text(&lines);
    let n = d.replace_all("needle", "NEEDLE", yi_edit_core::SearchOptions::exact());
    assert_eq!(n, 50, "夹具里应该有 50 处匹配");
    assert_eq!(d.undo_depth(), 1, "全部替换分成了 {} 组", d.undo_depth());
    d.undo();
    assert_eq!(d.to_text(), lines, "一下撤销没把整个替换撤掉");
    d.redo();
    assert!(!d.to_text().contains("needle"), "重做没把替换重新施加");
    assert!(d.to_text().contains("NEEDLE"));
}

/// 上限两侧都断，并且真的敲超过上限时必须真的分成多组。
#[test]
fn the_group_cap_is_sane_and_really_caps() {
    assert!(
        MAX_GROUP_CHARS >= 16,
        "上限 {MAX_GROUP_CHARS} 太小，连一个长词都装不下"
    );
    assert!(
        MAX_GROUP_CHARS <= 1000,
        "上限 {MAX_GROUP_CHARS} 太大，形同无上限，一下 Ctrl+Z 会撤掉一整段"
    );

    let long: String = "a".repeat(MAX_GROUP_CHARS * 2 + 5);
    let mut d = Doc::from_text("");
    type_text(&mut d, Pos::new(0, 0), &long);
    assert_eq!(d.to_text(), long);
    assert!(
        d.undo_depth() >= 3,
        "敲了 {} 个字符（上限 {MAX_GROUP_CHARS}）却只有 {} 组，上限没生效",
        long.chars().count(),
        d.undo_depth()
    );
    // 封顶不能改变语义：全部撤销仍然要回到空。
    while d.can_undo() {
        d.undo();
    }
    assert_eq!(d.to_text(), "", "封顶之后全部撤销没回到原文");
}

/// 承重：分组只能改变**步数**，不能改变**语义**。
/// 一串确定性的随机编辑全部撤销后必须逐字节回到原文，而且重做回去也要一致。
#[test]
fn grouping_changes_the_step_count_not_the_semantics() {
    // 确定性伪随机：纯核心不得用未播种的随机，测试也不得。
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

    let original = "fn main() {\n    let x = 1;\n}\n中文行\n";
    let mut d = Doc::from_text(original);
    let mut rng = Lcg(0xC0FFEE);
    let snippets = ["x", "\n", " ", "中", "let ", "();", "ab"];
    for _ in 0..400 {
        let line = rng.below(d.line_count());
        let col = rng.below(d.line(line).len() + 1);
        match rng.next() % 4 {
            0 => {
                let l2 = rng.below(d.line_count());
                let c2 = rng.below(d.line(l2).len() + 1);
                d.delete(Pos::new(line, col), Pos::new(l2, c2));
            }
            1 => {
                d.commit_undo_group();
            }
            _ => {
                let s = snippets[rng.below(snippets.len())];
                d.insert(Pos::new(line, col), s);
            }
        }
    }
    let after = d.to_text();
    assert_ne!(after, original, "这串操作没改动任何东西，后面的断言是空的");
    assert!(d.undo_depth() <= MAX_UNDO);
    let groups = d.undo_depth();

    while d.can_undo() {
        d.undo();
    }
    assert_eq!(d.to_text(), original, "全部撤销后没回到原文");

    // 重做回去：组数不变，文本回到撤销前。
    let mut redone = 0usize;
    while d.can_redo() {
        d.redo();
        redone += 1;
    }
    assert_eq!(redone, groups, "重做步数与撤销步数不对称");
    assert_eq!(d.to_text(), after, "全部重做后没回到撤销前的文本");
}

/// 新的编辑必须清空重做栈：否则重做会把一个已经不存在的历史推回来。
#[test]
fn a_new_edit_clears_the_redo_stack() {
    let mut d = Doc::from_text("");
    type_text(&mut d, Pos::new(0, 0), "ab");
    d.undo();
    assert!(d.can_redo());
    type_text(&mut d, Pos::new(0, 0), "z");
    assert!(!d.can_redo(), "新编辑后重做栈没清空");

    // 合并路径上也要清：第二个字符是合并进去的，那条路径很容易忘了清重做栈。
    d.undo();
    assert!(d.can_redo());
    let p = Pos::new(0, 0);
    d.insert(p, "m");
    let p2 = d.insert(Pos::new(0, 1), "n");
    assert_eq!(p2.col, 2);
    assert!(!d.can_redo(), "合并进去的编辑没清重做栈");
}

/// 撤销之后接着敲字，不能归进一个已经被撤销的组里。
#[test]
fn typing_after_an_undo_starts_a_fresh_group() {
    let mut d = Doc::from_text("");
    type_text(&mut d, Pos::new(0, 0), "abc");
    d.undo();
    assert_eq!(d.to_text(), "");
    type_text(&mut d, Pos::new(0, 0), "xy");
    assert_eq!(d.to_text(), "xy");
    assert_eq!(d.undo_depth(), 1);
    d.undo();
    assert_eq!(d.to_text(), "", "撤销后的输入被搅进了旧组");
}
