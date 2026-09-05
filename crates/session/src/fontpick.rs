//! 字体挑选：只挑「真的画得出中日韩字」的那一张脸。
//!
//! 为什么不是「文件存在且够大」：Windows 上微软雅黑是 **msyh.ttc**（字体集合），
//! 老候选表写的是 msyh.ttf —— 于是文件根本读不到，一个中文字也没有，界面全是豆腐块。
//! 而集合里第 0 张脸未必覆盖中文，所以必须连 face index 一起挑。
//!
//! **豆腐块是一条「坏了不会喊」的通道。** Linux runner 自带 Droid fallback，
//! 于是截图永远是好的，而真正出事的平台压根不拍截图。所以这一层故意放在
//! 纯层（只解析字节，不碰 GUI），快闸门拿合成字体直接断它；读文件的部分薄薄包在外面。

use std::io;
use std::path::{Path, PathBuf};

/// 界面上一定会画到的字（工具条 / 侧栏 / 状态条）。挑不出覆盖它们的字体就不算挑到。
pub const REQUIRED: &[char] = &['中', '文', '侧', '栏', '保', '存'];

/// 低于这个字节数的文件不可能是一份真字体。它**只**用来拓「读到的是个占位文件」，
/// 覆盖与否由下面的 cmap 查询判，不由体积判—— 老检查就是只看体积，而它看不见豆腐块。
pub const MIN_FONT_BYTES: usize = 4096;

/// Windows 候选表。**这是一张登记表**：这里的中日韩字体几乎全是 .ttc 集合，
/// 表里只写 .ttf 就是当前这个 bug。有断言守着它里真的有集合。
pub const WINDOWS_CANDIDATES: &[&str] = &[
    "C:\\Windows\\Fonts\\msyh.ttc",
    "C:\\Windows\\Fonts\\msyhl.ttc",
    "C:\\Windows\\Fonts\\simsun.ttc",
    "C:\\Windows\\Fonts\\msjh.ttc",
    "C:\\Windows\\Fonts\\mingliu.ttc",
    "C:\\Windows\\Fonts\\YuGothM.ttc",
    "C:\\Windows\\Fonts\\simhei.ttf",
    "C:\\Windows\\Fonts\\msgothic.ttc",
    "C:\\Windows\\Fonts\\malgun.ttf",
    "C:\\Windows\\Fonts\\msyh.ttf",
];

/// macOS 候选表。PingFang 也是集合，且第 0 张脸不保证是简体。
pub const MACOS_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/System/Library/Fonts/STHeiti Light.ttc",
    "/System/Library/Fonts/Supplemental/Songti.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
    "/System/Library/Fonts/SFNSMono.ttf",
];

/// Linux / BSD 候选表。
pub const UNIX_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    "/usr/share/fonts/truetype/arphic/uming.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttf",
];

/// 手动指定字体的逗口。候选表让人卡死时至少有一条出路。
pub const FONT_ENV: &str = "YI_EDIT_FONT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reject {
    Unreadable(io::ErrorKind),
    TooSmall(usize),
    NotSfnt([u8; 4]),
    FaceOutOfRange {
        index: u32,
        faces: u32,
    },
    Truncated(&'static str),
    NoCmap,
    NoUnicodeSubtable,
    /// 字体本身没问题，但它画不出这个字 —— 照样用就是豆腐块。
    Missing(char),
}

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reject::Unreadable(kind) => write!(f, "读不到（{kind:?}）"),
            Reject::TooSmall(n) => write!(f, "只有 {n} 字节，不可能是字体"),
            Reject::NotSfnt(tag) => write!(f, "不是 sfnt/ttc（首四字节 {tag:?}）"),
            Reject::FaceOutOfRange { index, faces } => {
                write!(f, "要第 {index} 张脸，而里面只有 {faces} 张")
            }
            Reject::Truncated(what) => write!(f, "截断于 {what}"),
            Reject::NoCmap => write!(f, "没有 cmap 表"),
            Reject::NoUnicodeSubtable => write!(f, "cmap 里没有 Unicode 子表"),
            Reject::Missing(ch) => write!(f, "画不出「{ch}」（照样用就是豆腐块）"),
        }
    }
}

/// 挑到的那一张脸。`index` 必须一起交给渲染层，否则集合里用的还是第 0 张。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picked {
    pub path: PathBuf,
    pub index: u32,
    pub bytes: Vec<u8>,
}

const TTCF: &[u8; 4] = b"ttcf";

fn u16_at(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(off)?, *b.get(off + 1)?]))
}

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *b.get(off)?,
        *b.get(off + 1)?,
        *b.get(off + 2)?,
        *b.get(off + 3)?,
    ]))
}

