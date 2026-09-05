//! 截图检查器。它不看文件大小——大片纯色的界面 PNG 压到十几 KB 很正常，
//! 拿字节数当下限只会把正常图判成假的（这是上一个项目真踩过的坑）。
//!
//! 四类检查，每一类都打印实测值：
//! 1. 尺寸：与请求的窗口尺寸一致（允许 HiDPI 整数倍）。
//! 2. 不同颜色数：一张真的画了高亮文本的图，颜色数不会只有几个。
//! 3. 背景占比：全屏只有背景色 = 空窗口。
//! 4. **区域检查**（`--band`）：指定的一条横带必须真的有内容。
//!
//! 2 与 3 的**判词不同**，所以不能合成一条：一个守「真的画了内容」，
//! 一个守「不是一张纯色图」。而 4 守的是第三件事：**局部**空白。
//! 实测过：底部一大块黑色留白（占全图约 15%）既不改颜色数也不改背景占比，
//! 前三条全绿。
//!
//! `--selftest` 把检查器套在合成图上：每一条都要双向（该红的红、该绿的绿）。
//! 只验一侧的话，一个永远返回「通过」的检查器也能完美交差。
#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// 不同颜色数下限。**实测值**（1280x800 软件渲染，CI）：小文件 3063、大文件 3055。
/// 取 1000，留三倍以上余量。
const MIN_DISTINCT_COLORS: usize = 1000;
/// 最常见色（背景）占比上限。**实测值**：小文件 0.9641、大文件 0.8980。
/// 故意留在 0.995：收得太紧它就会在「空白文件」这种完全正常的场景下假红。
const MAX_BG_RATIO: f64 = 0.995;

/// 一条横带的检查：`上比例:下比例:最少颜色数`。
#[derive(Debug, Clone, Copy)]
struct Band {
    top: f64,
    bottom: f64,
    min_colors: usize,
}

impl Band {
    fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 3 {
            return Err(format!("--band 要写成 上:下:最少颜色数，比如 0.85:1.0:8，得到的是 {s:?}"));
        }
        let top: f64 = parts[0].parse().map_err(|_| format!("上边界不是数：{s:?}"))?;
        let bottom: f64 = parts[1].parse().map_err(|_| format!("下边界不是数：{s:?}"))?;
        let min_colors: usize = parts[2]
            .parse()
            .map_err(|_| format!("最少颜色数不是数：{s:?}"))?;
        if !(0.0..=1.0).contains(&top) || !(0.0..=1.0).contains(&bottom) || top >= bottom {
            return Err(format!("带的边界不合法：{s:?}"));
        }
        Ok(Self {
            top,
            bottom,
            min_colors,
        })
    }

    /// 带在图上的像素行区间。至少一行高：否则带检查会在矮图上变成空断言。
    fn rows(self, height: u32) -> (u32, u32) {
        let y0 = (self.top * height as f64).floor() as u32;
        let y1 = (self.bottom * height as f64).ceil() as u32;
        let y0 = y0.min(height.saturating_sub(1));
        let y1 = y1.clamp(y0 + 1, height);
        (y0, y1)
    }
}

struct Report {
    width: u32,
    height: u32,
    distinct: usize,
    bg_ratio: f64,
    /// 每条带的实测颜色数，按传入顺序。
    band_colors: Vec<(Band, usize)>,
    problems: Vec<String>,
}

