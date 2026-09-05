//! core 必须保持纯净。这不是口头约定，是一个扫描器。

use yi_edit_meta as meta;

/// 不许出现在 crates/core 可执行代码里的东西。
/// 每一条后面都带着「为什么」，因为一条没理由的禁止项到头来会被人直接删掉。
const FORBIDDEN: &[(&str, &str)] = &[
    ("std::fs", "碰文件就没法在毫秒内跑几万步压测"),
    ("File::", "同上"),
    ("std::net", "网络让「同样输入同样输出」不成立"),
    ("std::process", "起子进程让核心没法换外壳复用"),
    ("std::env", "读环境变量会让行为随环境变，而那种差异很难复现"),
    ("SystemTime", "系统时间让结果不可重现"),
    ("Instant::now", "同上"),
    ("thread_rng", "未播种的随机让失败无法重现"),
    ("eprintln!", "向终端写东西是 I/O，而且会在 GUI 下变成静默的垃圾"),
    ("println!", "同上"),
];

fn core_sources() -> Vec<(String, String)> {
    meta::list_dir("crates/core/src")
        .into_iter()
        .filter(|n| n.ends_with(".rs"))
        .map(|n| {
            let rel = format!("crates/core/src/{n}");
            let src = meta::read(&rel);
            (rel, src)
        })
        .collect()
}

/// 剥完注释与字符串之后必须还剩真东西。
/// 不先证这一条的话，一个把源码剥成空串的 bug 会让后面每条断言免费通过。
#[test]
fn the_stripper_leaves_real_code_behind() {
    let files = core_sources();
    assert!(files.len() >= 6, "core 只扫到 {} 个文件", files.len());
    for (rel, src) in &files {
        let code = meta::strip_comments_and_strings(src);
        let visible: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        let original: String = src.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(
            code.contains("pub ") || code.contains("fn "),
            "{rel} 剥完之后看不到任何声明，扫描器自己坏了"
        );
        assert!(
            visible.len() * 10 >= original.len() * 4,
            "{rel} 剥完只剩 {}/{} 字符（不到四成），扫描器吃多了",
            visible.len(),
            original.len()
        );
    }
}

/// 双向自证：扫描器必须能区分「注释里提到」与「代码里真用了」。
/// 两侧都要能红：只验一侧的话，一个永远返回空的扫描器也能完美交差。
#[test]
fn the_stripper_tells_comments_from_real_usage() {
    let commented = "// 这里提到 std::fs 和 File::open\nlet s = \"std::fs\";\nfn f() {}\n";
    assert!(
        meta::hits_in_code(commented, "std::fs").is_empty(),
        "注释与字符串里的字面量被当成了真用法（误报）"
    );
    let real = "fn f() { let _ = std::fs::read(\"x\"); }\n";
    assert!(
        !meta::hits_in_code(real, "std::fs").is_empty(),
        "真正的用法没被拓到（漏报），这一侧不能红就等于没有扫描器"
    );
    let nested = "/* 外 /* 内 std::net */ 仍在注释 */ fn g() {}\n";
    assert!(
        meta::hits_in_code(nested, "std::net").is_empty(),
        "嵌套块注释没剥干净"
    );
}

#[test]
fn core_has_no_io_in_executable_code() {
    let mut bad = Vec::new();
    for (rel, src) in core_sources() {
        for (needle, why) in FORBIDDEN {
            for (line, text) in meta::hits_in_code(&src, needle) {
                bad.push(format!("{rel}:{line} 出现 {needle}（{why}）：{text}"));
            }
        }
    }
    assert!(bad.is_empty(), "core 不再纯净了：\n{}", bad.join("\n"));
}

/// core 的依赖集合必须是空集。零依赖是一个约束，不只是一个优点。
#[test]
fn core_declares_no_dependencies() {
    let toml = meta::read("crates/core/Cargo.toml");
    let mut in_deps = false;
    let mut found = Vec::new();
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t == "[dependencies]" || t == "[dev-dependencies]";
            continue;
        }
        if in_deps && !t.is_empty() && !t.starts_with('#') {
            found.push(t.to_string());
        }
    }
    assert!(found.is_empty(), "core 冗上依赖了：{found:?}");
}
