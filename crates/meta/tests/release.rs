//! 发布流水线的断言。它比其它流水线危险：**它会把二进制给到外面**。
//!
//! 三个形状都不会报错，所以各配一条断言：
//! 1. 发了一份**没过闸门**的树 —— 「发布了」与「验过了」在页面上一模一样。
//! 2. Release 建出来了而**附件没上去** —— 下载的人看到一个空发布，而 CI 是绿的。
//! 3. 流水线**根本没触发** —— 与「没有新版本要发」在面板上一模一样。
//!
//! 第 3 条机器只能守一半（见下面那条注释），这一点写在 docs/RELEASING.md 里。

use yi_edit_meta as meta;

fn release_yml() -> String {
    meta::read(".github/workflows/release.yml")
}

/// 从 YAML 的非注释部分里读 `NAME: "数字"` 形式的值，返回所有出现。
fn yaml_numbers(src: &str, name: &str) -> Vec<u32> {
    let code = meta::strip_yaml_comments(src);
    let needle = format!("{name}:");
    code.lines()
        .filter_map(|l| {
            let t = l.trim();
            let rest = t.strip_prefix(&needle)?;
            let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
            digits.parse().ok()
        })
        .collect()
}

/// 只发布闸门验过的提交，而且两条触发路径都要真的问到结论。
///
/// 手动触发就免检的话，那个门禁等于没有 —— 而手动触发恰好是最容易被用的那条路。
#[test]
fn only_gate_verified_commits_get_released() {
    let src = release_yml();
    assert!(
        meta::yaml_code_contains(&src, "workflow_run:"),
        "没有挂在 verify 跑完之后，那就可能发一份没过闸门的树"
    );
    assert!(
        meta::yaml_code_contains(&src, "workflows: [\"verify\"]"),
        "workflow_run 没指向 verify"
    );
    let code = meta::strip_yaml_comments(&src);
    assert!(
        code.contains("workflow_run.conclusion"),
        "没读 verify 的 conclusion，那么红的那次也会发"
    );
    assert!(
        code.contains("select(.name == \"verify\")"),
        "手动触发时没去问 verify 的结论，那条路径上的门禁等于没有"
    );
    assert!(
        code.contains("!= \"success\""),
        "没有把「不是 success 就不发」写成一个真的分支"
    );
    assert!(
        code.contains("!= \"main\""),
        "没限定主干，任何分支的绿都能触发发布"
    );
}

/// 循环终止靠 tag 身份，不靠提交信息字符串。
/// 字符串守卫改一次模板就哑，而哑掉的表现不是红，是每次都重复发布。
#[test]
fn the_loop_terminates_on_tag_identity_not_on_a_commit_message() {
    let src = release_yml();
    let code = meta::strip_yaml_comments(&src);
    assert!(
        code.contains("git/ref/tags/v"),
        "没有查 tag 存不存在，那每次主干绿都会再发一次"
    );
    for banned in [
        "git log",
        "%s%b",
        "contains(github.event.head_commit.message",
    ] {
        assert!(
            !code.contains(banned),
            "终止条件靠上了提交信息（{banned}），那种守卫哑掉不会红"
        );
    }
    // 版本号只能有一份真身：流水线里不得写死版本号。
    let version = workspace_version();
    assert!(
        !code.contains(&format!("v{version}\"")),
        "流水线里抄了一份版本号（v{version}），两头会各自漂"
    );
    assert!(code.contains("Cargo.toml"), "版本号不是从 Cargo.toml 读的");
}

fn workspace_version() -> String {
    let toml = meta::read("Cargo.toml");
    toml.lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("version = \"")
                .and_then(|s| s.strip_suffix('"'))
                .map(|s| s.to_string())
        })
        .expect("Cargo.toml 里读不到版本号")
}

