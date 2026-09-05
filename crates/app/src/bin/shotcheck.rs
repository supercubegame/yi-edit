//! 截图检查器。它不看文件大小——大片纯色的界面 PNG 压到十几 KB 很正常，
//! 拿字节数当下限只会把正常图判成假的（这是上一个项目真蹩过的坑）。
//!
//! 五类检查，每一类都打印实测值：
//! 1. 尺寸：与请求的窗口尺寸一致（允许 HiDPI 整数倍）。
//! 2. 不同颜色数：一张真的画了高亮文本的图，颜色数不会只有几个。
//! 3. 背景占比：全屏只有背景色 = 空窗口。
//! 4. **横带检查**（`--band`）：指定的一条横带必须真的有内容。
//! 5. **竖带检查**（`--vband`）：指定的一条竖带必须真的有内容。
//!
//! 2 与 3 的**判词不同**，所以不能合成一条：一个守「真的画了内容」，
//! 一个守「不是一张纯色图」。而 4 守的是第三件事：**局部**空白。
//! 实测过：底部一大块黑色留白（占全图约 15%）既不改颜色数也不改背景占比，
//! 前三条全绿。
//!
//! 5 是因为横带在另一个方向上有同样的盲区：右侧跳转面板整条没画的话，
//! 每一条横带里仍然有大量正文颜色，于是四条横带全绿而面板根本不存在。
//! 自测里有一个向右置空的变异体先证明横带真的看不见它。
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

/// 带的方向。两个方向各自有盲区，所以不能只留一个。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    /// 横带：按高度比例切，守工具栏 / 正文 / 状态栏这种上下分层。
    Horizontal,
    /// 竖带：按宽度比例切，守侧栏 / 正文 / 跳转面板这种左右分层。
    Vertical,
}

impl Axis {
    fn label(self) -> &'static str {
        match self {
            Axis::Horizontal => "band",
            Axis::Vertical => "vband",
        }
    }
}

/// 一条带的检查：`起比例:止比例:最少颜色数`。
#[derive(Debug, Clone, Copy)]
struct Band {
    axis: Axis,
    from: f64,
    to: f64,
    min_colors: usize,
}

impl Band {
    fn parse(axis: Axis, s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 3 {
            return Err(format!(
                "--{} 要写成 起:止:最少颜色数，比如 0.85:1.0:8，得到的是 {s:?}",
                axis.label()
            ));
        }
        let from: f64 = parts[0].parse().map_err(|_| format!("起边界不是数：{s:?}"))?;
        let to: f64 = parts[1].parse().map_err(|_| format!("止边界不是数：{s:?}"))?;
        let min_colors: usize = parts[2]
            .parse()
            .map_err(|_| format!("最少颜色数不是数：{s:?}"))?;
        if !(0.0..=1.0).contains(&from) || !(0.0..=1.0).contains(&to) || from >= to {
            return Err(format!("带的边界不合法：{s:?}"));
        }
        Ok(Self {
            axis,
            from,
            to,
            min_colors,
        })
    }

