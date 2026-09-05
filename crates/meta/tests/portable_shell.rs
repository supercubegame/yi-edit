//! 跑在多个平台上的脚本必须兼容 bash 3.2。
//!
//! 为什么这条值得一个扫描器：**实测踩过**。macOS runner 的 /bin/bash 是 3.2.57（2007 年），
//! 一句 bash 4 年代的写法让 macOS 那条 job 在**评论已经成功发出去之后**才红，
//! 而面板上看不出任何与 shell 版本有关的线索 —— 三个平台里只有一个红，
//! 看起来像平台特有的网络/权限问题。一条规矩被违反了就该变成断言。

use yi_edit_meta as meta;

/// bash 4+ 才有的写法。每一条都带理由：没理由的禁止项到头来会被人直接删掉。
const BANNED: &[(&str, &str)] = &[
    ("declare -A", "关联数组是 bash 4+"),
    ("local -A", "同上"),
    ("mapfile", "bash 4+"),
    ("readarray", "bash 4+"),
    ("&>>", "这种追加重定向是 bash 4+"),
    (",,}", "小写化展开是 bash 4+"),
    ("^^}", "大写化展开是 bash 4+"),
    ("${EPOCHSECONDS", "bash 5+"),
    ("wait -n", "bash 4.3+"),
];

/// 跑在三个平台上的脚本。这份名单就是期望：新加一个要跑在 macOS 上的脚本
/// 却忘了登记，它就完全在扫描范围之外。
const CROSS_PLATFORM: &[&str] = &["scripts/report.sh", "scripts/attest.sh", "scripts/raw-log.sh"];

fn banned_hits(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let t = line.trim();
        if t.starts_with('#') {
            continue;
        }
        for (pat, why) in BANNED {
            if t.contains(pat) {
                out.push(format!("{}: {pat}（{why}）：{t}", i + 1));
            }
        }
    }
    out
}

/// 正向自证：扫描器必须能拓到一个合成的违规写法，也必须放过注释里的提到。
/// 只验一侧的话，一个永远返回空列表的扫描器也能完美交差。
#[test]
fn the_scanner_actually_catches_bash4_syntax() {
    let bad = "#!/usr/bin/env bash\ndeclare -A m\nmapfile -t x < f\n";
    assert_eq!(
        banned_hits(bad).len(),
        2,
        "扫描器没拓到合成的违规写法：{:?}",
        banned_hits(bad)
    );
    let commented = "# 这里提到 declare -A 和 mapfile\necho ok\n";
    assert!(
        banned_hits(commented).is_empty(),
        "注释里的提到被当成了真用法（误报）"
    );
}

#[test]
fn cross_platform_scripts_avoid_bash4_only_syntax() {
    let mut bad = Vec::new();
    for s in CROSS_PLATFORM {
        assert!(meta::exists(s), "{s} 不存在，登记表已经过期了");
        for hit in banned_hits(&meta::read(s)) {
            bad.push(format!("{s}:{hit}"));
        }
    }
    assert!(
        bad.is_empty(),
        "这些脚本要在 macOS 的 bash 3.2 上跑：\n{}",
        bad.join("\n")
    );
}

/// 回写脚本里的变量要先初始化：它们带 `set -u`，而 bash 3.2 对多行命令的解析
/// 与新版不完全一致——上一轮就是一个未初始化的变量把整个 job 弄红的。
#[test]
fn the_report_script_initialises_its_variables() {
    let src = meta::read("scripts/report.sh");
    assert!(src.contains("set -Eeuo pipefail"), "report.sh 没开 set -u");
    for var in ["channel=\"\"", "existing=\"\"", "attempts=0", "pr=\"\""] {
        assert!(
            src.contains(var),
            "report.sh 没有先初始化 {var}，配上 set -u 就是一颗只在某些平台上炸的雷"
        );
    }
}
