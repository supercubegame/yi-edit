//! 跑在多个平台上的脚本必须兼容 bash 3.2，而且两处必须一致的逻辑只能有一份真身。
//!
//! 为什么这几条值得扫描器：**同一类根因连红三轮**。macOS runner 的 /bin/bash 是 3.2.57
//! （2007 年），两次的症状都是「评论已经成功发出去了，job 才红」，且三个平台里
//! 只有一个红——看起来像平台特有的网络或权限问题。第三次是 PR 号解析分岜，
//! 它把一个完全正常的回写判成了「没送达」。
//! 一条规矩被违反两次，就该变成断言。

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
const CROSS_PLATFORM: &[&str] = &[
    "scripts/report.sh",
    "scripts/attest.sh",
    "scripts/raw-log.sh",
    "scripts/pr-number.sh",
];

fn code_lines(src: &str) -> Vec<(usize, &str)> {
    src.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim_start().starts_with('#'))
        .map(|(i, l)| (i + 1, l))
        .collect()
}

fn banned_hits(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (no, line) in code_lines(src) {
        for (pat, why) in BANNED {
            if line.contains(pat) {
                out.push(format!("{no}: {pat}（{why}）：{}", line.trim()));
            }
        }
    }
    out
}

/// `$var` 紧跟非 ASCII 字符。bash 3.2 在 C 语言环境下会把那几个字节当成变量名的
/// 一部分，配上 `set -u` 就是硬错。`${var}` 不会。
fn dollar_before_multibyte(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (no, line) in code_lines(src) {
        let b = line.as_bytes();
        let mut j = 0usize;
        while j < b.len() {
            let starts_name = b[j] == b'$'
                && j + 1 < b.len()
                && (b[j + 1].is_ascii_alphabetic() || b[j + 1] == b'_');
            if !starts_name {
                j += 1;
                continue;
            }
            let mut k = j + 1;
            while k < b.len() && (b[k].is_ascii_alphanumeric() || b[k] == b'_') {
                k += 1;
            }
            if k < b.len() && b[k] >= 0x80 {
                out.push(format!(
                    "{no}: ${} 紧跟非 ASCII 字符，改成 ${{{}}}：{}",
                    &line[j + 1..k],
                    &line[j + 1..k],
                    line.trim()
                ));
            }
            j = k;
        }
    }
    out
}

/// 双向自证：扫描器必须拓到合成的违规写法，也必须放过正确写法与注释。
/// 只验一侧的话，一个永远返回空列表的扫描器也能完美交差。
#[test]
fn the_scanners_catch_what_they_claim_to_catch() {
    let bad4 = "declare -A m\nmapfile -t x < f\n";
    assert_eq!(
        banned_hits(bad4).len(),
        2,
        "bash4 扫描器漏报：{:?}",
        banned_hits(bad4)
    );
    assert!(
        banned_hits("# 这里提到 declare -A 和 mapfile\necho ok\n").is_empty(),
        "bash4 扫描器把注释当成了真用法（误报）"
    );

    // 这一行里 $channel 紧跟一个全角括号 —— 实测把 macOS 那条 job 弄红了两轮。
    let bad_mb = "echo \"已写入 $channel（尝试 $attempts 次）\"\n";
    let hits = dollar_before_multibyte(bad_mb);
    assert_eq!(hits.len(), 1, "多字节扫描器漏报：{hits:?}");
    assert!(hits[0].contains("$channel"), "拓错了变量：{hits:?}");

    // 花括号写法与空格分隔都不得误报，否则它会变成一台假红工厂。
    assert!(
        dollar_before_multibyte("echo \"已写入 ${channel}（尝试 ${attempts} 次）\"\n").is_empty(),
        "花括号写法被误报了"
    );
    assert!(
        dollar_before_multibyte("echo \"已写入 $channel 次\"\n").is_empty(),
        "空格分隔的写法被误报了"
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

#[test]
fn cross_platform_scripts_never_put_a_variable_before_a_multibyte_char() {
    let mut bad = Vec::new();
    for s in CROSS_PLATFORM {
        for hit in dollar_before_multibyte(&meta::read(s)) {
            bad.push(format!("{s}:{hit}"));
        }
    }
    assert!(
        bad.is_empty(),
        "bash 3.2 会把非 ASCII 字节当成变量名的一部分（实测连红两轮）：\n{}",
        bad.join("\n")
    );
}

/// PR 号解析只能有一份真身。**负向那条是承重的**：光断言「两边都调了共享脚本」
/// 对「另外还自己写了一遍」毫无意见，而分岜的表现是回写完全正常却被判成没送达。
#[test]
fn the_pr_number_resolution_has_exactly_one_source_of_truth() {
    assert!(
        meta::exists("scripts/pr-number.sh"),
        "共享的 PR 解析脚本不存在"
    );
    let shared = meta::read("scripts/pr-number.sh");
    assert!(
        shared.contains("pull_request.number") && shared.contains("/pulls"),
        "共享脚本里两条途径必须都在（事件载荷 + API 退回）"
    );
    for s in ["scripts/report.sh", "scripts/attest.sh"] {
        let src = meta::read(s);
        assert!(
            src.contains("bash scripts/pr-number.sh"),
            "{s} 没调共享的 PR 解析"
        );
        for own in ["pull_request.number", "/pulls"] {
            let leaked = code_lines(&src).iter().any(|(_, l)| l.contains(own));
            assert!(
                !leaked,
                "{s} 自己又写了一遍 PR 解析（{own}），真身就不止一份了"
            );
        }
    }
}

/// 回写脚本里的变量要先初始化：它们带 `set -u`。
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
