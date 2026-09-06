//! 带期限的义务。三件事缺一不可：
//! - 完成判据读代码，不读我的报告（靠自己声明完成的义务等于没有义务）。
//! - 到期判红。
//! - **做完了还挂着也判红**：一份挂着已完成事项的清单，没人会再读它。
//!
//! **OB-1（让 cargo fmt 变成阻断项）已履行完毕。** 它属于「从此以后都要保持」那一类，
//! 所以正确的履行动作不是删记录就完事，而是先把完成判据**转成一条常驻断言**
//! （下面那条），再从台账上删。直接删等于把看守与待办一起扔了。

use yi_edit_meta as meta;

/// 阻断那一步的名字。它就是 OB-1 当年的完成判据，现在变成了常驻断言。
const FMT_BLOCKING_STEP: &str = r#"run "format check""#;

fn verify_sh() -> String {
    meta::read("scripts/verify.sh")
}

/// 双向自证：shell 剥注释必须能分得清「注释里提到」与「代码里真用了」。
/// 只验一侧的话，一个永远返回空字串的剥离器也能完美交差。
#[test]
fn the_shell_stripper_tells_comments_from_real_code() {
    let commented = format!("# 这里提到 {FMT_BLOCKING_STEP}\necho ok\n");
    assert!(
        !meta::shell_code_contains(&commented, FMT_BLOCKING_STEP),
        "注释里的字面量被当成了真用法（误报），一条没做完的义务会被当成已完成"
    );
    let real = format!("{FMT_BLOCKING_STEP} format_check\n");
    assert!(
        meta::shell_code_contains(&real, FMT_BLOCKING_STEP),
        "真正的用法没被拓到（漏报），这一侧不能红就等于没有扫描器"
    );
    // 剥完要先自证还剩真东西：剥成空字符串的话，后面每条断言都免费通过。
    let code = meta::strip_shell_comments(&verify_sh());
    assert!(
        code.contains("cargo test") && code.contains("gate-result.txt"),
        "剥完 verify.sh 之后看不到任何真代码，剥离器自己坏了"
    );
}

/// **常驻断言（原 OB-1 的完成判据）：fmt 必须一直是阻断项。**
///
/// 履行一次就删记录、不留看守的后果是：下一个人（或下一个我）把那一步拿掉，
/// 而没有任何东西会报错 —— fmt 静默地退回成一个参考项。
#[test]
fn the_format_check_stays_blocking() {
    let code = meta::strip_shell_comments(&verify_sh());
    assert!(
        code.contains(FMT_BLOCKING_STEP),
        "闸门里没有格式阻断那一步了。它是 OB-1 的履行结果，拿掉它等于把那笔欠账静默恢复"
    );
    // 阻断必须是真的：不规范时要非零退出，而不是只打一个数字。
    assert!(
        code.contains("FORMAT CHECK FAILED"),
        "阻断步骤没有可识别的失败标记，报告里就分不出是它红的"
    );
    assert!(
        code.contains("return 1"),
        "阻断步骤不会非零退出，那它就不是阻断而是一行日志"
    );
    // 旧的上限棘轮不得与阻断共存：上限 110 在阻断存在时永远碰不到，
    // 而一条永远为真的断言比没有断言更坏：它看起来像在守。
    assert!(
        !code.contains("FMT_CEILING"),
        "阻断与上限棘轮同时存在，后者已经是一条永远为真的装饰"
    );
    // 指标里仍要带 rustfmt 版本：否则上游换格式规则与我敲歪了代码分不出。
    assert!(
        code.contains("rustfmt_version="),
        "指标里没有 rustfmt 版本，阻断红了时分不出是谁的错"
    );
}

/// 做完了就不能还挂在台账上。一份挂着已完成事项的清单，没人会再读它。
#[test]
fn a_finished_obligation_is_no_longer_listed() {
    let doc = meta::read("docs/OBLIGATIONS.md");
    for (title, _) in meta::md_sections(&doc) {
        assert!(
            !title.starts_with("OB-1 "),
            "OB-1 已经做完了（fmt 已阻断）却还挂在清单上，删掉它"
        );
    }
}

/// 台账里仍在挂着的义务，必须带 unix 时间戳与完成判据。
///
/// 为什么现在没有义务也要留这条：下一次有人新增一条 OB-N 而只写个日期字符串时，
/// 那个期限就没有任何东西能自动判到期 —— 而「没人判到期」与「没过期」长得一模一样。
#[test]
fn any_listed_obligation_carries_a_machine_readable_deadline() {
    let doc = meta::read("docs/OBLIGATIONS.md");
    for (title, body) in meta::md_sections(&doc) {
        if !title.starts_with("OB-") {
            continue;
        }
        assert!(
            body.contains("unix "),
            "{title} 没带 unix 时间戳，到期就没有任何东西能自动判红"
        );
        assert!(
            body.contains("完成判据"),
            "{title} 没写完成判据，于是只能靠我自己声明完成"
        );
    }
}

/// 没有期限的那些事必须写成「有理由的记录」，而不是假装有截止日。
/// 期限由别人决定的事登记成义务，只会逐人把日期一次次往后挑。
#[test]
fn the_reasoned_records_section_exists_and_is_not_empty() {
    let doc = meta::read("docs/OBLIGATIONS.md");
    let sections = meta::md_sections(&doc);
    let sec = sections
        .iter()
        .find(|(t, _)| t.contains("不登记成义务"))
        .expect("没有「不登记成义务的事」那一节");
    assert!(
        sec.1.trim().len() > 80,
        "那一节几乎是空的，那么无期限的事会全部流回「以后再说」"
    );
}

/// **一份跟不上被它描述的东西的记录，比没有记录更坏。**
/// 字体层现在真的接受字体集合，所以一条写着「只接受 .ttf」的记录就是一句谎。
#[test]
fn the_records_do_not_describe_limitations_the_code_already_lifted() {
    let fontpick = meta::read("crates/session/src/fontpick.rs");
    let accepts_collections = fontpick.contains(".ttc") && fontpick.contains("ttcf");
    assert!(
        accepts_collections,
        "字体层不再处理字体集合了，这条断言得重写"
    );
    let doc = meta::read("docs/OBLIGATIONS.md");
    let sections = meta::md_sections(&doc);
    let reasoned = sections
        .iter()
        .find(|(t, _)| t.contains("不登记成义务"))
        .map(|(_, body)| body.clone())
        .unwrap_or_default();
    for stale in ["只接受 .ttf", "当前只接受"] {
        assert!(
            !reasoned.contains(stale),
            "记录里还写着「{stale}」，而代码已经不是那样了"
        );
    }
    let pitfalls = meta::read("docs/PITFALLS.md");
    assert!(
        !pitfalls.contains("而当前只接受 .ttf/.otf"),
        "PITFALLS 里还留着那条已经解决的限制"
    );
}
