//! 会话层的断言。这些逻辑之前卡在 GUI crate 里，一条断言都没有。
//!
//! 每一条承重断言前面都先自证夹具真的压到了该压的情形 —— 否则
//! 「该拓的没拓到」与「该不拓的确实没拓」在输出上一模一样。

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use yi_edit_core::{Doc, LineState, Pos, SearchOptions};
use yi_edit_session::{Editor, MAX_HITS, WINDOW_LINES};

/// 不拉 tempfile 依赖：用进程号 + 标签做唯一目录，结束时自己清。
struct Tmp(PathBuf);

impl Tmp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("yi-session-{}-{tag}", std::process::id()));
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

/// 行数故意超过窗口：不超的话缓存从不重填，那条窗口读断言就在测空气。
const LINES: usize = WINDOW_LINES * 3 + 7;

fn corpus() -> String {
    let mut s = String::new();
    for i in 0..LINES {
        // 每隔几行掉一个中文行与一个 needle，让窗口边界上也能落到多字节字符。
        if i % 7 == 0 {
            let _ = writeln!(s, "line{i} 中文内容 needle 尾巴");
        } else {
            let _ = writeln!(s, "line{i} plain content {}", i * 3);
        }
    }
    s
}

/// 把阈值设成 0：不能为了验一条只读路径就真写 64MB。
fn open_huge(path: &std::path::Path) -> Editor {
    let e = Editor::open_with_threshold(path, 0).expect("打开");
    assert!(e.is_huge(), "阈值设成 0 却没进大文件模式，这个参数是装饰");
    e
}

/// 承重：大文件模式的按需窗口读，逐行必须等于整份进内存的结果。
/// 窗口缓存算错一行不会报错，只会让用户看到错位的文件内容。
#[test]
fn huge_mode_windowed_read_equals_the_in_memory_lines() {
    let t = Tmp::new("window");
    let text = corpus();
    let p = t.file("big.txt", &text);
    let want = Doc::from_text(&text);
    let mut e = open_huge(&p);

    assert_eq!(e.line_count(), want.line_count(), "行数就不一致");
    assert!(
        e.line_count() > WINDOW_LINES,
        "只有 {} 行，不超过窗口 {WINDOW_LINES}，缓存从不重填，这条断言在测空气",
        e.line_count()
    );

    // 正序跑一遍（模拟往下滚）
    for i in 0..e.line_count() {
        assert_eq!(e.line(i), want.line(i), "第 {i} 行（正序）不一致");
    }
    // 反序再跑一遍（模拟往上滚）：向后跳的缓存重填是另一条路径。
    for i in (0..e.line_count()).rev() {
        assert_eq!(e.line(i), want.line(i), "第 {i} 行（反序）不一致");
    }
    // 随机跳（确定性序列，模拟搜索结果跳转）
    let mut i = 0usize;
    for _ in 0..200 {
        i = (i * 7919 + 104_729) % e.line_count();
        assert_eq!(e.line(i), want.line(i), "第 {i} 行（跳转）不一致");
    }
}

/// 大文件模式必须真的拒绝写，而不是只在状态栏里写一句。
/// 假装可编辑的代价是用户敲了一下键才发现改不了，而那时候他已经信任它了。
#[test]
fn huge_mode_is_really_read_only() {
    let t = Tmp::new("readonly");
    let text = corpus();
    let p = t.file("big.txt", &text);
    let mut e = open_huge(&p);

    assert!(e.doc().is_none(), "大文件模式不应该交出可编辑文档");
    assert!(e.doc_mut().is_none(), "大文件模式不应该交出可变文档");

    let err = e.save().expect_err("大文件模式不应该能保存");
    assert_eq!(err.kind(), std::io::ErrorKind::Unsupported, "错误种类不对：{err}");
    // 磁盘上的字节一个都不能变。
    assert_eq!(fs::read(&p).unwrap(), text.as_bytes(), "只读模式却改动了文件");
}

/// 小文件走内存模式，且真的可编辑可保存。这是上一条的对照侧：
/// 只验「拒绝写」的话，一个任何情况下都拒绝写的编辑器也能完美交差。
#[test]
fn small_files_stay_editable_and_saveable() {
    let t = Tmp::new("editable");
    let p = t.file("small.txt", "hello\n中文\n");
    let mut e = Editor::open(&p).expect("打开");
    assert!(!e.is_huge(), "小文件被当成了大文件");

    let d = e.doc_mut().expect("内存模式应该交出可变文档");
    d.insert(Pos::new(0, 5), ", yi");
    assert!(d.is_dirty());
    e.save().expect("保存");
    assert_eq!(fs::read_to_string(&p).unwrap(), "hello, yi\n中文\n");
    assert!(!e.doc().unwrap().is_dirty(), "保存后还挂着未保存标记");
}

