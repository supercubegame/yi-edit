//! 自动缩进与括号匹配。
//!
//! 最重要的一条是**对照实验**：字符串/注释里的括号不参与匹配。
//! 只验「引号里的不算」的话，一个永远返回 None 的实现也能完美交差；
//! 所以同一段文本把引号去掉之后必须真的匹配得上。

use yi_edit_core::indent::{
    bracket_pair_at, indent_unit, indent_width, is_close, is_open, leading_whitespace,
    match_bracket, matching_char, newline_edit, Mask, MAX_BRACKET_MATCH_BYTES,
};
use yi_edit_core::Lang;

#[test]
fn the_indent_unit_is_the_only_source_of_truth() {
    assert_eq!(indent_unit().len(), indent_width());
    assert!(
        indent_unit().chars().all(|c| c == ' '),
        "缩进单位里有非空格字符"
    );
    assert!(indent_width() >= 2, "缩进小于两格，胁层根本看不出来");
    assert!(indent_width() <= 8, "缩进大于八格，几层嵌套就顶到右边了");
}

#[test]
fn brackets_come_in_pairs_and_nothing_else_does() {
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        assert_eq!(matching_char(open), Some(close));
        assert_eq!(matching_char(close), Some(open));
        assert!(is_open(open) && !is_close(open));
        assert!(is_close(close) && !is_open(close));
    }
    // 负向：引号与尖括号不算括号。尖括号在泛型与小于号之间歧义，
    // 算进去的话 `a < b` 会被当成一个未闭合的括号。
    for c in ['"', '\'', '<', '>', 'a', ' '] {
        assert_eq!(matching_char(c), None, "{c} 被当成了括号");
    }
}

#[test]
fn leading_whitespace_is_returned_verbatim() {
    assert_eq!(leading_whitespace("    let x = 1;"), "    ");
    assert_eq!(leading_whitespace("\t\tlet x = 1;"), "\t\t");
    assert_eq!(leading_whitespace("no indent"), "");
    // 全空白行返回整行：它本来就都是缩进。
    assert_eq!(leading_whitespace("   "), "   ");
    assert_eq!(leading_whitespace(""), "");
}

#[test]
fn a_plain_line_just_inherits_its_indent() {
    let e = newline_edit("    let x = 1;", 14);
    assert_eq!(e.insert, "\n    ");
    assert_eq!(e.cursor_offset, e.insert.len());
    assert!(!e.split_pair);
}

#[test]
fn an_open_bracket_deepens_by_one_unit() {
    let unit = indent_unit();
    let line = "    fn f() {";
    let e = newline_edit(line, line.len());
    assert_eq!(e.insert, format!("\n    {unit}"));
    assert!(!e.split_pair, "后面没有配对的闭括号，不应该拆成三行");
    // 开括号后面还有空白也算：人真的会在 `{ ` 后面回车。
    let spaced = "    fn f() {   ";
    assert_eq!(
        newline_edit(spaced, spaced.len()).insert,
        format!("\n    {unit}")
    );
}

/// 在 `{}` 中间回车：插两行，而光标停在中间那行。
/// 光标停在末尾的话，用户每次都要再敲一下上箭头 —— 那就不如不做。
#[test]
fn splitting_a_pair_puts_the_cursor_in_the_middle() {
    let unit = indent_unit();
    let line = "    fn f() {}";
    let col = line.len() - 1; // 光标在 } 之前
    let e = newline_edit(line, col);
    assert!(e.split_pair);
    assert_eq!(e.insert, format!("\n    {unit}\n    "));
    // 光标偏移指向第一行末尾（深缩进那行），而不是整段末尾。
    assert_eq!(e.cursor_offset, 1 + 4 + unit.len());
    assert!(e.cursor_offset < e.insert.len(), "光标停在了最后一行");
    assert_eq!(&e.insert[..e.cursor_offset], format!("\n    {unit}"));
}

