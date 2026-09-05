//! 字体挑选的断言。夹具是**手搓的字节**，不依赖任何系统字体：
//! 拿真字体做夹具的话，这层断言就变成了「这台 runner 装了什么字体」，
//! 而那正是我想拓的东西。

use std::path::PathBuf;

use yi_edit_session::fontpick::{self, Reject};

fn be16(v: u16) -> [u8; 2] {
    v.to_be_bytes()
}
fn be32(v: u32) -> [u8; 4] {
    v.to_be_bytes()
}

/// cmap format 4。每段用 idDelta=1 做恒等偏移，于是命中一定不为 0。
fn cmap4(ranges: &[(u16, u16)]) -> Vec<u8> {
    let mut segs: Vec<(u16, u16)> = ranges.to_vec();
    segs.push((0xFFFF, 0xFFFF));
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&be16(4));
    out.extend_from_slice(&be16(0)); // length 占位
    out.extend_from_slice(&be16(0)); // language
    out.extend_from_slice(&be16((segs.len() * 2) as u16));
    out.extend_from_slice(&be16(0)); // searchRange
    out.extend_from_slice(&be16(0)); // entrySelector
    out.extend_from_slice(&be16(0)); // rangeShift
    for &(_, e) in &segs {
        out.extend_from_slice(&be16(e));
    }
    out.extend_from_slice(&be16(0)); // reservedPad
    for &(s, _) in &segs {
        out.extend_from_slice(&be16(s));
    }
    for _ in &segs {
        out.extend_from_slice(&be16(1)); // idDelta
    }
    for _ in &segs {
        out.extend_from_slice(&be16(0)); // idRangeOffset
    }
    let len = out.len() as u16;
    out[2..4].copy_from_slice(&be16(len));
    out
}

/// cmap format 12。第 i 组的 startGlyphID = i + 1。
fn cmap12(groups: &[(u32, u32)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&be16(12));
    out.extend_from_slice(&be16(0));
    out.extend_from_slice(&be32(0)); // length 占位
    out.extend_from_slice(&be32(0)); // language
    out.extend_from_slice(&be32(groups.len() as u32));
    for (i, &(s, e)) in groups.iter().enumerate() {
        out.extend_from_slice(&be32(s));
        out.extend_from_slice(&be32(e));
        out.extend_from_slice(&be32(i as u32 + 1));
    }
    let len = out.len() as u32;
    out[4..8].copy_from_slice(&be32(len));
    out
}

fn cmap(platform: u16, encoding: u16, sub: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&be16(0)); // version
    out.extend_from_slice(&be16(1)); // numTables
    out.extend_from_slice(&be16(platform));
    out.extend_from_slice(&be16(encoding));
    out.extend_from_slice(&be32(12)); // 子表偏移（相对 cmap 开头）
    out.extend_from_slice(sub);
    out
}

/// 造一张脸。`base` 是这张脸的 sfnt 头在**整个文件**里的偏移，
/// 因为表目录里的偏移是绝对的 —— 集合里也一样，这正是 ttc 必须带 index 的地方。
fn face(base: usize, cm: &[u8], pad: usize) -> Vec<u8> {
    let num_tables: u16 = 2;
    let dir = 12 + 16 * num_tables as usize;
    let cmap_off = base + dir;
    let pad_off = cmap_off + cm.len();
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&be32(0x0001_0000));
    out.extend_from_slice(&be16(num_tables));
    out.extend_from_slice(&be16(0));
    out.extend_from_slice(&be16(0));
    out.extend_from_slice(&be16(0));
    out.extend_from_slice(b"cmap");
    out.extend_from_slice(&be32(0));
    out.extend_from_slice(&be32(cmap_off as u32));
    out.extend_from_slice(&be32(cm.len() as u32));
    out.extend_from_slice(b"post");
    out.extend_from_slice(&be32(0));
    out.extend_from_slice(&be32(pad_off as u32));
    out.extend_from_slice(&be32(pad as u32));
    out.extend_from_slice(cm);
    out.extend(std::iter::repeat(0x5A).take(pad));
    out
}