/// **「建了一个 Release」不等于「附件真的上去了」。**
/// 上传少了一个包的话，Release 页面仍然看起来很正常。所以发完要回查一次。
#[test]
fn the_release_verifies_its_own_assets_and_tag() {
    let src = release_yml();
    let code = meta::strip_yaml_comments(&src);
    assert!(
        code.contains("--json assets"),
        "发完没回查附件，一个空发布会以绿灯收场"
    );
    assert!(
        code.contains("ASSET COUNT FAILED"),
        "附件数不对时没有可识别的失败标记"
    );
    assert!(
        code.contains("PACKAGE COUNT FAILED"),
        "包数不对时没有可识别的失败标记（少了平台就不应该发）"
    );
    assert!(
        code.contains(".object.sha"),
        "没核对 tag 真的指向那个提交；下一轮的循环终止就靠它"
    );
}

/// 平台数与期望附件数是一组耦合参数：改一头必须重算另一头。
/// 只改矩阵不改期望数的后果是：第四个平台默默被丢掉，而且发布仍然绿。
#[test]
fn the_platform_count_and_the_expected_asset_count_stay_coupled() {
    let src = release_yml();
    let code = meta::strip_yaml_comments(&src);
    let platforms = code.matches("- os: ").count();
    let expected = yaml_numbers(&src, "EXPECTED_ASSETS");
    assert!(!expected.is_empty(), "没有 EXPECTED_ASSETS，附件数无从校对");
    println!("实测：平台 {platforms} 个，期望附件数 {expected:?}");
    for n in &expected {
        assert_eq!(
            *n as usize, platforms,
            "期望附件数 {n} 与矩阵里的 {platforms} 个平台不一致"
        );
    }
    // 多处写的话必须彼此一致：只改一处比两处都错更难查。
    let first = expected[0];
    for n in &expected {
        assert_eq!(
            *n, first,
            "EXPECTED_ASSETS 在不同 job 里写了不同的值：{expected:?}"
        );
    }
    assert!(platforms >= 3, "只发 {platforms} 个平台");
}

/// 跳过时要明说「不是发布失败」，而且无论发不发都要回写一条评论。
///
/// **未被自动守住的那一半：** 工作流“根本没触发”时不会有任何东西跑，
/// 也就没有任何断言能开口。这里能做的只是保证**跑了就一定留下痕迹**，
/// 于是「没有那条评论」就成为一个人可以读的信号。该限制写在 docs/RELEASING.md 里。
#[test]
fn skipping_is_reported_out_loud_and_never_reads_like_success() {
    let src = release_yml();
    assert!(
        meta::yaml_code_contains(&src, "不是「发布失败」"),
        "跳过时的措词读起来像干净的一遍"
    );
    let code = meta::strip_yaml_comments(&src);
    assert!(
        code.contains("本次没有发布任何东西"),
        "报告里没有那句明确的未完成标记"
    );
    // report job 必须 if: always()，否则跳过时连痕迹都没有。
    let report_at = code.find("回写发布报告").expect("没有回写报告那个 job");
    let tail = &code[report_at..];
    let head = &tail[..tail.len().min(400)];
    assert!(
        head.contains("if: always()"),
        "回写报告那个 job 没有 if: always()，跳过时它自己也不跑：{head}"
    );
    assert!(
        code.contains("根本没跑"),
        "报告里没告诉读者「看不到本条评论意味着什么」"
    );
}

/// 发布说明里必须写清机器验不到的部分。
/// 一份只写好消息的发布说明会让人以为一切都验过了。
#[test]
fn the_release_notes_admit_what_the_machine_cannot_check() {
    let src = release_yml();
    let code = meta::strip_yaml_comments(&src);
    assert!(code.contains("人工验收"), "发布说明里没写机器验不到的部分");
    assert!(
        code.contains("docs/PITFALLS.md"),
        "发布说明里没指向已知限制档案"
    );
    assert!(meta::exists("docs/RELEASING.md"), "没有发布流程文档");
}
