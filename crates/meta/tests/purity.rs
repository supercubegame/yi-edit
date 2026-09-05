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

/// 聚合存留率的下限（百分之）。
/// **为什么是聚合而不是按文件：** 按文件判的第一版就是一台假红工厂 ——
/// consts.rs 本来就是注释占大头（那是故意的，耦合参数必须解释清楚），
/// 于是一个完全正常的文件会把闸门弄红，而假红会逼人去改产品迎合尺子。
/// 这个数字是**拍的**，但断言会把实测值打印出来，下一轮拿实测值收紧。
const MIN_AGGREGATE_RETENTION_PCT: usize = 25;

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

fn dense(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

/// 剥完注释与字符串之后必须还剩真东西。
/// 不先证这一条的话，一个把源码剥成空串的 bug 会让后面每条断言免费通过。
#[test]
fn the_stripper_leaves_real_code_behind() {
    let files = core_sources();
    assert!(files.len() >= 6, "core 只扫到 {} 个文件", files.len());
    let mut total_before = 0usize;
    let mut total_after = 0usize;
    for (rel, src) in &files {
        let code = meta::strip_comments_and_strings(src);
        assert!(
            code.contains("pub ") || code.contains("fn "),
            "{rel} 剥完之后看不到任何声明，扫描器自己坏了"
        );
        let (before, after) = (dense(src), dense(&code));
        println!(
            "{rel}: 剥完剩 {after}/{before} 字符（{}%）",
            after * 100 / before.max(1)
        );
        total_before += before;
        total_after += after;
    }
    let pct = total_after * 100 / total_before.max(1);
    println!("core 聚合存留率实测值：{pct}%（下限 {MIN_AGGREGATE_RETENTION_PCT}%）");
    assert!(
        pct >= MIN_AGGREGATE_RETENTION_PCT,
        "core 剥完只剩 {pct}%，扫描器吃多了"
    );
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
