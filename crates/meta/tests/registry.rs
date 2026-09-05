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
        assert!(meta::exists(&format!("{m}/Cargo.toml")), "{m} 没有 Cargo.toml");
    }
}

/// 工作流文件也是一张登记表：新加一条流水线而忘了给它接回写，
/// 它第一次就会红，而失败原因只存在于我读不到的 CI 日志里。
#[test]
fn workflow_files_are_registered() {
    const EXPECTED: &[&str] = &["verify.yml"];
    let mut actual: Vec<String> = meta::list_dir(".github/workflows")
        .into_iter()
        .filter(|n| n.ends_with(".yml") || n.ends_with(".yaml"))
        .collect();
    actual.sort();
    let mut want: Vec<String> = EXPECTED.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(actual, want, "工作流文件集合与登记不一致");
}
