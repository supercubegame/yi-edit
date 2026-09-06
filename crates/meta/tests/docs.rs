//! 文档的断言。重点是那条**负向**的：它才能区分「拆干净了」与「各留一份」。

use yi_edit_meta as meta;

/// 两份规矩文件必须逐字节相同。一份改了另一份没改，是这类结构最常见的分岜方式。
#[test]
fn agents_and_claude_are_byte_identical() {
    let a = meta::read("AGENTS.md");
    let c = meta::read("CLAUDE.md");
    assert_eq!(a, c, "AGENTS.md 与 CLAUDE.md 已经分岜");
}

/// 行数上限本身得是一条断言：写在文件里的「限 200 行」不会阻止任何人。
/// 它红的时候只能压措词或拆文件，绝不能调宽这个数。
#[test]
fn the_instruction_file_stays_within_its_line_budget() {
    let n = meta::read("AGENTS.md").lines().count();
    assert!(
        n <= 200,
        "AGENTS.md 已经 {n} 行，上限 200（压措词或拆文件，不要调宽上限）"
    );
}

/// 拆分的四条一起断：指令那份不超限、档案那份存在且不空、档案真的被引用、
/// **那几节标题不在指令那份里**。只有最后一条能区分拆完与各留一份。
#[test]
fn the_archive_was_moved_out_not_copied() {
    const MOVED_SECTIONS: &[&str] = &["测不出来", "阈值实测值", "目前的限制"];
    let agents = meta::read("AGENTS.md");
    let pit = meta::read("docs/PITFALLS.md");

    assert!(pit.trim().len() > 200, "docs/PITFALLS.md 几乎是空的");
    assert!(
        agents.contains("docs/PITFALLS.md"),
        "AGENTS.md 里没指向档案文件，那份文档永远不会被读到"
    );

    let pit_titles: Vec<String> = meta::md_sections(&pit)
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    let agents_titles: Vec<String> = meta::md_sections(&agents)
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    for s in MOVED_SECTIONS {
        assert!(
            pit_titles.iter().any(|t| t.contains(s)),
            "档案里缺了一节：{s}（现有：{pit_titles:?}）"
        );
        assert!(
            !agents_titles.iter().any(|t| t.contains(s)),
            "{s} 在指令文件里也留了一份，这就是「各留一份」，两边会各自长歪"
        );
    }
}

/// 文档里不要写条数：那类数字注定漂，而一份跟不上实情的文档比没文档糟。
/// 这里只抓最容易漂的那几个句式。
#[test]
fn the_instruction_file_does_not_hardcode_counts() {
    let agents = meta::read("AGENTS.md");
    for bad in ["条断言", "个 crate", "条流水线", "个 job"] {
        assert!(
            !agents.contains(bad),
            "AGENTS.md 里写了会漂的条数（{bad}），改成描述或交给断言"
        );
    }
}
