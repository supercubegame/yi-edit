//! 闸门自己的断言。闸门坏了不会喊，所以得有东西盯着它。

use yi_edit_meta as meta;

/// 快闸门必须保持快：不能编 GUI。`--workspace` 会把 egui 拉进来，
/// 一轮几分钟之后就没人愿意跑它了，而一个没人跑的闸门等于没有闸门。
#[test]
fn the_fast_gate_does_not_build_the_gui() {
    let sh = meta::read("scripts/verify.sh");
    assert!(sh.contains("-p yi-edit-core"), "快闸门没有按 crate 选择测试目标");
    let code: Vec<&str> = sh
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    let joined = code.join("\n");
    assert!(
        !joined.contains("cargo test --workspace"),
        "快闸门里出现了 cargo test --workspace，它会把 GUI 一起编"
    );
}

/// marker 只能有一份真身。两个脚本各拄一份字面量的话，改一侧另一侧不会喊，
/// 而表现是「去重失效、attest 找不到」—— 两种都很难从面板上看出来。
/// 这里的负向断言（脚本里不得出现字面量）是承重的那一条。
#[test]
fn the_marker_has_exactly_one_source_of_truth() {
    let marker = meta::read("scripts/marker.txt");
    let m = marker.lines().next().unwrap_or_default().to_string();
    assert!(m.starts_with("<!--") && m.ends_with("-->"), "marker 不是一个 HTML 注释：{m:?}");
    assert!(m.len() > 10, "marker 太短，容易误命中：{m:?}");

    let core = m
        .trim_start_matches("<!--")
        .trim_end_matches("-->")
        .trim()
        .to_string();
    for script in ["scripts/report.sh", "scripts/attest.sh"] {
        let src = meta::read(script);
        assert!(
            src.contains("scripts/marker.txt"),
            "{script} 没从 marker.txt 读，它在拄第二份"
        );
        assert!(
            !src.contains(&core),
            "{script} 里硬编了 marker 字面量（{core}），真身就不止一份了"
        );
    }
}

/// 引用的 action 必须铉在不可变的 40 位提交 SHA 上。铉分支或 tag 是可变引用：
/// 上游改一行就能悄悄改掉我这边所有流水线的行为。
#[test]
fn every_action_is_pinned_to_an_immutable_sha() {
    let mut checked = 0usize;
    for f in meta::list_dir(".github/workflows") {
        if !(f.ends_with(".yml") || f.ends_with(".yaml")) {
            continue;
        }
        let src = meta::read(&format!(".github/workflows/{f}"));
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if t.starts_with('#') {
                continue;
            }
            let Some(rest) = t.strip_prefix("uses:") else {
                continue;
            };
            let reference = rest.trim().split_whitespace().next().unwrap_or_default();
            let Some((_, pin)) = reference.rsplit_once('@') else {
                panic!("{f}:{} 的 uses 没有 @ 固定：{t}", i + 1);
            };
            assert_eq!(pin.len(), 40, "{f}:{} 不是 40 位 SHA：{t}", i + 1);
            assert!(
                pin.chars().all(|c| c.is_ascii_hexdigit()),
                "{f}:{} 铉的不是十六进制：{t}",
                i + 1
            );
            checked += 1;
        }
    }
    assert!(checked >= 2, "只检查了 {checked} 个 uses，这条断言在测空气");
}

/// 兑底评论必须前置：先发一条只带结果的简版，富文本版再盖掉它。
/// 否则兑底路径会烂在一个从没走过的分支里，而它恰好是出事时唯一的依靠。
#[test]
fn the_fallback_report_is_posted_before_the_rich_one() {
    let wf = meta::read(".github/workflows/verify.yml");
    let min = wf.find("gate-report-min.md").expect("没有兑底报告调用");
    let rich = wf.find("report.sh gate-report.md").expect("没有富文本报告调用");
    assert!(min < rich, "富文本报告写在了兑底报告前面");
    assert!(wf.contains("scripts/attest.sh"), "没有送达核对，回写坏掉不会有人知道");
}

/// 慢闸门除了挂 push/PR，还得有一条定时：上游（egui / image / rustc）漂是
/// **时间驱动**的，人家改格式那天我可能一行代码都没推。
#[test]
fn the_slow_gate_also_runs_on_a_schedule() {
    let wf = meta::read(".github/workflows/verify.yml");
    assert!(wf.contains("schedule:"), "没有定时触发，上游漂会静默很久");
    assert!(wf.contains("cron:"), "schedule 下面没有 cron");
}