fn check(path: &Path, want: Option<(u32, u32)>, bands: &[Band]) -> Result<Report, String> {
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
        let ratio_ok =
            w > 0 && h > 0 && width % w == 0 && height % h == 0 && width / w == height / h;
        if !ratio_ok {
            problems.push(format!(
                "尺寸 {width}x{height} 不是请求的 {w}x{h} 的整数倍（HiDPI 允许整倍）"
            ));
        }
    }
    if distinct.len() < MIN_DISTINCT_COLORS {
        problems.push(format!(
            "只有 {} 种颜色（下限 {MIN_DISTINCT_COLORS}），看起来没真的画出高亮文本",
            distinct.len()
        ));
    }
    if bg_ratio > MAX_BG_RATIO {
        problems.push(format!(
            "最常见色占 {bg_ratio:.4}（上限 {MAX_BG_RATIO}），这是一张纯色图"
        ));
    }

    // 区域检查：整图指标对局部空白毫无意见，而局部空白是真 bug。
    let mut band_colors = Vec::new();
    for b in bands {
        let (y0, y1) = b.rows(height);
        let mut seen: HashSet<[u8; 4]> = HashSet::new();
        for y in y0..y1 {
            for x in 0..width {
                seen.insert(img.get_pixel(x, y).0);
            }
        }
        band_colors.push((*b, seen.len()));
        if seen.len() < b.min_colors {
            problems.push(format!(
                "带 [{:.2},{:.2}]（像素行 {y0}..{y1}）只有 {} 种颜色，下限 {}：这一条横带几乎是空的",
                b.top,
                b.bottom,
                seen.len(),
                b.min_colors
            ));
        }
    }

    Ok(Report {
        width,
        height,
        distinct: distinct.len(),
        bg_ratio,
        band_colors,
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

/// 双向自证。每一条检查都要两侧都能红。
fn selftest() -> i32 {
    let dir = std::env::temp_dir().join(format!("yi-shotcheck-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let mut bad = Vec::new();
    let (w, h) = (256u32, 128u32);
    assert!(
        (w as usize) * (h as usize) > MIN_DISTINCT_COLORS * 4,
        "合成图太小，凑不出 {MIN_DISTINCT_COLORS} 种颜色，这条自证会变成假红"
    );
    let rich = |x: u32, y: u32| -> [u8; 4] {
        [
            (x % 256) as u8,
            (y * 2 % 256) as u8,
            ((x * 3 + y * 5) % 256) as u8,
            255,
        ]
    };
    let bottom_band = Band::parse("0.85:1.0:8").expect("带语法");

    // 1. 纯色图必须判红。
    let solid = dir.join("solid.png");
    write_png(&solid, w, h, |_, _| [30, 30, 30, 255]);
    match check(&solid, Some((w, h)), &[]) {
        Ok(r) if r.problems.is_empty() => {
            bad.push(String::from("纯色图被判成了正常截图，检查器是装饰"))
        }
        Ok(r) => println!(
            "selftest solid: 已正确判红（颜色数={} 背景占比={:.4}）",
            r.distinct, r.bg_ratio
        ),
        Err(e) => bad.push(format!("纯色图连解码都没过，夹具自己坏了：{e}")),
    }

    // 2. 多色图必须判绿（包括带检查）。
    let full = dir.join("rich.png");
    write_png(&full, w, h, rich);
    match check(&full, Some((w, h)), &[bottom_band]) {
        Ok(r) if r.problems.is_empty() => println!(
            "selftest rich: 已正确判绿（颜色数={} 背景占比={:.4} 底带颜色数={}）",
            r.distinct,
            r.bg_ratio,
            r.band_colors.first().map(|(_, n)| *n).unwrap_or(0)
        ),
        Ok(r) => bad.push(format!("多色图被判红，这是一台假红工厂：{:?}", r.problems)),
        Err(e) => bad.push(format!("多色图解不开：{e}")),
    }

    // 3. 尺寸断言自证：错的期望尺寸必须判红。
    match check(&full, Some((w + 1, h)), &[]) {
        Ok(r) if r.problems.is_empty() => {
            bad.push(String::from("尺寸对不上却判绿了，--size 那条是装饰"))
        }
        Ok(_) => println!("selftest size: 错的期望尺寸已正确判红"),
        Err(e) => bad.push(format!("尺寸自证读图失败：{e}")),
    }

    // 4. **底部留白的变异体**：上方有内容、底部纯黑。
    //    先证明旧的三条检查**真的看不见它**（否则新加这条带检查未被证明有必要），
    //    再证明带检查能抓到。
    let cut = (h as f64 * 0.85) as u32;
    let padded = dir.join("bottom-blank.png");
    write_png(&padded, w, h, |x, y| {
        if y >= cut {
            [0, 0, 0, 255]
        } else {
            rich(x, y)
        }
    });
    match check(&padded, Some((w, h)), &[]) {
        Ok(r) if r.problems.is_empty() => println!(
            "selftest bottom-blank: 已确认旧的三条检查看不见底部留白（颜色数={} 背景占比={:.4}）",
            r.distinct, r.bg_ratio
        ),
        Ok(r) => bad.push(format!(
            "底部留白的图被旧检查拓到了，那么新加带检查未被证明有必要：{:?}",
            r.problems
        )),
        Err(e) => bad.push(format!("底部留白图解不开：{e}")),
    }
    match check(&padded, Some((w, h)), &[bottom_band]) {
        Ok(r) if r.problems.is_empty() => bad.push(String::from(
            "带检查没拓到底部留白，它是装饰",
        )),
        Ok(r) => println!(
            "selftest bottom-blank + band: 已正确判红（底带颜色数={}）",
            r.band_colors.first().map(|(_, n)| *n).unwrap_or(0)
        ),
        Err(e) => bad.push(format!("带检查读图失败：{e}")),
    }

    // 5. 带语法本身的错误要报，不能默默当成默认值。
    for bogus in ["1.0:0.5:8", "abc", "0.1:0.2", "0.1:2.0:8"] {
        if Band::parse(bogus).is_ok() {
            bad.push(format!("非法的带语法 {bogus:?} 被接受了"));
        }
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
        eprintln!("用法：yi-shotcheck <a.png> […] [--size WxH] [--band 上:下:颜色数]… | --selftest");
        std::process::exit(2);
    }
    let mut want: Option<(u32, u32)> = None;
    let mut bands: Vec<Band> = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--size" => {
                let v = it.next().cloned().unwrap_or_default();
                let (w, h) = v.split_once('x').unwrap_or(("0", "0"));
                want = Some((w.parse().unwrap_or(0), h.parse().unwrap_or(0)));
            }
            "--band" => {
                let v = it.next().cloned().unwrap_or_default();
                match Band::parse(&v) {
                    Ok(b) => bands.push(b),
                    Err(e) => {
                        eprintln!("SHOTCHECK FAILED: {e}");
                        std::process::exit(2);
                    }
                }
            }
            _ => files.push(PathBuf::from(a)),
        }
    }

    let mut failed = 0usize;
    for f in &files {
        match check(f, want, &bands) {
            Ok(r) => {
                let band_txt: Vec<String> = r
                    .band_colors
                    .iter()
                    .map(|(b, n)| format!("band[{:.2},{:.2}]={n}", b.top, b.bottom))
                    .collect();
                println!(
                    "{}: {}x{} distinct_colors={} bg_ratio={:.4} {}",
                    f.display(),
                    r.width,
                    r.height,
                    r.distinct,
                    r.bg_ratio,
                    band_txt.join(" ")
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