    /// 带在那个轴上的像素区间。至少一行 / 一列宽：否则带检查会在小图上变成空断言。
    fn span(self, extent: u32) -> (u32, u32) {
        let a = (self.from * extent as f64).floor() as u32;
        let b = (self.to * extent as f64).ceil() as u32;
        let a = a.min(extent.saturating_sub(1));
        let b = b.clamp(a + 1, extent);
        (a, b)
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
        let (a, z) = match b.axis {
            Axis::Horizontal => b.span(height),
            Axis::Vertical => b.span(width),
        };
        let mut seen: HashSet<[u8; 4]> = HashSet::new();
        match b.axis {
            Axis::Horizontal => {
                for y in a..z {
                    for x in 0..width {
                        seen.insert(img.get_pixel(x, y).0);
                    }
                }
            }
            Axis::Vertical => {
                for x in a..z {
                    for y in 0..height {
                        seen.insert(img.get_pixel(x, y).0);
                    }
                }
            }
        }
        band_colors.push((*b, seen.len()));
        if seen.len() < b.min_colors {
            let what = match b.axis {
                Axis::Horizontal => "横带",
                Axis::Vertical => "竖带",
            };
            problems.push(format!(
                "{what} [{:.2},{:.2}]（像素 {a}..{z}）只有 {} 种颜色，下限 {}：这一条{what}几乎是空的",
                b.from,
                b.to,
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
    let bottom_band = Band::parse(Axis::Horizontal, "0.85:1.0:8").expect("带语法");
    let right_band = Band::parse(Axis::Vertical, "0.92:1.0:8").expect("竖带语法");
    let four_bands = [
        Band::parse(Axis::Horizontal, "0.00:0.05:8").expect("带语法"),
        Band::parse(Axis::Horizontal, "0.40:0.60:8").expect("带语法"),
        Band::parse(Axis::Horizontal, "0.88:0.96:8").expect("带语法"),
        Band::parse(Axis::Horizontal, "0.97:1.00:8").expect("带语法"),
    ];

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

    // 2. 多色图必须判绿（包括两个方向的带检查）。
    let full = dir.join("rich.png");
    write_png(&full, w, h, rich);
    match check(&full, Some((w, h)), &[bottom_band, right_band]) {
        Ok(r) if r.problems.is_empty() => println!(
            "selftest rich: 已正确判绿（颜色数={} 背景占比={:.4} 底带={} 右带={}）",
            r.distinct,
            r.bg_ratio,
            r.band_colors.first().map(|(_, n)| *n).unwrap_or(0),
            r.band_colors.get(1).map(|(_, n)| *n).unwrap_or(0)
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
    //    再证明带检查能拓到。
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

    // 5. **右侧置空的变异体**：右边一条纯色，其余有内容。跳转面板根本没画
    //    就是这个形状。先证明**四条横带加整图指标全部看不见它**，再证明竖带能拓到。
    let right_cut = (w as f64 * 0.92) as u32;
    let blank_right = dir.join("right-blank.png");
    write_png(&blank_right, w, h, |x, y| {
        if x >= right_cut {
            [0, 0, 0, 255]
        } else {
            rich(x, y)
        }
    });
    match check(&blank_right, Some((w, h)), &four_bands) {
        Ok(r) if r.problems.is_empty() => println!(
            "selftest right-blank: 已确认横带与整图指标看不见右侧置空（颜色数={} 背景占比={:.4}）",
            r.distinct, r.bg_ratio
        ),
        Ok(r) => bad.push(format!(
            "右侧置空被横带拓到了，那么竖带未被证明有必要：{:?}",
            r.problems
        )),
        Err(e) => bad.push(format!("右侧置空图解不开：{e}")),
    }
    match check(&blank_right, Some((w, h)), &[right_band]) {
        Ok(r) if r.problems.is_empty() => {
            bad.push(String::from("竖带没拓到右侧置空，它是装饰"))
        }
        Ok(r) => println!(
            "selftest right-blank + vband: 已正确判红（右带颜色数={}）",
            r.band_colors.first().map(|(_, n)| *n).unwrap_or(0)
        ),
        Err(e) => bad.push(format!("竖带读图失败：{e}")),
    }

    // 6. 带语法本身的错误要报，不能默默当成默认值。两个方向都要验。
    for bogus in ["1.0:0.5:8", "abc", "0.1:0.2", "0.1:2.0:8"] {
        for axis in [Axis::Horizontal, Axis::Vertical] {
            if Band::parse(axis, bogus).is_ok() {
                bad.push(format!("非法的{}语法 {bogus:?} 被接受了", axis.label()));
            }
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
        eprintln!(
            "用法：yi-shotcheck <a.png> … [--size WxH] [--band 上:下:颜色数]… [--vband 左:右:颜色数]… | --selftest"
        );
        std::process::exit(2);
    }
    let mut want: Option<(u32, u32)> = None;
    let mut bands: Vec<Band> = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let axis = match a.as_str() {
            "--band" => Some(Axis::Horizontal),
            "--vband" => Some(Axis::Vertical),
            _ => None,
        };
        if let Some(axis) = axis {
            let v = it.next().cloned().unwrap_or_default();
            match Band::parse(axis, &v) {
                Ok(b) => bands.push(b),
                Err(e) => {
                    eprintln!("SHOTCHECK FAILED: {e}");
                    std::process::exit(2);
                }
            }
            continue;
        }
        if a == "--size" {
            let v = it.next().cloned().unwrap_or_default();
            let (w, h) = v.split_once('x').unwrap_or(("0", "0"));
            want = Some((w.parse().unwrap_or(0), h.parse().unwrap_or(0)));
            continue;
        }
        files.push(PathBuf::from(a));
    }

    let mut failed = 0usize;
    for f in &files {
        match check(f, want, &bands) {
            Ok(r) => {
                let band_txt: Vec<String> = r
                    .band_colors
                    .iter()
                    .map(|(b, n)| {
                        format!("{}[{:.2},{:.2}]={n}", b.axis.label(), b.from, b.to)
                    })
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
