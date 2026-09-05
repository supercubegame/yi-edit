//! 字体那条通道的结构断言。
//!
//! 它是典型的「坏了不会喊」：截图只在 Linux 上拍，而那台机器自带中日韩 fallback，
//! 于是三张截图、四条带、颜色数、背景占比全部绿的同时，Windows 上整个界面可以
//! 是豆腐块。所以这里守两件事：探测真的三个平台都跑，且它红了真的能让 job 红。

use yi_edit_meta as meta;

fn compiled_ui() -> String {
    let main = meta::read("crates/app/src/main.rs");
    let path = main
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("#[path = \"")
                .and_then(|s| s.strip_suffix("\"]"))
        })
        .unwrap_or("ui.rs");
    meta::read(&format!("crates/app/src/{path}"))
}

/// 取出工作流里一个 step 的正文（从 `- name: X` 到下一个 `- name:`）。
/// 「某段里有没有 X」必须先把那段切出来再找 —— 全文搜的话，别的 step 里的
/// `matrix.probe` 会让这条断言误报，而误报会逐人去改产品迎合尺子。
fn step_body(wf: &str, name: &str) -> String {
    let head = format!("- name: {name}");
    let start = wf
        .find(&head)
        .unwrap_or_else(|| panic!("工作流里没有这一步：{name}"));
    let rest = &wf[start + head.len()..];
    let end = rest.find("- name:").unwrap_or(rest.len());
    rest[..end].to_string()
}

/// 候选表只能有一份真身，而它在有断言盘的那一层。UI 层再写一张的话，
/// 两张表会各自长歪，而只有一张跑得到快闸门。
#[test]
fn the_gui_gets_its_font_through_the_audited_layer() {
    let src = compiled_ui();
    assert!(
        src.contains("fontpick::pick") && src.contains("fontpick::REQUIRED"),
        "compiled UI 没有走 fontpick，字体又变成「读到就用」了"
    );
    // 集合（.ttc）里第 0 张脸未必覆盖中文：不传 index 就等于没挑。
    let index_hits = meta::hits_in_code(&src, "index");
    assert!(
        index_hits.iter().any(|(_, l)| l.contains("data.index")),
        "compiled UI 没有把挑中的 face index 交给渲染层：{index_hits:?}"
    );
    for bad in [
        "C:\\Windows\\Fonts",
        "/usr/share/fonts",
        "/System/Library/Fonts",
    ] {
        assert!(
            !src.contains(bad),
            "compiled UI 里又出现了字体路径候选表（{bad}），现在有两份真身"
        );
    }
}

/// 覆盖必须是真的查 cmap，不能靠体积。老检查就是只看体积，而它对一份
/// 只有拉丁的字体毫无意见。对照实验在 crates/session/tests/fontpick.rs 里。
#[test]
fn coverage_is_decided_by_a_cmap_lookup_and_the_control_experiment_exists() {
    let src = meta::read("crates/session/src/fontpick.rs");
    for needle in ["cmap", "lookup_format4", "lookup_format12", "face_offset"] {
        assert!(!meta::hits_in_code(&src, needle).is_empty(), "字体层里没有 {needle}");
    }
    let tests = meta::read("crates/session/tests/fontpick.rs");
    assert!(
        tests.contains("the_old_size_only_check_would_have_accepted_a_latin_only_font"),
        "对照实验没了：那条是唯一能证明老检查真的看不见豆腐块的断言"
    );
    assert!(
        tests.contains("a_collection_picks_the_face_that_covers_cjk_not_just_face_zero"),
        ".ttc 选脸那条断言没了"
    );
}

/// 探测必须三个平台都跑。只在拍截图的那台跑的话，它刚好只守住了
/// 本来就不会出错的那一台 —— 那是一条永远为真的断言。
#[test]
fn the_font_probe_runs_on_every_platform_not_only_the_screenshot_one() {
    let wf = meta::read(".github/workflows/verify.yml");
    let step = step_body(&wf, "Font probe");
    assert!(step.contains("yi-fontprobe"), "探测那一步没调探测程序");
    assert!(
        !step.contains("matrix.probe"),
        "探测挂上了 matrix.probe，于是只在本来就正常的那台机器上跑"
    );
    assert!(
        step.contains(".exe"),
        "没处理 Windows 上的 .exe 后缀，那台机器上它会直接找不到文件"
    );

    // 红了要真的能让 job 红，而且不能被包在 probe 那个 if 里。
    let verdict = step_body(&wf, "Evaluate steps");
    let check = verdict
        .find("steps.fontprobe.outcome\" ] || note")
        .or_else(|| verdict.find("steps.fontprobe.outcome }}\" = success ] || note"))
        .expect("判定里没有把字体探测算进去，它红了也不会让 job 红");
    let probe_only = verdict
        .find("matrix.probe }}\" = \"true\" ]")
        .expect("判定里没有 probe 专属分支了，这条断言得重写");
    assert!(
        check < probe_only,
        "字体探测的判定被放进了 probe 专属分支，只对一台机器生效"
    );
    assert!(
        verdict.contains("fontprobe=${{ steps.fontprobe.outcome }}"),
        "outcomes.txt 里没写探测结果，报告里就看不到它跑没跑"
    );

    // 输出要能到我读得到的地方，否则红了也只剩一个比特。
    assert!(
        wf.matches("font-probe.txt").count() >= 3,
        "font-probe.txt 没有同时进两条 raw-log 与产物，失败详情传不回来"
    );
}

#[test]
fn the_probe_binary_is_registered_and_needs_no_display() {
    let toml = meta::read("crates/app/Cargo.toml");
    assert!(toml.contains("yi-fontprobe"), "Cargo.toml 里没登记探测程序");
    assert!(meta::exists("crates/app/src/bin/fontprobe.rs"));
    let src = meta::read("crates/app/src/bin/fontprobe.rs");
    // 一碰 eframe/egui 它就需要窗口，而 macOS/Windows runner 上没有显示器 ——
    // 那会把一条能跑的断言变成一条永远红的断言，而人会学会忽略它。
    for banned in ["eframe", "egui"] {
        assert!(
            meta::hits_in_code(&src, banned).is_empty(),
            "探测程序碰了 {banned}，它就需要窗口了"
        );
    }
    // 拿不到字体时必须非零退出：静默跳过与「字体正常」在面板上一模一样。
    assert!(
        meta::hits_in_code(&src, "exit(1)")
            .iter()
            .any(|(_, l)| l.contains("process::exit(1)")),
        "探测拉不到字体时没有非零退出"
    );
}