fn ttc(cmaps: &[Vec<u8>], pad: usize) -> Vec<u8> {
    let header = 12 + 4 * cmaps.len();
    let mut faces: Vec<Vec<u8>> = Vec::new();
    let mut at = header;
    for cm in cmaps {
        let f = face(at, cm, pad);
        at += f.len();
        faces.push(f);
    }
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"ttcf");
    out.extend_from_slice(&be32(0x0001_0000));
    out.extend_from_slice(&be32(cmaps.len() as u32));
    let mut off = header;
    for f in &faces {
        out.extend_from_slice(&be32(off as u32));
        off += f.len();
    }
    for f in &faces {
        out.extend_from_slice(f);
    }
    out
}

const PAD: usize = 5000;

fn latin_only() -> Vec<u8> {
    cmap(3, 1, &cmap4(&[(0x20, 0x7E)]))
}
fn cjk_bmp() -> Vec<u8> {
    cmap(3, 1, &cmap4(&[(0x20, 0x7E), (0x4E00, 0x9FFF)]))
}
fn cjk_full() -> Vec<u8> {
    cmap(
        3,
        10,
        &cmap12(&[(0x20, 0x7E), (0x3000, 0x9FFF), (0x20000, 0x2A6DF)]),
    )
}

#[test]
fn a_font_that_really_maps_cjk_is_accepted() {
    let f = face(0, &cjk_bmp(), PAD);
    assert_eq!(fontpick::face_count(&f), Ok(1));
    assert_eq!(fontpick::face_covers(&f, 0, fontpick::REQUIRED), Ok(()));
    assert_eq!(fontpick::pick_face(&f, fontpick::REQUIRED), Ok(0));
    assert!(fontpick::glyph_id(&f, 0, '中').unwrap() != 0);
}

/// 这一条是承重的：**先证明老检查真的看不见它。**
/// 老检查是「文件读得到且 > 4096 字节」，而一份只有拉丁的字体完全满足它，
/// 照样装上去整个界面就是豆腐块 —— 这正是 Windows 上拍到的那张图。
#[test]
fn the_old_size_only_check_would_have_accepted_a_latin_only_font() {
    let f = face(0, &latin_only(), PAD);
    let old_check_passes = f.len() > fontpick::MIN_FONT_BYTES;
    assert!(
        old_check_passes,
        "夹具本身没过老检查（{} 字节），那这条对照没比到东西",
        f.len()
    );
    assert_eq!(
        fontpick::face_covers(&f, 0, fontpick::REQUIRED),
        Err(Reject::Missing('中'))
    );
    // 拉丁字本身要能查到，否则上面那个 Missing 只说明我把 cmap 解错了。
    assert!(fontpick::glyph_id(&f, 0, 'A').unwrap() != 0);
}

#[test]
fn format_12_subtables_work_including_beyond_the_bmp() {
    let f = face(0, &cjk_full(), PAD);
    assert_eq!(fontpick::face_covers(&f, 0, fontpick::REQUIRED), Ok(()));
    assert!(fontpick::glyph_id(&f, 0, '\u{20001}').unwrap() != 0);
    assert_eq!(fontpick::glyph_id(&f, 0, '\u{1000}'), Ok(0));
}

/// 集合里第 0 张脸不覆盖中文。不带 index 直接用的话，字体文件是对的而屏幕上仍然是豆腐块。
#[test]
fn a_collection_picks_the_face_that_covers_cjk_not_just_face_zero() {
    let col = ttc(&[latin_only(), cjk_bmp()], PAD);
    assert_eq!(fontpick::face_count(&col), Ok(2));
    assert_eq!(
        fontpick::face_covers(&col, 0, fontpick::REQUIRED),
        Err(Reject::Missing('中'))
    );
    assert_eq!(fontpick::face_covers(&col, 1, fontpick::REQUIRED), Ok(()));
    assert_eq!(fontpick::pick_face(&col, fontpick::REQUIRED), Ok(1));
    assert_eq!(
        fontpick::glyph_id(&col, 2, '中'),
        Err(Reject::FaceOutOfRange { index: 2, faces: 2 })
    );
}

#[test]
fn a_collection_with_no_cjk_face_at_all_is_rejected_with_a_reason() {
    let col = ttc(&[latin_only(), latin_only()], PAD);
    assert_eq!(
        fontpick::pick_face(&col, fontpick::REQUIRED),
        Err(Reject::Missing('中'))
    );
}

