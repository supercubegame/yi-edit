//! 耦合参数的等号断言。
//!
//! 写在散文里的那句「改一个必须重算另一个」自己会腐化：前提变了，数字还留在原地。
//! 所以除了写进文档，还要有一条等号把两头钉在一起。

use yi_edit_core::{CHUNK_OVERLAP, CHUNK_SIZE, HUGE_FILE_THRESHOLD, MAX_PATTERN_LEN, MAX_UNDO};
use yi_edit_meta as meta;

#[test]
fn chunk_overlap_covers_the_longest_pattern() {
    // 少一个字节，横跨块边界的匹配就会静默丢失。
    assert_eq!(CHUNK_OVERLAP, MAX_PATTERN_LEN - 1);
}

#[test]
fn huge_threshold_is_a_whole_number_of_chunks() {
    assert_eq!(HUGE_FILE_THRESHOLD % CHUNK_SIZE as u64, 0);
}

#[test]
fn chunk_is_big_enough_that_overlap_is_not_the_dominant_cost() {
    assert!(
        CHUNK_SIZE >= CHUNK_OVERLAP * 16,
        "CHUNK_SIZE={CHUNK_SIZE} CHUNK_OVERLAP={CHUNK_OVERLAP}：重叠占比太大"
    );
}

#[test]
fn undo_limit_is_large_enough_for_the_fuzz_fixture() {
    // crates/core/tests/edit.rs 里的夹具跑 400 步；上限小于它的话，
    // 那个测试测到的是丢弃策略而不是撤销正确性，而两者失败时长得一样。
    assert!(MAX_UNDO > 400, "MAX_UNDO={MAX_UNDO}");
}

/// 文档里必须逐字写着这几条等式。文档与代码分岔时，这一条先红。
#[test]
fn the_coupled_parameters_are_documented_verbatim() {
    let agents = meta::read("AGENTS.md");
    for eq in [
        "CHUNK_OVERLAP == MAX_PATTERN_LEN - 1",
        "HUGE_FILE_THRESHOLD % CHUNK_SIZE == 0",
        "CHUNK_SIZE >= CHUNK_OVERLAP * 16",
    ] {
        assert!(agents.contains(eq), "AGENTS.md 里没有逐字写着：{eq}");
    }
}
