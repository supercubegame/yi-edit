//! 跳转映射的断言。这是整个面板里唯一会静默算错的东西：
//! 点一下跳到第 300 行而实际到了 320 行，不报错、不卡死、截图也看不出来。

use yi_edit_session::jump::{JumpMap, MIN_PANEL_PX};

/// 覆盖三种截然不同的形式：行比像素少、差不多、行比像素多很多。
/// 只试一种的话，多行挤在同一行像素上那种情形永远碰不到。
const SHAPES: &[(u32, usize)] = &[
    (2, 2),
    (2, 1000),
    (10, 3),
    (100, 100),
    (120, 37),
    (37, 120),
    (600, 1138),
    (800, 1_040_619),
    (7, 1_000_000),
];

/// 顶部与底部两个像素是显式夹的，所以往返断言对它们豁免。
/// **每种形状最多两个像素**，所以总数有上限；豁免范围一旦漂大，下面那条会红。
const MAX_CLAMP_EXEMPTIONS: usize = 2 * SHAPES.len();

/// 不能服务的几何直接拒，而不是拿一个像素去假装。
/// 两侧都断：1px 必须被拒，2px 必须被接受 —— 否则这个下限会静默往上漂。
#[test]
fn geometry_that_cannot_be_served_is_refused() {
    assert!(JumpMap::new(0, 100).is_none(), "高度为 0 时不应该假装能算");
    assert!(JumpMap::new(100, 0).is_none(), "空文件时不应该假装能算");
    // 1px：一个像素不可能同时是文首和文末。
    assert!(JumpMap::new(1, 1).is_none(), "1px 面板应该被拒");
    assert!(JumpMap::new(1, 1000).is_none(), "1px 面板应该被拒");
    // 2px：下限就在这里，必须被接受。
    assert!(
        JumpMap::new(MIN_PANEL_PX, 1000).is_some(),
        "{MIN_PANEL_PX}px 应该被接受"
    );
    assert_eq!(MIN_PANEL_PX, 2, "下限漂了，上面两侧的断言就不再夹着它了");
}

/// 承重：往返一致。line -> band 中点 -> line 必须回到原行。
///
/// 两类豁免，每类都单独计数并有上下限（否则豁免范围会静默变大，而那正是
/// 把一条断言改成装饰的典型手法）：
/// - **退化带**（top == bottom）：行比像素多时多行挤一个像素，它们在面板上本来就画不出来。
/// - **被夹的两个像素**（mid == 0 或 mid == h-1）：顶/底显式夹到文首/文末，
///   而 h < n 时那两个像素的带主可能不是第 0 / 最后一行。这是有意的取舍：
///   拖到顶要到文首比「顶部那一像素往返一致」重要。
///
/// 下限 400 是从**实测值 491** 往下留的余量。上一版拍了 1000，它第一次就红了。
#[test]
fn line_to_band_to_line_round_trips() {
    let mut checked = 0usize;
    let mut collapsed = 0usize;
    let mut clamped = 0usize;
    for (h, n) in SHAPES {
        let m = JumpMap::new(*h, *n).expect("合法几何");
        let mut probes: Vec<usize> = vec![0, n - 1];
        let step = (n / 500).max(1);
        let mut i = 0usize;
        while i < *n {
            probes.push(i);
            i += step;
        }
        for line in probes {
            let (top, bottom) = m.line_band(line).expect("行号合法");
            assert!(top <= bottom, "band 倒置：h={h} n={n} line={line}");
            assert!(bottom <= *h, "band 越界：h={h} n={n} line={line}");
            if top == bottom {
                collapsed += 1;
                continue;
            }
            let mid = top + (bottom - top) / 2;
            if mid == 0 || mid == h - 1 {
                clamped += 1;
                continue;
            }
            assert_eq!(
                m.line_at(mid),
                line,
                "h={h} n={n} line={line} band=({top},{bottom}) 往返不一致"
            );
            checked += 1;
        }
    }
    println!("往返实测：校过 {checked} 次，退化带 {collapsed} 次，被夹像素 {clamped} 次");
    assert!(
        checked > 400,
        "只真正校了 {checked} 次往返（实测基线 491），语料缩水了"
    );
    // 夹具自证：「多行挤一像素」那种形状真的被压到了。
    assert!(
        collapsed > 100,
        "只碰到 {collapsed} 个退化带，那一分支在测空气"
    );
    // 豁免不得漂大：每种形状最多两个像素。
    assert!(
        clamped <= MAX_CLAMP_EXEMPTIONS,
        "被夹像素豁免了 {clamped} 次，上限 {MAX_CLAMP_EXEMPTIONS}（每种形状最多两个）"
    );
}