/// 承重：到达上限必须报 truncated，而且**两侧都能红**。
/// 「只找到这么多」与「只有这么多」不能长得一样，否则界面上的计数就是一条静默的谎。
#[test]
fn hitting_the_hit_limit_is_reported_and_the_other_side_can_fail_too() {
    let t = Tmp::new("limit");
    let mut over = String::new();
    for _ in 0..(MAX_HITS + 10) {
        over.push_str("needle\n");
    }
    let p = t.file("over.txt", &over);
    let mut e = Editor::open(&p).expect("打开");
    let (hits, truncated) = e.search("needle", SearchOptions::exact());
    assert_eq!(hits.len(), MAX_HITS, "没按上限截");
    assert!(truncated, "到达上限了却没报 truncated");

    let p2 = t.file("under.txt", "needle\nneedle\n");
    let mut e2 = Editor::open(&p2).expect("打开");
    let (h2, t2) = e2.search("needle", SearchOptions::exact());
    assert_eq!(h2.len(), 2);
    assert!(!t2, "没到上限却报了 truncated，这一侧也得能红");

    // 过长模式也要明确报，而不是静默返回空结果。
    let long = "x".repeat(yi_edit_core::MAX_PATTERN_LEN + 1);
    let (h3, t3) = e2.search(&long, SearchOptions::exact());
    assert!(h3.is_empty());
    assert!(t3, "模式过长时必须标成未完整，不能像「真的没匹配」");
    assert!(
        e2.status.contains("太长"),
        "状态栏没说清为什么没结果：{}",
        e2.status
    );

    let (h4, t4) = e2.search("", SearchOptions::exact());
    assert!(h4.is_empty() && !t4, "空搜索词不应该报截断");
}

/// 大文件模式的搜索：字节偏移到行/列的换算必须对，并且那个位置上真的是 needle。
/// 只比数量的话，一个把所有命中都报在第 0 行的实现也能完美交差。
#[test]
fn huge_mode_search_maps_offsets_to_real_line_and_column() {
    let t = Tmp::new("search");
    let text = corpus();
    let p = t.file("big.txt", &text);
    let mut e = open_huge(&p);

    let (hits, truncated) = e.search("needle", SearchOptions::exact());
    assert!(!truncated);
    let expected = LINES.div_ceil(7);
    assert_eq!(hits.len(), expected, "命中数不对");
    assert!(hits.len() > 100, "命中太少，下面的逐个校对压不到窗口边界");

    // 后面的命中定然跨过多个窗口，而且行号必须递增。
    let mut last_line = None;
    for h in &hits {
        if let Some(prev) = last_line {
            assert!(h.line > prev, "行号没递增：{prev} -> {}", h.line);
        }
        last_line = Some(h.line);
        let line = e.line(h.line);
        assert!(
            line[h.col..].starts_with("needle"),
            "第 {} 行第 {} 列上不是 needle：{line:?}",
            h.line,
            h.col
        );
    }
}

/// 大文件模式的替换走磁盘，完事必须重建索引。
/// 不重建的话行索引会指向旧偏移，而那不会报错，只会让界面错位。
#[test]
fn huge_mode_replace_rewrites_the_file_and_rebuilds_the_index() {
    let t = Tmp::new("replace");
    let text = corpus();
    let p = t.file("big.txt", &text);
    let mut e = open_huge(&p);

    let before = e.line_count();
    let expected = LINES.div_ceil(7);
    let n = e
        .replace_all("needle", "REPLACED_LONGER", SearchOptions::exact())
        .expect("替换");
    assert_eq!(n, expected, "替换计数不对");

    let on_disk = fs::read_to_string(&p).unwrap();
    assert!(!on_disk.contains("needle"), "磁盘上还剩着 needle");
    assert_eq!(on_disk.matches("REPLACED_LONGER").count(), expected);

    // 行数不应该变（只是行变长了），但索引必须是重建过的：
    // 拿重建后的索引去读行，必须与磁盘上的新内容逐行对得上。
    assert_eq!(e.line_count(), before, "行数不应该变");
    let want = Doc::from_text(&on_disk);
    for i in 0..e.line_count() {
        assert_eq!(e.line(i), want.line(i), "替换后第 {i} 行错位（索引没重建）");
    }
}

