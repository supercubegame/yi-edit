//! 登记表即期望，而且要双向。
//!
//! 单向的版本（「登记的都存在」）对「新加了一个没登记的」毫无意见，
//! 而那个新加的东西就完全在扫描范围之外了。

use yi_edit_meta as meta;

fn workspace_members() -> Vec<String> {
    let toml = meta::read("Cargo.toml");
    let start = toml.find("members").expect("Cargo.toml 里没有 members");
    let open = toml[start..].find('[').expect("members 后面没有 [") + start;
    let close = toml[open..].find(']').expect("members 没闭合") + open;
    let mut out: Vec<String> = toml[open + 1..close]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    out.sort();
    out
}

fn workflow_files() -> Vec<String> {
    meta::list_dir(".github/workflows")
        .into_iter()
        .filter(|n| n.ends_with(".yml") || n.ends_with(".yaml"))
        .collect()
}

#[test]
fn workspace_members_equal_the_crate_directories() {
    let members = workspace_members();
    let mut dirs: Vec<String> = meta::list_subdirs("crates")
        .into_iter()
        .map(|d| format!("crates/{d}"))
        .collect();
    dirs.sort();
    assert_eq!(
        members, dirs,
        "工作区成员与 crates/ 下的目录不一致（左=登记 右=实际）"
    );
    assert!(members.len() >= 4, "只登记了 {} 个 crate", members.len());
}

/// 每个 crate 都得有 src/lib.rs 或 src/main.rs，否则它根本不会被编译，
//  而「没编译」与「编译通过」在闸门输出上长得一模一样。
#[test]
fn every_crate_has_an_entry_point() {
    for m in workspace_members() {
        let has_lib = meta::exists(&format!("{m}/src/lib.rs"));
        let has_main = meta::exists(&format!("{m}/src/main.rs"));
        assert!(has_lib || has_main, "{m} 既没有 lib.rs 也没有 main.rs");
        assert!(
            meta::exists(&format!("{m}/Cargo.toml")),
            "{m} 没有 Cargo.toml"
        );
    }
}

/// 工作流文件也是一张登记表：新加一条流水线而忘了给它接回写，
/// 它第一次就会红，而失败原因只存在于我读不到的 CI 日志里。
#[test]
fn workflow_files_are_registered() {
    const EXPECTED: &[&str] = &["format.yml", "verify.yml"];
    let mut actual = workflow_files();
    actual.sort();
    let mut want: Vec<String> = EXPECTED.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(actual, want, "工作流文件集合与登记不一致");
}

/// 每条工作流都要能把原始输出送到我读得到的地方。
/// 新加一条只有 CI 面板可看的流水线，红了之后我手里只剩一个比特。
#[test]
fn every_workflow_can_post_its_own_raw_log() {
    let files = workflow_files();
    assert!(files.len() >= 2, "只扫到 {} 条工作流", files.len());
    for f in files {
        let src = meta::read(&format!(".github/workflows/{f}"));
        assert!(
            meta::yaml_code_contains(&src, "scripts/raw-log.sh"),
            "{f} 没接回写通道，它红了我只能看到一个比特"
        );
        assert!(
            meta::yaml_code_contains(&src, "raw_log_outcome="),
            "{f} 没带回写脚本自己的 trace（观察者不能是被观察的那个）"
        );
    }
}

/// 格式化那条会**改代码并回推**，所以它的循环终止与诚实汇报得有断言守。
///
/// **负向那几条一律只看非注释部分。** 实测踩过：我把事故经过写进了流水线的注释，
/// 而那条负向断言正在找那个字面量 —— 于是一条已经修好的规矩被判成还坏着。
#[test]
fn the_formatter_terminates_by_content_and_admits_it_is_not_a_gate() {
    let src = meta::read(".github/workflows/format.yml");

    // 终止条件要看**内容**（零改动就不提交），不能靠提交信息里的字符串：
    // 字符串守卫改一次提交信息模板就哑，而哑掉的表现不是红。
    assert!(
        meta::yaml_code_contains(&src, "git status --porcelain"),
        "没有按内容判断有没有改动"
    );
    assert!(
        meta::yaml_code_contains(&src, "steps.after.outputs.changed"),
        "零改动时没有跳过提交，那就会每次都推一个空提交"
    );
    for banned in ["git log", "%s%b"] {
        assert!(
            !meta::yaml_code_contains(&src, banned),
            "循环终止靠上了提交信息（{banned}），那种守卫哑掉不会红"
        );
    }

    // 收敛自证：格式化之后再检查必须是 0，否则「推了一个提交」不等于「真的归零了」。
    assert!(
        meta::yaml_code_contains(&src, "fmt_diff_after"),
        "没量格式化之后还剩多少，无法区分全部处理了与有文件被跳过了"
    );
    assert!(
        meta::yaml_code_contains(&src, "fmt_diff_before"),
        "没量格式化之前有多少，本来就规范与格式化没跑就分不开"
    );

    // 零改动时的措词要明确说出「不是没跑」；拉不到 rustfmt 时绝不能静默跳过。
    assert!(
        meta::yaml_code_contains(&src, "不是「格式化没跑」"),
        "零改动时的输出读起来像干净的一遍，而那与工具链挂了一模一样"
    );
    assert!(
        meta::yaml_code_contains(&src, "拉不到 rustfmt"),
        "工具链拉不到时没有明报，那就是一条会自己变绿的跳过"
    );

    // 沉默通道：默认令牌推的提交不会触发工作流，于是那个提交未经闸门
    // 却顶着上一个提交的绿对勾。这一条必须写在报告里。
    assert!(
        meta::yaml_code_contains(&src, "未经验证"),
        "没说清回推的提交未经闸门，那是一条会自己变绿的假话"
    );
    assert!(
        meta::yaml_code_contains(&src, "它不是闸门"),
        "没说清自己不是闸门，人会拿它的绿当成验证通过"
    );

    // 它不得成为一条默默往主干推代码的流水线。
    assert!(
        !meta::yaml_code_contains(&src, "branches: [main]"),
        "格式化流水线挂到了主干推送上，它会直接往 main 推提交"
    );
}
