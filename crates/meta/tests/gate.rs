//! 闸门自己的断言。闸门坏了不会喊，所以得有东西盯着它。

use std::collections::HashSet;

use yi_edit_meta as meta;

fn markers() -> Vec<String> {
    meta::read("scripts/marker.txt")
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn verify_code() -> Vec<String> {
    meta::read("scripts/verify.sh")
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

/// 快闸门必须保持快：不能编 GUI。`--workspace` 会把 egui 拉进来，
/// 一轮几分钟之后就没人愿意跑它了，而一个没人跑的闸门等于没有闸门。
#[test]
fn the_fast_gate_does_not_build_the_gui() {
    let sh = meta::read("scripts/verify.sh");
    assert!(
        sh.contains("-p yi-edit-core"),
        "快闸门没有按 crate 选择测试目标"
    );
    let joined = verify_code().join("\n");
    assert!(
        !joined.contains("cargo test --workspace"),
        "快闸门里出现了 cargo test --workspace，它会把 GUI 一起编"
    );
    assert!(
        !joined.contains("-p yi-edit "),
        "快闸门里选了 GUI 那个包，同样会把 egui 拉进来"
    );
}

/// **闸门必须跑完每个测试二进制。** `cargo test` 默认 fail-fast：一个二进制红之后
/// 它直接退出，后面的一个都不跑，而「没跑」与「通过了」在输出里长得一模一样。
/// 实测：一次变异体审计里四个变异体只有一个被审判，其余三个既没被抓也没被放过。
#[test]
fn the_gate_runs_every_test_binary_even_after_a_failure() {
    let mut checked = 0usize;
    for line in verify_code() {
        let t = line.trim().to_string();
        if !t.contains("cargo test") {
            continue;
        }
        checked += 1;
        assert!(
            t.contains("--no-fail-fast"),
            "这行 cargo test 没带 --no-fail-fast，第一个红的二进制会把后面全部淹掉：{t}"
        );
    }
    assert_eq!(checked, 1, "期望恰好一处 cargo test，实际 {checked} 处（新加的那处也要带开关）");
}

/// **失败详情必须在日志末尾。** 实测踩过：回写只带末尾 N 行，而参考项（fmt）
/// 的输出接在测试输出后面且足够长，把失败的测试名全部挤出了窗口。
/// 那条评论告诉我「失败 1 项」而定位不到根因，等于没有报告。
#[test]
fn the_failure_detail_lands_at_the_end_of_the_gate_log() {
    let code = verify_code();
    let joined = code.join("\n");

    assert!(
        joined.contains("FAILURE SUMMARY"),
        "没有失败摘要节，失败详情会被前面的输出挤出回写窗口"
    );
    assert!(
        joined.contains("gate-failed-steps.txt"),
        "摘要节没有登记失败步骤，它只能输出一句空话"
    );

    // 负向那条是承重的：参考项的原始输出一旦重定向进闸门日志，同一个坑就回来了。
    for line in &code {
        let t = line.trim();
        if t.starts_with("cargo fmt") {
            assert!(
                !t.contains("${LOG}") && !t.contains("$LOG") && !t.contains("gate.log"),
                "fmt 的原始输出又往闸门日志里堆了：{t}"
            );
        }
    }
}

/// 每一种报告占一个 marker，且真身只在 marker.txt 里。
/// 负向那条（脚本里不得出现字面量）是承重的：拄两份的失败方式是
/// 「去重失效 / attest 找不到」，两种都很难从面板上看出来。
#[test]
fn every_marker_has_exactly_one_source_of_truth() {
    let ms = markers();
    assert!(
        ms.len() >= 5,
        "marker 只登记了 {} 个，不够每种报告一个",
        ms.len()
    );
    let uniq: HashSet<&String> = ms.iter().collect();
    assert_eq!(
        uniq.len(),
        ms.len(),
        "marker 有重复，它们会互相覆盖：{ms:?}"
    );

    let scripts = [
        "scripts/report.sh",
        "scripts/attest.sh",
        "scripts/raw-log.sh",
    ];
    for s in scripts {
        let src = meta::read(s);
        for m in &ms {
            let core = m
                .trim_start_matches("<!--")
                .trim_end_matches("-->")
                .trim()
                .to_string();
            assert!(
                !src.contains(&core),
                "{s} 里硬编了 marker 字面量（{core}），真身就不止一份了"
            );
        }
    }
    for m in &ms {
        assert!(
            m.starts_with("<!--") && m.ends_with("-->") && m.len() > 10,
            "marker 不是一个够长的 HTML 注释：{m:?}"
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
    let rich = wf
        .find("report.sh gate-report.md")
        .expect("没有富文本报告调用");
    assert!(min < rich, "富文本报告写在了兑底报告前面");
    assert!(
        wf.contains("scripts/attest.sh"),
        "没有送达核对，回写坏掉不会有人知道"
    );
}

/// 每一条流水线、每一个平台，红了都要能把原始输出送到我读得到的地方。
/// 实测踩过：产物管道静默挂掉，手里只剩一个比特。
#[test]
fn every_platform_can_post_its_raw_log() {
    let wf = meta::read(".github/workflows/verify.yml");
    let n = wf.matches("scripts/raw-log.sh").count();
    assert!(n >= 2, "只有 {n} 处调用了 raw-log.sh，快闸门与慢闸门都得有");
    // 矩阵里每个平台一个 marker 行，否则三个平台的日志会互相覆盖。
    for line in ["marker_line: 3", "marker_line: 4", "marker_line: 5"] {
        assert!(wf.contains(line), "矩阵里缺了 {line}");
    }
    assert!(
        wf.contains("actions: read"),
        "permissions 里没给 actions: read，下载产物会静默挂掉（已经挂过一次）"
    );
}

/// 慢闸门除了挂 push/PR，还得有一条定时：上游（egui / image / rustc）漂是
/// **时间驱动**的，人家改格式那天我可能一行代码都没推。
#[test]
fn the_slow_gate_also_runs_on_a_schedule() {
    let wf = meta::read(".github/workflows/verify.yml");
    assert!(wf.contains("schedule:"), "没有定时触发，上游漂会静默很久");
    assert!(wf.contains("cron:"), "schedule 下面没有 cron");
}
