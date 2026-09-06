//! 仓库根目录也是一张登记表。
//!
//! 为什么需要它：格式化流水线的第一版用了 `git add` 的全量形式，于是把**自己的日志**
//! 提交进了仓库（四个 txt）。那一轮所有闸门全绿：仓库变脏不会让任何东西报错。
//!
//! 而后果不只是脏：那几份日志每次运行都不一样，于是「零改动就不提交」那一层循环终止
//! **直接失效**了 —— 每次推送都会多一个垃圾提交，而它看起来很像「流水线在正常工作」。
//!
//! **忽略名单不在这里再写一份。** 第一版手写了一张产物表，第一次跑就假红：
//! 闸门自己的 `gate-step-1.log` 不在那张表里。两张登记表必定分岔，所以现在
//! 忽略名单直接读 `.gitignore`（唯一真身）。
//!
//! **而那个匹配器第一版也是错的，是它自己的自证拓到的：** 只支持尾部 `*`，
//! 而 `.gitignore` 里真实存在的是 `gate-step-*.log`（星号在中间）。
//! 没有那条双向自证的话，它会默默把闸门产物当成「未登记的垃圾」，变成一台假红工厂。

use yi_edit_meta as meta;

/// 仓库根应该有的东西（不含被 .gitignore 盖住的产物）。双向断言：少了或多了都红。
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

/// `.gitignore` 里的模式（去注释、去空行）。
fn ignore_patterns() -> Vec<String> {
    meta::read(".gitignore")
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

/// 这个名字被忽略了吗。只支持本仓库实际用到的形状：精确名、`/前缀`、
/// 以及**一个星号（位置不限）**。写一个完整的 gitignore 引擎是过度工程，
/// 但不支持的形状必须有一条断言拦着，否则将来有人写了一个我读不懂的模式，
/// 而这里会默默当它没忽略。
fn is_ignored(name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| {
        let p = p.trim_start_matches('/');
        match p.split_once('*') {
            Some((pre, post)) => {
                name.len() >= pre.len() + post.len()
                    && name.starts_with(pre)
                    && name.ends_with(post)
            }
            None => p == name,
        }
    })
}

#[test]
fn the_ignore_matcher_only_meets_shapes_it_understands() {
    let patterns = ignore_patterns();
    assert!(patterns.len() >= 10, "只读到 {} 条忽略规则", patterns.len());
    for p in &patterns {
        let body = p.trim_start_matches('/');
        let unsupported = body.contains("**")
            || body.contains('?')
            || body.contains('[')
            || body.matches('*').count() > 1;
        // `**/*.rs.bk` 这种形状不影响根目录判断（根下不会出现 .rs.bk 目录项），
        // 所以只要求它们带着目录分隔符 —— 不带的话就是一个我读不懂的根级模式。
        if unsupported {
            assert!(
                body.contains('/'),
                "{p} 是一个本匹配器读不懂的根级模式，它会被默默当成「没忽略」"
            );
        }
    }
    // 双向自证：该匹配的匹配，不该匹配的不匹配。
    // `gate-step-*.log` 是真实存在的模式，星号在中间 —— 第一版匹配器就挂在这里。
    let sample = vec![
        String::from("gate.log"),
        String::from("gate-step-*.log"),
        String::from("/shots"),
    ];
    assert!(is_ignored("gate.log", &sample));
    assert!(
        is_ignored("gate-step-1.log", &sample),
        "星号在中间的模式没匹配上"
    );
    assert!(is_ignored("shots", &sample));
    assert!(!is_ignored("Cargo.toml", &sample), "匹配器把正常文件也忽略了");
    assert!(!is_ignored("fmt-run.txt", &sample), "匹配器太宽松");
    assert!(
        !is_ignored("gate-step-", &sample),
        "长度不够的名字也被匹配了（前后缀重叠）"
    );
}

#[test]
fn the_repo_root_has_exactly_the_registered_entries() {
    let patterns = ignore_patterns();
    let mut actual: Vec<String> = meta::list_dir(".")
        .into_iter()
        // 被忽略的产物存在不是问题（本地与 CI 跑完都会有），**被提交**才是。
        .filter(|n| !is_ignored(n, &patterns))
        .filter(|n| n != "target" && n != "Cargo.lock")
        .collect();
    actual.sort();
    let mut want: Vec<String> = ROOT_ENTRIES.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(
        actual, want,
        "仓库根的内容与登记不一致（左=实际 右=登记）。多出来的那几个要么是某条流水线把自己的临时文件提交进来了，要么是忽略名单没跟上"
    );
}

/// 格式化流水线只能提交 .rs，而且临时文件要写在仓库外。
/// 两条都是承重的：它们各自就能防住那个已经发生过的事故。
///
/// **负向那几条必须先剥 YAML 注释：** 我在注释里写下了事故经过（包括那个全量
/// `git add` 的写法），于是一条已经修好的规矩被判成还坏着 —— 实测就这么红了一轮。
#[test]
fn the_formatter_stages_only_rust_files_and_keeps_scratch_outside_the_repo() {
    let src = meta::read(".github/workflows/format.yml");

    assert!(
        !meta::yaml_code_contains(&src, "add -A"),
        "格式化流水线又用了全量 git add，它会把未跟踪的临时文件一起提交（已经发生过一次）"
    );
    assert!(
        meta::yaml_code_contains(&src, "git add -u -- '*.rs'"),
        "没有只暂存已跟踪的 .rs"
    );
    assert!(
        meta::yaml_code_contains(&src, "git diff --cached --name-only"),
        "没有检查暂存区里到底暂存了什么"
    );
    assert!(
        meta::yaml_code_contains(&src, ".rs$"),
        "没有那条拒绝非 .rs 的负向守卫"
    );
    assert!(
        meta::yaml_code_contains(&src, "runner.temp"),
        "临时文件还写在仓库里，一个写歪的 git add 就能把它们提交进去"
    );

    // 剥离器自证：剥完之后还看得见真东西。剥成空字串的话，上面那几条负向断言
    // 全部免费通过，而正向那几条会全红 —— 后者至少会喊，但不能只靠它。
    let code = meta::strip_yaml_comments(&src);
    assert!(
        code.contains("jobs:") && code.contains("runs-on:"),
        "剥完 YAML 注释之后看不到任何结构，剥离器自己坏了"
    );
    assert!(
        src.contains("add -A"),
        "事故经过从注释里消失了；那么上面那条剥注释的断言也就不再验得到任何东西"
    );
}
