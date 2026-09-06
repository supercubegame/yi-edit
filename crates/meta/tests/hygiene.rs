//! 仓库根目录也是一张登记表。
//!
//! 为什么需要它：格式化流水线的第一版用了 `git add -A`，于是把**自己的日志**
//! 提交进了仓库（fmt-run.txt 等四个）。那一轮所有闸门全绿：仓库变脏不会让任何东西报错。
//!
//! 而后果不只是脏：那几份日志每次运行都不一样，于是「零改动就不提交」那一层循环终止
//! **直接失效**了 —— 每次推送都会多一个垃圾提交，而它看起来很像「流水线在正常工作」。

use yi_edit_meta as meta;

/// 仓库根应该有的东西。双向断言：少了一个或多了一个都红。
const ROOT_ENTRIES: &[&str] = &[
    ".git",
    ".github",
    ".gitignore",
    "AGENTS.md",
    "CLAUDE.md",
    "Cargo.toml",
    "README.md",
    "crates",
    "docs",
    "scripts",
];

/// 工作流与脚本会写到仓库里的产物（或曾经会）。每一个都必须被 .gitignore 盖住。
const ARTIFACTS: &[&str] = &[
    "gate.log",
    "gate-result.txt",
    "gate-metrics.txt",
    "fmt.log",
    "raw-log.md",
    "raw-log-run.txt",
    "bench-run.txt",
    "app-run.txt",
    "build.log",
    "outcomes.txt",
    "shotcheck.txt",
    "fmt-run.txt",
    "fmt-before.txt",
    "fmt-after.txt",
    "fmt-apply.txt",
    "fmt-outcomes.txt",
    "gate-report.md",
    "gate-report-min.md",
    "sample.rs",
    "big.rs",
];

#[test]
fn the_repo_root_has_exactly_the_registered_entries() {
    let mut actual: Vec<String> = meta::list_dir(".")
        .into_iter()
        // 本地跑时可能有被忽略的产物；它们的存在不是问题，被提交才是。
        // 所以这里只看“不在忽略名单里的东西”。
        .filter(|n| !ARTIFACTS.contains(&n.as_str()))
        .filter(|n| n != "target" && n != "Cargo.lock")
        .collect();
    actual.sort();
    let mut want: Vec<String> = ROOT_ENTRIES.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(
        actual, want,
        "仓库根的内容与登记不一致（左=实际 右=登记）。多出来的那几个很可能是某条流水线把自己的临时文件提交进来了"
    );
}

#[test]
fn every_artifact_name_is_ignored() {
    let ignore = meta::read(".gitignore");
    let lines: Vec<&str> = ignore
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    for a in ARTIFACTS {
        let covered = lines.iter().any(|l| {
            *l == *a
                || l.trim_start_matches('/') == *a
                || l.strip_suffix('*')
                    .map(|p| a.starts_with(p))
                    .unwrap_or(false)
        });
        assert!(
            covered,
            "{a} 不在 .gitignore 里。它被提交进仓库时不会有任何东西报错"
        );
    }
}

/// 格式化流水线只能提交 .rs，而且临时文件要写在仓库外。
/// 两条都是承重的：它们各自就能防住那个已经发生过的事故。
#[test]
fn the_formatter_stages_only_rust_files_and_keeps_scratch_outside_the_repo() {
    let src = meta::read(".github/workflows/format.yml");
    assert!(
        !src.contains("git add -A"),
        "格式化流水线又用了 git add -A，它会把未跟踪的临时文件一起提交（已经发生过一次）"
    );
    assert!(
        src.contains("git add -u -- '*.rs'"),
        "没有只暂存已跟踪的 .rs"
    );
    assert!(
        src.contains("git diff --cached --name-only"),
        "没有检查暂存区里到底暂存了什么"
    );
    assert!(
        src.contains("grep -v '\\.rs$'"),
        "没有那条拒绝非 .rs 的负向守卫"
    );
    assert!(
        src.contains("runner.temp"),
        "临时文件还写在仓库里，一个写歪的 git add 就能把它们提交进去"
    );
    // 旧的相对路径写法不得回来。
    for bad in ["> fmt-run.txt", "> fmt-before.txt", "> fmt-after.txt"] {
        assert!(
            !src.contains(bad),
            "临时文件又写回仓库根了（{bad}）"
        );
    }
}