/// 截断与垃圾字节要返回错误，不能 panic：一个在启动路径上 panic 的探测
/// 会把「字体没挑到」变成「程序起不来」，而后者的报告里看不到真正的原因。
#[test]
fn truncated_and_garbage_fonts_are_errors_not_panics() {
    let col = ttc(&[latin_only(), cjk_bmp()], 256);
    for n in 0..col.len() {
        let _ = fontpick::face_count(&col[..n]);
        let _ = fontpick::glyph_id(&col[..n], 0, '中');
        let _ = fontpick::glyph_id(&col[..n], 1, '中');
        let _ = fontpick::pick_face(&col[..n], fontpick::REQUIRED);
    }
    assert!(fontpick::face_count(&[0u8; 9000]).is_err());
    assert!(fontpick::face_count(&[0xFFu8; 3]).is_err());
    assert_eq!(fontpick::face_count(&[]), Err(Reject::TooSmall(0)));
}

/// 三张候选表都要在任何平台上可断言。把它们写成 cfg 里的局部常量的话，
/// Windows 那张表在 Linux 快闸门里压根不存在 —— 而它正是出事的那张。
#[test]
fn the_windows_table_lists_collections_because_that_was_the_bug() {
    let win = fontpick::WINDOWS_CANDIDATES;
    let ttc_count = win.iter().filter(|p| p.ends_with(".ttc")).count();
    assert!(
        ttc_count >= 3,
        "Windows 候选表里只有 {ttc_count} 个 .ttc；这里的中日韩字体几乎全是集合"
    );
    assert!(
        win.iter().any(|p| p.ends_with("msyh.ttc")),
        "候选表里没有 msyh.ttc，而微软雅黑就是那个名字"
    );
    for table in [
        fontpick::WINDOWS_CANDIDATES,
        fontpick::MACOS_CANDIDATES,
        fontpick::UNIX_CANDIDATES,
    ] {
        assert!(table.len() >= 4, "候选表太短：{table:?}");
        let mut sorted: Vec<&str> = table.to_vec();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(before, sorted.len(), "候选表里有重复项：{table:?}");
        for p in table {
            assert!(!p.trim().is_empty());
            assert!(
                p.starts_with('/') || p.contains(":\\"),
                "候选路径不是绝对路径：{p}"
            );
        }
    }
}

#[test]
fn the_current_platform_table_is_what_candidates_returns() {
    let got = fontpick::candidates();
    let want = if cfg!(target_os = "windows") {
        fontpick::WINDOWS_CANDIDATES
    } else if cfg!(target_os = "macos") {
        fontpick::MACOS_CANDIDATES
    } else {
        fontpick::UNIX_CANDIDATES
    };
    // 环境变量可能在最前面插一条，所以只断言末尾那段。
    assert!(got.len() >= want.len());
    let tail: Vec<PathBuf> = got[got.len() - want.len()..].to_vec();
    let want: Vec<PathBuf> = want.iter().map(PathBuf::from).collect();
    assert_eq!(tail, want);
}

/// 拒绝理由要按候选逐条带回去，而且“读不到”与“字体里没有这个字”必须分开。
#[test]
fn pick_reports_every_rejection_with_its_own_reason() {
    let dir = std::env::temp_dir().join(format!("yi-fontpick-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    let missing = dir.join("不存在 带空格.ttc");
    let latin = dir.join("latin only.ttf");
    let good = dir.join("good.ttc");
    std::fs::write(&latin, face(0, &latin_only(), PAD)).expect("写拉丁字体");
    std::fs::write(&good, ttc(&[latin_only(), cjk_bmp()], PAD)).expect("写集合");

    let paths = vec![missing.clone(), latin.clone(), good.clone()];
    let (picked, rejects) = fontpick::pick(&paths, fontpick::REQUIRED);
    let picked = picked.expect("应该挑到第三个");
    assert_eq!(picked.path, good);
    assert_eq!(picked.index, 1, "没有选到覆盖中文的那张脸");
    assert_eq!(rejects.len(), 2);
    assert_eq!(rejects[0].0, missing);
    assert!(matches!(rejects[0].1, Reject::Unreadable(_)));
    assert_eq!(rejects[1], (latin.clone(), Reject::Missing('中')));
    // 拒绝理由要能印成人读得懂的一句话，否则报告里只有一个枚举名。
    assert!(rejects[1].1.to_string().contains('中'));

    let (none, all) = fontpick::pick(&[latin], fontpick::REQUIRED);
    assert!(none.is_none());
    assert_eq!(all.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
