//! 大文件基准。两个目的，而且后者更重要：
//!
//! 1. 采集实测耗时（打开、建索引、搜索、替换、保存）。第一轮只记录，不设阈值：
//!    没有实测值就写下限，那条断言只会制造假红。
//! 2. 在真实尺寸上验正确性：埋下确定数量的靶模式，搜到的数和替掉的数必须对得上。
//!    单元测试里的文件只有几十字节，真正的多块边界只有在这里才会被压到。
//!
//! 输出写成 key=value 的 bench-result.txt，给报告脚本读。

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use yi_edit_core::SearchOptions;
use yi_edit_fileio as fio;

/// 每隔多少行埋一个靶模式。
const NEEDLE_EVERY: usize = 500;
const NEEDLE: &str = "YI_TARGET_TOKEN";
const REPL: &str = "YI_REPLACED_TOKEN_LONGER";

fn mb() -> usize {
    std::env::var("YI_EDIT_BENCH_MB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64)
}

fn main() -> std::io::Result<()> {
    let target_bytes = mb() * 1024 * 1024;
    let dir = std::env::temp_dir().join(format!("yi-edit-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("big.rs");

    // ---- 生成语料 ----
    let mut planted = 0usize;
    let gen_start = Instant::now();
    let mut buf = String::with_capacity(target_bytes + 4096);
    let mut line = 0usize;
    while buf.len() < target_bytes {
        if line % NEEDLE_EVERY == 0 {
            let _ = writeln!(buf, "fn f{line}() {{ let s = \"{NEEDLE}\"; }} // planted");
            planted += 1;
        } else {
            let _ = writeln!(
                buf,
                "fn f{line}() -> usize {{ let v = vec![{line}, {}]; v.len() }}",
                line * 3
            );
        }
        line += 1;
    }
    let bytes = buf.into_bytes();
    let gen_ms = gen_start.elapsed().as_millis();

    let write_start = Instant::now();
    fio::save_atomic(&path, &bytes)?;
    let write_ms = write_start.elapsed().as_millis();

    // ---- 打开（建行索引，不整读） ----
    let index_start = Instant::now();
    let idx = fio::index_lines(&path)?;
    let index_ms = index_start.elapsed().as_millis();

    // ---- 随机窗口读（模拟滚动到文件中间） ----
    let mid_line = idx.line_count() / 2;
    let window_start = Instant::now();
    let span = idx.line_span(mid_line).expect("中间行存在");
    let win = fio::read_range(&path, span.start as u64, 64 * 1024)?;
    let window_ms = window_start.elapsed().as_millis();
    assert!(!win.is_empty(), "窗口读到了空数据，后面的耗时没意义");

    // ---- 全文搜索 ----
    let search_start = Instant::now();
    let (hits, truncated) =
        fio::find_offsets(&path, NEEDLE.as_bytes(), SearchOptions::exact(), 0)?;
    let search_ms = search_start.elapsed().as_millis();

    // ---- 就地替换（替换串比原串长，故意让文件变大） ----
    let replace_start = Instant::now();
    let replaced = fio::replace_in_place(
        &path,
        NEEDLE.as_bytes(),
        REPL.as_bytes(),
        SearchOptions::exact(),
    )?;
    let replace_ms = replace_start.elapsed().as_millis();

    let after = fio::info(&path)?;
    let (left, _) = fio::find_offsets(&path, NEEDLE.as_bytes(), SearchOptions::exact(), 0)?;

    let mut out = String::new();
    let _ = writeln!(out, "bench_mb={}", mb());
    let _ = writeln!(out, "bytes_before={}", bytes.len());
    let _ = writeln!(out, "bytes_after={}", after.len);
    let _ = writeln!(out, "lines={}", idx.line_count());
    let _ = writeln!(out, "planted={planted}");
    let _ = writeln!(out, "hits={}", hits.len());
    let _ = writeln!(out, "replaced={replaced}");
    let _ = writeln!(out, "needle_left={}", left.len());
    let _ = writeln!(out, "gen_ms={gen_ms}");
    let _ = writeln!(out, "write_ms={write_ms}");
    let _ = writeln!(out, "index_ms={index_ms}");
    let _ = writeln!(out, "window_ms={window_ms}");
    let _ = writeln!(out, "search_ms={search_ms}");
    let _ = writeln!(out, "replace_ms={replace_ms}");
    print!("{out}");
    std::fs::write(PathBuf::from("bench-result.txt"), out.as_bytes())?;

    let _ = std::fs::remove_dir_all(&dir);

    // ---- 正确性断言（这才是这个二进制的主体） ----
    let mut bad = Vec::new();
    if truncated {
        bad.push(String::from("搜索被截断了，limit=0 本该不限"));
    }
    if hits.len() != planted {
        bad.push(format!("搜到 {} 个，埋了 {planted} 个", hits.len()));
    }
    if replaced != planted {
        bad.push(format!("替掉 {replaced} 个，埋了 {planted} 个"));
    }
    if !left.is_empty() {
        bad.push(format!("替换后还剩 {} 个靶模式", left.len()));
    }
    let grown = REPL.len() - NEEDLE.len();
    let expect_after = bytes.len() + grown * planted;
    if after.len as usize != expect_after {
        bad.push(format!(
            "替换后字节数 {} 不等于预期 {expect_after}",
            after.len
        ));
    }
    if idx.line_count() < 1000 {
        bad.push(format!("只有 {} 行，这个尺寸压不到多块边界", idx.line_count()));
    }

    if bad.is_empty() {
        println!("BENCH OK planted={planted}");
        Ok(())
    } else {
        for b in &bad {
            eprintln!("BENCH FAIL: {b}");
        }
        std::process::exit(1);
    }
}
