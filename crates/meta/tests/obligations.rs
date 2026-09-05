//! 带期限的义务。三件事缺一不可：
//! - 完成判据读代码，不读我的报告（靠自己声明完成的义务等于没有义务）。
//! - 到期判红。
//! - **做完了还挂着也判红**：一份挂着已完成事项的清单，没人会再读它。

use std::time::{SystemTime, UNIX_EPOCH};

use yi_edit_meta as meta;

/// OB-1 的期限。与文档里的日期耦合：两头必须同时改，下面有等号断言。
const OB1_DEADLINE: u64 = 1_790_380_800; // 2026-09-26T00:00:00Z
const OB1_DATE: &str = "2026-09-26";
const OB1_ID: &str = "OB-1";
/// 完成判据：fmt 检查变成阻断项（进了 run 列表）。
const OB1_DONE_MARKER: &str = r#"run "format check""#;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间在 1970 之前")
        .as_secs()
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
    let done = meta::read("scripts/verify.sh").contains(OB1_DONE_MARKER);

    if done {
        assert!(
            !listed,
            "{OB1_ID} 已经做完了（verify.sh 里 fmt 已阻断）却还挂在清单上，删掉它"
        );
        return;
    }

    assert!(listed, "{OB1_ID} 没做完却从清单上消失了，这就是「以后再说」");
    let now = now();
    assert!(
        now <= OB1_DEADLINE,
        "{OB1_ID} 已过期 {} 天。只有两个正确反应：真的做完，或确认做不到并挑进「测不出来」。把日期往后挑不是其中任何一个。",
        (now - OB1_DEADLINE) / 86_400
    );
    let left = (OB1_DEADLINE - now) / 86_400;
    println!("{OB1_ID} 还有 {left} 天（截止 {OB1_DATE}）");
}

/// 没有期限的那些事必须写成「有理由的记录」，而不是假装有截止日。
/// 期限由别人决定的事登记成义务，只会逼人把日期一次次往后挑。
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