/// 承重：光标按**字符**而不是按字节走。
/// 按字节走的话，在中文行上敲一下方向键就能切出一个非字符边界，而那之后
/// 任何切片都会 panic。这两个函数之前卡在 UI 层里，永远验不到。
#[test]
fn cursor_walks_char_boundaries_not_bytes() {
    let t = Tmp::new("cursor");
    let line0 = "a中文b\u{1f600}c";
    let text = format!("{line0}\n第二行\nx\n");
    let p = t.file("cjk.txt", &text);
    let mut e = Editor::open(&p).expect("打开");

    // 夹具自证：这一行真的有多字节字符（字节数 > 字符数）。
    assert!(
        line0.len() > line0.chars().count(),
        "语料全是单字节，这条断言在测空气"
    );

    // 从行首往后走到行尾，每一步都必须落在字符边界上，且步数等于字符数。
    let mut p0 = Pos::new(0, 0);
    let mut steps = 0usize;
    while p0.col < line0.len() {
        p0 = e.next_pos(p0);
        steps += 1;
        assert!(
            line0.is_char_boundary(p0.col),
            "next_pos 停在了非字符边界 col={}",
            p0.col
        );
        assert!(steps <= line0.len(), "next_pos 没在推进，死循环了");
    }
    assert_eq!(steps, line0.chars().count(), "步数不等于字符数，那就是按字节走的");

    // 再走回来，必须逐步回到行首。
    let mut back = 0usize;
    while p0.col > 0 {
        p0 = e.prev_pos(p0);
        back += 1;
        assert!(line0.is_char_boundary(p0.col), "prev_pos 停在了非字符边界");
        assert!(back <= line0.len(), "prev_pos 没在推进，死循环了");
    }
    assert_eq!(back, steps, "正反步数不对称");

    // 跨行：行首向前要到上一行行尾；文头向前不动。
    let up = e.prev_pos(Pos::new(1, 0));
    assert_eq!(up, Pos::new(0, line0.len()));
    assert_eq!(e.prev_pos(Pos::new(0, 0)), Pos::new(0, 0), "文头向前应该不动");

    // 文尾向后不动。文本以 \n 结尾，所以最后一行是空行。
    let last = e.line_count() - 1;
    assert_eq!(e.next_pos(Pos::new(last, 0)), Pos::new(last, 0), "文尾向后应该不动");
}

/// 承重：跨行高亮状态。只高亮可见行意味着渲染第 N 行时必须知道第 N-1 行的出口状态。
/// 不管的话，从中间开始看的文件会把块注释内的代码染成普通色 —— 而那不会报错。
#[test]
fn cross_line_highlight_state_is_carried_and_invalidated() {
    let t = Tmp::new("states");
    let mut src = String::from("fn a() {}\n/* 开头\n");
    for i in 0..50 {
        let _ = writeln!(src, "还在注释里 {i} let x = 1;");
    }
    src.push_str("*/ fn b() {}\nfn c() {}\n");
    let p = t.file("a.rs", &src);
    let mut e = Editor::open(&p).expect("打开");

    // 从文件中间直接问（模拟滚到中间），必须知道自己在块注释里。
    let inside = 30usize;
    assert!(
        e.state_at(inside).block_comment,
        "第 {inside} 行在未闭合的块注释里，却报不在"
    );
    // 夹具自证：缓存真的长到了那里，否则下面的失效断言测的是空气。
    assert!(
        e.cached_state_count() > inside,
        "只缓存了 {} 条状态",
        e.cached_state_count()
    );

    // 注释结束之后的行不应该还在注释里。
    let after = e.line_count() - 2;
    assert!(!e.state_at(after).block_comment, "块注释结束了却没退出");
    assert_eq!(e.state_at(0), LineState::default(), "第 0 行入口状态应该是默认");

    // 失效之后缓存真的要缩，否则编辑之后高亮就是陈的。
    e.invalidate_states(5);
    assert!(
        e.cached_state_count() <= 5,
        "失效后还缓存着 {} 条",
        e.cached_state_count()
    );
    // 重新问一遍，结果必须与失效前一致（缓存只是加速，不改变语义）。
    assert!(e.state_at(inside).block_comment, "失效重建后结果变了");
}

/// 大文件模式不回溯跨行状态。这是一条**已知限制**，写成断言而不是口头承诺：
/// 它改了的那天，这条会红，提醒我同步改 docs/PITFALLS.md。
#[test]
fn huge_mode_does_not_backtrack_highlight_state_and_says_so() {
    let t = Tmp::new("hugestate");
    let mut src = String::from("/* 开头\n");
    for i in 0..LINES {
        let _ = writeln!(src, "还在注释里 {i}");
    }
    let p = t.file("big.rs", &src);
    let mut e = open_huge(&p);
    assert_eq!(
        e.state_at(100),
        LineState::default(),
        "大文件模式现在会回溯跨行状态了？那 docs/PITFALLS.md 里那条限制得删掉"
    );
}

#[test]
fn selection_is_ordered_and_empty_selection_is_none() {
    let t = Tmp::new("sel");
    let p = t.file("a.txt", "abcdef\nghij\n");
    let mut e = Editor::open(&p).expect("打开");

    assert!(e.selection().is_none(), "没有锚点时不应该有选区");
    e.anchor = Some(Pos::new(0, 2));
    e.cursor = Pos::new(0, 2);
    assert!(e.selection().is_none(), "锚点与光标重合时不应该有选区");

    e.cursor = Pos::new(0, 5);
    assert_eq!(e.selection(), Some((Pos::new(0, 2), Pos::new(0, 5))));
    // 反向拖选：返回的区间仍然要是有序的，否则切片会 panic。
    e.anchor = Some(Pos::new(1, 1));
    e.cursor = Pos::new(0, 1);
    assert_eq!(e.selection(), Some((Pos::new(0, 1), Pos::new(1, 1))));
}

#[test]
fn opening_a_missing_file_is_an_error_not_an_empty_editor() {
    let t = Tmp::new("missing");
    let p = t.0.join("nope.txt");
    assert!(Editor::open(&p).is_err(), "打不开的文件应该报错，而不是默默给个空编辑器");
}