fn tag_at(b: &[u8], off: usize) -> Option<[u8; 4]> {
    Some([
        *b.get(off)?,
        *b.get(off + 1)?,
        *b.get(off + 2)?,
        *b.get(off + 3)?,
    ])
}

fn is_sfnt(tag: &[u8; 4]) -> bool {
    matches!(tag, [0x00, 0x01, 0x00, 0x00] | b"true" | b"OTTO" | b"typ1")
}

/// 集合里有几张脸；普通 ttf/otf 是 1。
pub fn face_count(b: &[u8]) -> Result<u32, Reject> {
    if b.len() < 12 {
        return Err(Reject::TooSmall(b.len()));
    }
    let tag = tag_at(b, 0).ok_or(Reject::Truncated("sfnt header"))?;
    if &tag == TTCF {
        let n = u32_at(b, 8).ok_or(Reject::Truncated("ttc numFonts"))?;
        if n == 0 {
            return Err(Reject::Truncated("ttc numFonts=0"));
        }
        return Ok(n);
    }
    if !is_sfnt(&tag) {
        return Err(Reject::NotSfnt(tag));
    }
    Ok(1)
}

/// 第 index 张脸的 sfnt 头在文件里的偏移。
///
/// **表目录里的偏移是相对文件开头的**，集合与单字体在这一点上一致，
/// 所以下面读表统一按绝对偏移。
fn face_offset(b: &[u8], index: u32) -> Result<usize, Reject> {
    let faces = face_count(b)?;
    if index >= faces {
        return Err(Reject::FaceOutOfRange { index, faces });
    }
    let tag = tag_at(b, 0).ok_or(Reject::Truncated("sfnt header"))?;
    if &tag != TTCF {
        return Ok(0);
    }
    let off =
        u32_at(b, 12 + 4 * index as usize).ok_or(Reject::Truncated("ttc offset table"))? as usize;
    let inner = tag_at(b, off).ok_or(Reject::Truncated("ttc face header"))?;
    if !is_sfnt(&inner) {
        return Err(Reject::NotSfnt(inner));
    }
    Ok(off)
}

fn table(b: &[u8], face: usize, want: &[u8; 4]) -> Option<usize> {
    let n = u16_at(b, face + 4)? as usize;
    for i in 0..n {
        let rec = face + 12 + i * 16;
        let tag = tag_at(b, rec)?;
        if &tag == want {
            return Some(u32_at(b, rec + 8)? as usize);
        }
    }
    None
}

/// cmap 子表里 `cp` 对应的 glyph id。0 意为「没有」，`None` 意为「这张子表读不下去」。
fn lookup(b: &[u8], sub: usize, cp: u32) -> Option<u32> {
    match u16_at(b, sub)? {
        4 => lookup_format4(b, sub, cp),
        6 => lookup_format6(b, sub, cp),
        12 => lookup_format12(b, sub, cp),
        _ => None,
    }
}

fn lookup_format4(b: &[u8], sub: usize, cp: u32) -> Option<u32> {
    if cp > 0xFFFF {
        return Some(0);
    }
    let cp = cp as u16;
    let seg2 = u16_at(b, sub + 6)? as usize;
    if seg2 < 2 || seg2 % 2 != 0 {
        return None;
    }
    // 段数抽上限：一份被改坏的字体里 segCount 可以大到把探测变成死循环。
    let avail = b.len().saturating_sub(sub + 16) / 8;
    let segs = (seg2 / 2).min(avail);
    let end = sub + 14;
    let start = end + seg2 + 2;
    let delta = start + seg2;
    let range = delta + seg2;
    for i in 0..segs {
        let e = u16_at(b, end + i * 2)?;
        if e < cp {
            continue;
        }
        let s = u16_at(b, start + i * 2)?;
        if s > cp {
            return Some(0);
        }
        let d = u16_at(b, delta + i * 2)?;
        let ro = u16_at(b, range + i * 2)?;
        if ro == 0 {
            return Some(cp.wrapping_add(d) as u32);
        }
        let at = range + i * 2 + ro as usize + 2 * (cp - s) as usize;
        let g = u16_at(b, at)?;
        if g == 0 {
            return Some(0);
        }
        return Some(g.wrapping_add(d) as u32);
    }
    Some(0)
}

fn lookup_format6(b: &[u8], sub: usize, cp: u32) -> Option<u32> {
    let first = u16_at(b, sub + 6)? as u32;
    let count = u16_at(b, sub + 8)? as u32;
    if cp < first || cp >= first + count {
        return Some(0);
    }
    let at = sub + 10 + 2 * (cp - first) as usize;
    Some(u16_at(b, at)? as u32)
}