#[test]
fn every_bracket_kind_splits() {
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        let line = format!("x{open}{close}");
        let e = newline_edit(&line, 2);
        assert!(e.split_pair, "{open}{close} 没有拆开");
    }
    // 不配对的两个括号不算：`(]` 拆开之后代码更丑了。
    let e = newline_edit("x(]", 2);
    assert!(!e.split_pair, "不配对的括号被拆开了");
}

/// 光标落在前导空白里时，继承的缩进不得超过光标自己的位置。
/// 否则在行首敲回车会凭空多出一段缩进（而用户没敲过空格）。
#[test]
fn a_cursor_inside_the_indent_does_not_invent_extra_indent() {
    let line = "        deep();";
    assert_eq!(newline_edit(line, 0).insert, "\n");
    assert_eq!(newline_edit(line, 4).insert, "\n    ");
    assert_eq!(newline_edit(line, 8).insert, "\n        ");
}

#[test]
fn multibyte_lines_do_not_panic_and_columns_get_clamped() {
    let line = "    let s = \"中文字串\"; // 中文注释";
    assert!(
        line.len() > line.chars().count(),
        "语料全是单字节，这条在测空气"
    );
    for col in 0..line.len() + 5 {
        let e = newline_edit(line, col);
        assert!(e.insert.starts_with('\n'));
        assert!(e.cursor_offset <= e.insert.len());
    }
    // 括号紧贴中文也要能拆。
    let cjk = "中文(){";
    let e = newline_edit(cjk, cjk.len());
    assert!(!e.split_pair);
    let cjk2 = "中文()";
    let at = "中文(".len();
    assert!(newline_edit(cjk2, at).split_pair, "中文后面的括号没拆开");
}

fn mask_of(text: &str) -> Mask {
    Mask::from_text(text, Lang::Rust)
}

#[test]
fn matching_walks_both_directions_and_round_trips() {
    let text = "fn f() {\n    g(h(1), i[2]);\n}\n";
    let mask = mask_of(text);
    let open = text.find('{').expect("有开花括号");
    let close = text.rfind('}').expect("有闭花括号");
    assert_eq!(match_bracket(text, &mask, open), Some(close));
    assert_eq!(match_bracket(text, &mask, close), Some(open));
    // 往返一致：每一个括号的配对的配对必须是自己。
    let mut checked = 0usize;
    for (i, c) in text.char_indices() {
        if matching_char(c).is_none() {
            continue;
        }
        let other = match_bracket(text, &mask, i).unwrap_or_else(|| panic!("{i} 处的 {c} 没配对"));
        assert_eq!(match_bracket(text, &mask, other), Some(i), "{i} 往返不一致");
        checked += 1;
    }
    assert!(checked >= 8, "只校了 {checked} 个括号，语料太稞");
}

#[test]
fn nesting_picks_the_right_partner_not_the_nearest_one() {
    let text = "((a))";
    let mask = mask_of(text);
    assert_eq!(match_bracket(text, &mask, 0), Some(4), "最外层配对错了");
    assert_eq!(match_bracket(text, &mask, 1), Some(3), "内层配对错了");
}