/// band 必须单调不减且无缝：前一行的 bottom 等于后一行的 top。
/// 有缝的话面板上会有点不属于任何行，而那些点上的点击行为是不定的。
#[test]
fn bands_tile_the_panel_without_gaps() {
    for (h, n) in SHAPES {
        let m = JumpMap::new(*h, *n).unwrap();
        let probe = (*n).min(3000);
        let mut prev_bottom = 0u32;
        for line in 0..probe {
            let (top, bottom) = m.line_band(line).unwrap();
            assert_eq!(top, prev_bottom, "h={h} n={n} line={line} 带之间有缝或重叠");
            prev_bottom = bottom;
        }
        // 最后一行的 bottom 必须恰好是面板底部，否则拖到底到不了文末。
        let (_, last_bottom) = m.line_band(*n - 1).unwrap();
        assert_eq!(last_bottom, *h, "h={h} n={n} 最后一行没顶到面板底部");
    }
}

/// 首末行都必须能通过点击到达，包括越界的 y。
///
/// 这一条抓到了一个真 bug：行数比像素多时第 0 行的像素带是空的，
/// 于是 h=800 n=1000 时点最顶端会到第 1 行而不是文首。
#[test]
fn first_and_last_line_are_both_reachable() {
    for (h, n) in SHAPES {
        let m = JumpMap::new(*h, *n).unwrap();
        assert_eq!(m.line_at(0), 0, "h={h} n={n} 顶部没映到第 0 行");
        assert_eq!(m.line_at(h - 1), *n - 1, "h={h} n={n} 底部没映到最后一行");
        assert_eq!(
            m.line_at(h * 10),
            *n - 1,
            "h={h} n={n} 越界的 y 没夹到最后一行"
        );
    }
}

/// 顶/底那两个夹在 h >= n 时必须是**空操作** —— 否则它们就不是在修退化情形，
/// 而是在改写正常情形的结果。（h < n 时它们确实改写结果，那是有意的：
/// 拖到顶要到文首。往返断言里那两个像素已经显式记清了。）
#[test]
fn the_edge_clamps_are_no_ops_when_pixels_outnumber_lines() {
    fn binary_search_only(h: u32, n: usize, y: u32) -> usize {
        let hh = h as u64;
        let nn = n as u64;
        let mut lo = 0u64;
        let mut hi = nn - 1;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if (mid + 1) * hh / nn > y as u64 {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        lo as usize
    }
    let mut checked = 0usize;
    for (h, n) in SHAPES {
        if (*h as usize) < *n {
            continue;
        }
        let m = JumpMap::new(*h, *n).unwrap();
        for y in [0u32, h - 1] {
            assert_eq!(
                m.line_at(y),
                binary_search_only(*h, *n, y),
                "h={h} n={n} y={y}：夹改写了正常情形的结果"
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 8,
        "只校了 {checked} 次，语料里 h >= n 的形状太少"
    );
}

/// line_at 必须单调不减：往下点不能跳到更靠前的行。
#[test]
fn line_at_is_monotonic_in_y() {
    for (h, n) in SHAPES {
        let m = JumpMap::new(*h, *n).unwrap();
        let mut prev = 0usize;
        for y in 0..*h {
            let line = m.line_at(y);
            assert!(line >= prev, "h={h} n={n} y={y} 倒退了：{prev} -> {line}");
            assert!(line < *n, "h={h} n={n} y={y} 越界：{line}");
            prev = line;
        }
    }
}

/// 对照实验：浮点反算在同一组输入上**真的会差行**。
/// 不做这一步的话，「所以改成了整数二分」只是一句无从验证的声明。
#[test]
fn the_float_shortcut_really_would_have_been_off_by_a_line() {
    fn naive(h: u32, n: usize, y: u32) -> usize {
        ((y as f64 / h as f64) * n as f64) as usize
    }
    let mut disagreements = 0usize;
    for (h, n) in SHAPES {
        let m = JumpMap::new(*h, *n).unwrap();
        for line in 0..(*n).min(2000) {
            let (top, bottom) = m.line_band(line).unwrap();
            if top == bottom {
                continue;
            }
            let mid = top + (bottom - top) / 2;
            if naive(*h, *n, mid) != m.line_at(mid) {
                disagreements += 1;
            }
        }
    }
    assert!(
        disagreements > 0,
        "浮点写法在这批输入上一次都没差，那么改成整数二分这件事未被证明有必要"
    );
    println!("浮点反算与整数二分在 {disagreements} 处不一致（实测值）");
}

/// 可见窗口高亮区至少一像素高，且不越界。
/// 在百万行文件上，40 行的窗口占不到一像素，不夹的话指示器会完全消失。
#[test]
fn viewport_band_is_always_visible_and_in_bounds() {
    for (h, n) in SHAPES {
        let m = JumpMap::new(*h, *n).unwrap();
        for first in [0usize, n / 3, n.saturating_sub(1)] {
            for visible in [1usize, 40, 4000] {
                let (top, bottom) = m.viewport_band(first, visible);
                assert!(
                    bottom > top,
                    "h={h} n={n} first={first} 高亮区高度为 0，指示器会消失"
                );
                assert!(bottom <= *h, "h={h} n={n} first={first} 高亮区越下边界");
            }
        }
        assert_eq!(
            m.viewport_band(0, 10).0,
            0,
            "h={h} n={n} 滚到顶了高亮区却不从 0 开始"
        );
    }
}
