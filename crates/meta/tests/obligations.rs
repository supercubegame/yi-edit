//! 带期限的义务。三件事缺一不可：
//! - 完成判据读代码，不读我的报告（靠自己声明完成的义务等于没有义务）。
//! - 到期判红。
//! - **做完了还挂着也判红**：一份挂着已完成事项的清单，没人会再读它。
//!
//! 而「读代码」自己也有一个坑，已经踩了：判据是一个字串，而我在 verify.sh 的
//! **注释**里解释了一句那个判据是什么 —— 于是一条根本没做完的义务被认定为已完成，
//! 而那条断言会反过来逐人把它从清单上删掉。“某段里有没有 X”必须先把那段切出来。

use std::time::{SystemTime, UNIX_EPOCH};

use yi_edit_meta as meta;

/// OB-1 的期限。与文档里的日期耦合：两头必须同时改，下面有等号断言。
const OB1_DEADLINE: u64 = 1_790_380_800; // 2026-09-26T00:00:00Z
const OB1_DATE: &str = "2026-09-26";
const OB1_ID: &str = "OB-1";
/// 完成判据：fmt 检查变成阻断项（进了 run 列表）。
const OB1_DONE_MARKER: &str = r#"run "format check""#;
/// 棘轮那一步。它**不是**完成判据：守的是「不得再变差」而不是「已经归零」。
const FMT_RATCHET_MARKER: &str = r#"run "fmt ceiling""#;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间在 1970 之前")
        .as_secs()
}

fn verify_sh() -> String {
    meta::read("scripts/verify.sh")
}

/// 从 verify.sh 的可执行代码里读一个 `NAME=数字` 常量。
fn shell_number(name: &str) -> u64 {
    let code = meta::strip_shell_comments(&verify_sh());
    let needle = format!("{name}=");
    let line = code
        .lines()
        .find(|l| l.trim_start().starts_with(&needle))
        .unwrap_or_else(|| panic!("verify.sh 的可执行代码里没有 {name}"));
    let rhs = line.split('=').nth(1).unwrap_or_default();
    let digits: String = rhs.chars().filter(|c| c.is_ascii_digit()).collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("{name} 的值解不出来：{rhs}"))
}

/// 双向自证：shell 剥注释必须能分得清「注释里提到」与「代码里真用了」。
/// 只验一侧的话，一个永远返回空字串的剥离器也能完美交差（而那会让每条判据都变成「未完成」）。
#[test]
fn the_shell_stripper_tells_comments_from_real_code() {
    let commented = format!("# 这里提到 {OB1_DONE_MARKER}\necho ok\n");
    assert!(
        !meta::shell_code_contains(&commented, OB1_DONE_MARKER),
        "注释里的字面量被当成了真用法（误报），一条没做完的义务会被当成已完成"
    );
    let real = format!("{OB1_DONE_MARKER} cargo fmt --all -- --check\n");
    assert!(
        meta::shell_code_contains(&real, OB1_DONE_MARKER),
        "真正的用法没被拓到（漏报），这一侧不能红就等于没有扫描器"
    );
    // 剥完要先自证还剩真东西：剥成空字符串的话，后面每条断言都免费通过。
    let code = meta::strip_shell_comments(&verify_sh());
    assert!(
        code.contains("cargo test") && code.contains("gate-result.txt"),
        "剥完 verify.sh 之后看不到任何真代码，剥离器自己坏了"
    );
}

#[test]
fn the_deadline_in_code_and_in_the_doc_are_the_same() {
    let doc = meta::read("docs/OBLIGATIONS.md");
    assert!(doc.contains(OB1_DATE), "文档里的日期不是 {OB1_DATE}");
    assert!(
        doc.contains(&OB1_DEADLINE.to_string()),
        "文档里没写 unix {OB1_DEADLINE}，两头已经可以各自漂了"
    );
}

#[test]
fn ob1_is_either_pending_within_grace_or_done_and_removed() {
    let doc = meta::read("docs/OBLIGATIONS.md");
    let listed = doc.contains(OB1_ID);
    // 先剥注释再找：否则一句解释文字就能把义务判成已完成。
    let done = meta::shell_code_contains(&verify_sh(), OB1_DONE_MARKER);

    if done {
        assert!(
            !listed,
            "{OB1_ID} 已经做完了（verify.sh 里 fmt 已阶断）却还挂在清单上，删掉它"
        );
        return;
    }

    assert!(
        listed,
        "{OB1_ID} 没做完却从清单上消失了，这就是「以后再说」"
    );
    let now = now();
    assert!(
        now <= OB1_DEADLINE,
        "{OB1_ID} 已过期 {} 天。只有两个正确反应：真的做完，或确认做不到并挑进「测不出来」。把日期往后挑不是其中任何一个。",
        (now - OB1_DEADLINE) / 86_400
    );
    let left = (OB1_DEADLINE - now) / 86_400;
    println!("{OB1_ID} 还有 {left} 天（截止 {OB1_DATE}）");
}

/// 棘轮必须真的在闸门里，而且与义务的完成判据不是同一个东西。
///
/// 两者混为一谈的后果很具体：添一个上限检查就让 OB-1 看起来做完了，
/// 于是那条真正的欠账被从清单上删掉，而 fmt 从未阶断。
#[test]
fn the_fmt_ratchet_exists_and_is_not_mistaken_for_the_obligation() {
    assert_ne!(
        FMT_RATCHET_MARKER, OB1_DONE_MARKER,
        "棘轮与完成判据用了同一个名字，OB-1 会被误判为已完成"
    );
    let code = meta::strip_shell_comments(&verify_sh());
    assert!(
        code.contains(FMT_RATCHET_MARKER),
        "闸门里没有 fmt 上限那一步，欠账又变成一句只写在文档里的话"
    );
    // 上限的比较必须真的存在，而不是只打一行数字。
    assert!(
        code.contains("-gt \"${FMT_CEILING}\""),
        "那一步没有拿实测值与上限做比较，它只是在汇报数字"
    );
    // 报告里要同时带 rustfmt 版本：否则「我新敲了不规范的代码」与
    // 「上游换了格式规则」在一个孤零的数字上长得一模一样。
    assert!(
        code.contains("rustfmt_version="),
        "指标里没有 rustfmt 版本，上限假红时分不出是谁的错"
    );
}

/// 上限写在两处（闸门与台账），只改一头必须红。实测值也要在台账里，
/// 否则下一轮无从得知这个上限离真实值有多远，而那个拍的数字就会一直躺在那里。
#[test]
fn the_fmt_ceiling_in_the_gate_and_in_the_ledger_are_the_same() {
    let ceiling = shell_number("FMT_CEILING");
    let doc = meta::read("docs/OBLIGATIONS.md");
    assert!(
        doc.contains(&format!("上限 {ceiling}")),
        "台账里没有逐字写着「上限 {ceiling}」，两头已经可以各自漂了"
    );
    assert!(
        doc.contains("fmt_diff_lines="),
        "台账里没有实测值，那个上限就是一个无从收紧的拍数"
    );
    // 上限不得大到它什么都拦不住。这一条守的是「有人嵌不住了就把它调到天上去」。
    assert!(
        ceiling <= 200,
        "fmt 上限 {ceiling} 已经大到不会拦住任何东西，那就变成了一行装饰"
    );
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
