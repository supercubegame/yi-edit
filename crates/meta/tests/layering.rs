//! 分层的断言。两条**负向**的是承重的：
//! 光断言「新位置有了」对「旧位置还留着一份」毫无意见，
//! 而两份代码在文件树上长得一模一样，只有一份有断言在守。

use yi_edit_meta as meta;

/// 会话层搬到了这里。
const SESSION_LIB: &str = "crates/session/src/lib.rs";
/// 搬过来之前它在这里，现在不得存在。
const OLD_COPY: &str = "crates/app/src/editor.rs";

#[test]
fn the_session_layer_moved_out_and_the_old_copy_is_gone() {
    assert!(
        meta::exists(SESSION_LIB),
        "{SESSION_LIB} 不存在，搬家没搬到位"
    );
    // 这一条是承重的：只有它能区分「搬完了」与「各留一份」。
    assert!(
        !meta::exists(OLD_COPY),
        "{OLD_COPY} 还在，现在有两份会话逻辑，而只有一份有断言在守"
    );
}

/// 会话层不得碰 GUI：它要能进快闸门，而快闸门故意不编 egui。
/// 一旦它沾上 egui，这整层断言就会被请出快闸门，而那不会报错。
#[test]
fn the_session_layer_does_not_depend_on_any_gui() {
    let toml = meta::read("crates/session/Cargo.toml");
    for banned in ["egui", "eframe", "winit", "wgpu"] {
        let in_deps = toml
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .any(|l| l.contains(banned));
        assert!(
            !in_deps,
            "crates/session 依赖了 {banned}，它就进不了快闸门了"
        );
    }
    let src = meta::read(SESSION_LIB);
    for banned in ["egui", "eframe"] {
        let hits = meta::hits_in_code(&src, banned);
        assert!(
            hits.is_empty(),
            "{SESSION_LIB} 的可执行代码里出现了 {banned}：{hits:?}"
        );
    }
}

/// 快闸门必须真的跑了会话层的测试。搬出来但不接进闸门的话，
/// 那些断言与不存在完全等价 —— 而且在仓库里看起来很充实。
#[test]
fn the_fast_gate_actually_runs_the_session_tests() {
    let sh = meta::read("scripts/verify.sh");
    let in_code = sh
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .any(|l| l.contains("-p yi-edit-session"));
    assert!(
        in_code,
        "快闸门没有选上 yi-edit-session，那层断言根本不会跑"
    );
}

/// `Editor::open` 必须用文档里那个阈值常量。
/// 测试里用 `open_with_threshold(path, 0)` 只是为了不真写 64MB；如果 `open` 自己
/// 也拍了个数，那个参数就从「可测性手段」变成了悤悤改掉真实阈值的后门。
#[test]
fn open_uses_the_documented_huge_threshold() {
    let src = meta::read(SESSION_LIB);
    let hits = meta::hits_in_code(&src, "HUGE_FILE_THRESHOLD");
    assert!(
        hits.len() >= 2,
        "会话层可执行代码里只引用了 {} 处阈值常量（导入 + open 各一处）：{hits:?}",
        hits.len()
    );
    // 负向：open 里不得出现裸的字面量阈值。
    for lit in ["64 * 1024 * 1024", "67108864"] {
        let bad = meta::hits_in_code(&src, lit);
        assert!(bad.is_empty(), "会话层里硬编了阈值字面量 {lit}：{bad:?}");
    }
}