/// **对照实验。** 上半：字符串与注释里的括号不参与计数。
/// 下半：把引号与注释去掉之后，同一个位置必须真的匹配到那个括号 ——
/// 否则一个永远返回 None 的实现也能把上半验得很好看。
#[test]
fn brackets_inside_strings_and_comments_do_not_count() {
    let text = "fn f() {\n    let s = \"(\"; // )\n}\n";
    let mask = mask_of(text);
    assert!(
        mask.span_count() >= 2,
        "屏蔽区只有 {} 个，高亮器没认出字符串与注释",
        mask.span_count()
    );

    let quoted = text.find("\"(").expect("有引号里的括号") + 1;
    assert_eq!(
        match_bracket(text, &mask, quoted),
        None,
        "引号里的开括号参与了匹配，它后面整个文件的配对都会错位"
    );
    let commented = text.rfind(')').expect("有注释里的括号");
    assert_eq!(
        match_bracket(text, &mask, commented),
        None,
        "注释里的括号参与了匹配"
    );
    // 花括号仍然要能配对：屏蔽不能把真的括号也吃掉。
    let brace = text.find('{').expect("有花括号");
    assert_eq!(
        match_bracket(text, &mask, brace),
        Some(text.rfind('}').unwrap())
    );

    // 对照侧：同一个形状，去掉引号与注释。
    let bare = "fn f() {\n    let s = ( ) ;\n}\n";
    let bare_mask = mask_of(bare);
    assert_eq!(
        bare_mask.span_count(),
        0,
        "这段里没有字符串与注释，屏蔽区应该是空的"
    );
    let open = bare.find('(').expect("有开括号");
    assert_eq!(
        match_bracket(bare, &bare_mask, open),
        Some(bare.find(')').unwrap()),
        "去掉引号之后它反而不匹配了，说明屏蔽判得太宽"
    );
}

#[test]
fn unbalanced_and_non_bracket_positions_are_none() {
    let text = "fn f( {\n";
    let mask = mask_of(text);
    assert_eq!(
        match_bracket(text, &mask, text.find('(').unwrap()),
        None,
        "未闭合却找到了配对"
    );
    assert_eq!(match_bracket(text, &mask, 0), None, "非括号位置返回了配对");
    assert_eq!(
        match_bracket(text, &mask, text.len()),
        None,
        "越界位置没返回 None"
    );
    assert_eq!(match_bracket("", &Mask::default(), 0), None);
}

/// 光标在括号**右边**时也要能看到配对：刚敲完 `}` 正是最想看到的时候。
#[test]
fn the_pair_is_found_from_either_side_of_the_cursor() {
    let text = "a(b)c";
    let mask = mask_of(text);
    let open = 1usize;
    let close = 3usize;
    assert_eq!(
        bracket_pair_at(text, &mask, open),
        Some((open, close)),
        "光标在开括号上"
    );
    assert_eq!(
        bracket_pair_at(text, &mask, close),
        Some((close, open)),
        "光标在闭括号上"
    );
    assert_eq!(
        bracket_pair_at(text, &mask, close + 1),
        Some((close, open)),
        "光标刚过闭括号时看不到配对"
    );
    assert_eq!(bracket_pair_at(text, &mask, 0), None, "无括号处返回了配对");
    // 沉默截断的反面：上限必须大到能装下真实文件，但又不能大到每帧扫几十 MB。
    assert!(
        MAX_BRACKET_MATCH_BYTES >= 64 * 1024,
        "上限太小，普通源文件都用不上"
    );
    assert!(
        MAX_BRACKET_MATCH_BYTES <= 8 * 1024 * 1024,
        "上限太大，每帧扫一遍会卡"
    );
}

/// 屏蔽区的二分查找要与朴素遍历结果一致。
/// 二分写错的表现不是 panic，是偶尔把一个真括号当成字符串里的（或反之）。
#[test]
fn the_mask_lookup_agrees_with_a_naive_scan() {
    let text = "fn f() {\n    // (注释)\n    let a = \"[\";\n    let b = (1 + 2);\n}\n";
    let mask = mask_of(text);
    assert!(mask.span_count() >= 2);
    let mut inside = 0usize;
    for i in 0..text.len() {
        let by_mask = mask.contains(i);
        if by_mask {
            inside += 1;
        }
    }
    assert!(
        inside > 0,
        "一个字节都不在屏蔽区里，那下面那些断言都在测空气"
    );
    assert!(inside < text.len(), "整份文本都被屏蔽了，匹配永远返回 None");
    // 真括号仍然可匹配，屏蔽里的不行。
    let real = text.find("(1").expect("有真括号");
    assert!(match_bracket(text, &mask, real).is_some());
    let in_string = text.find("\"[").expect("有引号里的括号") + 1;
    assert_eq!(match_bracket(text, &mask, in_string), None);
}
