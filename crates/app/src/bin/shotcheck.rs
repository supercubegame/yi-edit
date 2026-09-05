//! 截图检查器。它不看文件大小——大片纯色的界面 PNG 压到十几 KB 很正常，
//! 拿字节数当下限只会把正常图判成假的（这是上一个项目真踩过的坑）。
//!
//! 它判三件事，每一件都打印实测值，下一轮拿实测值收紧：
//! 1. 尺寸：与请求的窗口尺寸一致（允许 HiDPI 整数倍）。
//! 2. 不同颜色数：一张真的画了高亮文本的图，颜色数不会只有几个。
//! 3. 背景占比：全屏只有背景色 = 空窗口，而空窗口与正常窗口在文件树上长得一模一样。
//!
//! `--selftest` 把检查器套在两种合成图上：纯色图必须判红，多色图必须判绿。
//! 只验一侧的话，一个永远返回「通过」的检查器也能完美交差。
#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// 不同颜色数下限。**这个数字是拍的**（首轮没有实测值），只用来抓「空窗口 / 纯色图」。
/// 首轮 CI 跑完拿到实测值之后要回来收紧，并在报告里带上实测值。
const MIN_DISTINCT_COLORS: usize = 12;
/// 背景色（最常见色）占比上限。同样是拍的，同样只用来抓空窗口。
const MAX_BG_RATIO: f64 = 0.995;

struct Report {
    width: u32,
    height: u32,
    distinct: usize,
    bg_ratio: f64,
    problems: Vec<String>,
}

fn check(path: &Path, want: Option<(u32, u32)>) -> Result<Report, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读不到 {}：{e}", path.display()))?;
    if bytes.len() < 8 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(format!("{} 不是 PNG（头八字节不对）", path.display()));
    }
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("{} 解不开：{e}", path.display()))?
        .to_rgba8();
    let (width, height) = (img.width(), img.height());

    let mut counts: std::collections::HashMap<[u8; 4], usize> = std::collections::HashMap::new();
    let mut distinct: HashSet<[u8; 4]> = HashSet::new();
    for px in img.pixels() {
        let k = px.0;
        distinct.insert(k);
        *counts.entry(k).or_insert(0) += 1;
    }
    let total = (width as usize) * (height as usize);
    let bg = counts.values().copied().max().unwrap_or(0);
    let bg_ratio = if total == 0 {
        1.0
    } else {
        bg as f64 / total as f64
    };

    let mut problems = Vec::new();
    if width == 0 || height == 0 {
        problems.push(String::from("尺寸为零"));
    }
    if let Some((w, h)) = want {
        let ratio_ok = width % w == 0 && height % h == 0 && width / w == height / h;
        if !ratio_ok {
            problems.push(format!(
                "尺寸 {width}x{height} 不是请求的 {w}x{h} 的整数倍（HiDPI 允许整倍）"
            ));
        }
    }
    if distinct.len() < MIN_DISTINCT_COLORS {
        problems.push(format!(
            "只有 {} 种颜色（下限 {MIN_DISTINCT_COLORS}），看起来是空窗口",
            distinct.len()
        ));
    }
    if bg_ratio > MAX_BG_RATIO {
        problems.push(format!(
            "最常见色占 {:.4}（上限 {MAX_BG_RATIO}），屏上几乎没内容",
            bg_ratio
        ));
    }
    Ok(Report {
        width,
        height,
        distinct: distinct.len(),
        bg_ratio,
        problems,
    })
}

fn write_png(path: &Path, w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) {
    let mut raw = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            raw.extend_from_slice(&f(x, y));
        }
    }
    image::save_buffer(path, &raw, w, h, image::ColorType::Rgba8).expect("写合成 PNG");
}

/// 双向自证：纯色图必须判红，多色图必须判绿。
fn selftest() -> i32 {
    let dir = std::env::temp_dir().join(format!("yi-shotcheck-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let mut bad = Vec::new();

    let solid = dir.join("solid.png");
    write_png(&solid, 64, 48, |_, _| [30, 30, 30, 255]);
    match check(&solid, Some((64, 48))) {
        Ok(r) if r.problems.is_empty() => {
            bad.push(String::from("纯色图被判成了正常截图，检查器是装饰"))
        }
        Ok(r) => println!(
            "selftest solid: 已正确判红（{} 条问题，颜色数={}）",
            r.problems.len(),
            r.distinct
        ),
        Err(e) => bad.push(format!("纯色图连解码都没过，夹具自己坏了：{e}")),
    }

    let rich = dir.join("rich.png");
    write_png(&rich, 64, 48, |x, y| {
        [(x * 4 % 256) as u8, (y * 5 % 256) as u8, ((x + y) % 256) as u8, 255]
    });
    match check(&rich, Some((64, 48))) {
        Ok(r) if r.problems.is_empty() => println!(
            "selftest rich: 已正确判绿（颜色数={} 背景占比={:.4}）",
            r.distinct, r.bg_ratio
        ),
        Ok(r) => bad.push(format!("多色图被判红，这是一台假红工厂：{:?}", r.problems)),
        Err(e) => bad.push(format!("多色图解不开：{e}")),
    }

    let _ = std::fs::remove_dir_all(&dir);
    if bad.is_empty() {
        println!("SHOTCHECK SELFTEST OK");
        0
    } else {
        for b in &bad {
            eprintln!("SHOTCHECK SELFTEST FAIL: {b}");
        }
        1
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--selftest") {
        std::process::exit(selftest());
    }
    if args.is_empty() {
        eprintln!("用法：yi-shotcheck <a.png> […] [--size WxH] | --selftest");
        std::process::exit(2);
    }
    let mut want: Option<(u32, u32)> = None;
    let mut files: Vec<PathBuf> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--size" {
            let v = it.next().cloned().unwrap_or_default();
            let (w, h) = v.split_once('x').unwrap_or(("0", "0"));
            want = Some((w.parse().unwrap_or(0), h.parse().unwrap_or(0)));
        } else {
            files.push(PathBuf::from(a));
        }
    }

    let mut failed = 0usize;
    for f in &files {
        match check(f, want) {
            Ok(r) => {
                println!(
                    "{}: {}x{} distinct_colors={} bg_ratio={:.4}",
                    f.display(),
                    r.width,
                    r.height,
                    r.distinct,
                    r.bg_ratio
                );
                for p in &r.problems {
                    eprintln!("  FAIL {}：{p}", f.display());
                }
                if !r.problems.is_empty() {
                    failed += 1;
                }
            }
            Err(e) => {
                eprintln!("  FAIL {e}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        eprintln!("SHOTCHECK FAILED {failed}/{}", files.len());
        std::process::exit(1);
    }
    println!("SHOTCHECK OK {}/{}", files.len(), files.len());
}