fn lookup_format12(b: &[u8], sub: usize, cp: u32) -> Option<u32> {
    let n = u32_at(b, sub + 12)? as usize;
    let avail = b.len().saturating_sub(sub + 16) / 12;
    let n = n.min(avail);
    for i in 0..n {
        let g = sub + 16 + i * 12;
        let s = u32_at(b, g)?;
        let e = u32_at(b, g + 4)?;
        let start_glyph = u32_at(b, g + 8)?;
        if s <= cp && cp <= e {
            return Some(start_glyph + (cp - s));
        }
    }
    Some(0)
}

/// `ch` 在这张脸上的 glyph id。0 就是豆腐块。
///
/// 注意「返回 0」与「报错」是两件事：前者是字体正常但不含这个字，
/// 后者是这份文件压根不能用。杂在一起的话报告里就分不出该换字体还是该换路径。
pub fn glyph_id(b: &[u8], index: u32, ch: char) -> Result<u32, Reject> {
    let face = face_offset(b, index)?;
    let cmap = table(b, face, b"cmap").ok_or(Reject::NoCmap)?;
    let n = u16_at(b, cmap + 2).ok_or(Reject::Truncated("cmap numTables"))? as usize;
    if n == 0 {
        return Err(Reject::NoUnicodeSubtable);
    }
    let mut looked = 0usize;
    for i in 0..n {
        let rec = cmap + 4 + i * 8;
        let Some(platform) = u16_at(b, rec) else {
            continue;
        };
        let Some(encoding) = u16_at(b, rec + 2) else {
            continue;
        };
        let Some(off) = u32_at(b, rec + 4) else {
            continue;
        };
        // 只认 Unicode 子表：(3,1) BMP、(3,10) 全平面、(0,*) Unicode。
        // (3,0) 是符号编码，它把字形挂在 PUA 上，拿它当覆盖会误报。
        if !matches!((platform, encoding), (3, 1) | (3, 10) | (0, _)) {
            continue;
        }
        looked += 1;
        if let Some(g) = lookup(b, cmap + off as usize, ch as u32) {
            if g != 0 {
                return Ok(g);
            }
        }
    }
    if looked == 0 {
        return Err(Reject::NoUnicodeSubtable);
    }
    Ok(0)
}

/// 这张脸覆盖不覆盖全部必需字。
pub fn face_covers(b: &[u8], index: u32, required: &[char]) -> Result<(), Reject> {
    if b.len() < MIN_FONT_BYTES {
        return Err(Reject::TooSmall(b.len()));
    }
    for &ch in required {
        if glyph_id(b, index, ch)? == 0 {
            return Err(Reject::Missing(ch));
        }
    }
    Ok(())
}

/// 一份字体文件里第一张覆盖全部必需字的脸。
pub fn pick_face(b: &[u8], required: &[char]) -> Result<u32, Reject> {
    let faces = face_count(b)?;
    let mut last = Reject::NoCmap;
    for i in 0..faces.min(64) {
        match face_covers(b, i, required) {
            Ok(()) => return Ok(i),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// 当前平台的候选表。`YI_EDIT_FONT` 排在最前面。
pub fn candidates() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var(FONT_ENV) {
        if !p.trim().is_empty() {
            out.push(PathBuf::from(p));
        }
    }
    let table = if cfg!(target_os = "windows") {
        WINDOWS_CANDIDATES
    } else if cfg!(target_os = "macos") {
        MACOS_CANDIDATES
    } else {
        UNIX_CANDIDATES
    };
    out.extend(table.iter().map(PathBuf::from));
    out
}

/// 按顺序试候选。**每一条拒绝的理由都要带回去**：否则「一个都没挑到」
/// 只剩一个比特，而「路径不对」与「字体里没中文」后续动作完全不同。
pub fn pick(paths: &[PathBuf], required: &[char]) -> (Option<Picked>, Vec<(PathBuf, Reject)>) {
    let mut rejects: Vec<(PathBuf, Reject)> = Vec::new();
    for path in paths {
        match read_font(path) {
            Ok(bytes) => match pick_face(&bytes, required) {
                Ok(index) => {
                    return (
                        Some(Picked {
                            path: path.clone(),
                            index,
                            bytes,
                        }),
                        rejects,
                    )
                }
                Err(why) => rejects.push((path.clone(), why)),
            },
            Err(kind) => rejects.push((path.clone(), Reject::Unreadable(kind))),
        }
    }
    (None, rejects)
}

fn read_font(path: &Path) -> Result<Vec<u8>, io::ErrorKind> {
    std::fs::read(path).map_err(|e| e.kind())
}
