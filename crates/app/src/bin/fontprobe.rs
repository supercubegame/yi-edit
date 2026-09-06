//! 字体探测。**不开窗口**，所以三个平台都能在慢闸门里跑它。
//!
//! 为什么需要它：豆腐块是一条「坏了不会喊」的通道。截图只在 Linux runner 上拍，
//! 而那台机器自带 Droid fallback，于是截图永远是好的；真正出事的 Windows 上
//! 字体是 .ttc 集合而老候选表写的是 .ttf，一个字也画不出 —— 而闸门全绿。
//!
//! 拿不到字体时绝不能静默跳过：那与「字体正常」在面板上一模一样。明确报并 exit 1。

use yi_edit_session::fontpick;

fn main() {
    let required = fontpick::REQUIRED;
    let paths = fontpick::candidates();
    let want: String = required.iter().collect();
    println!(
        "FONTPROBE os={} 候选 {} 条 必需字={want}",
        std::env::consts::OS,
        paths.len()
    );
    for p in &paths {
        println!("  候选: {}", p.display());
    }
    let (picked, rejects) = fontpick::pick(&paths, required);
    for (path, why) in &rejects {
        println!("  拒: {} —— {why}", path.display());
    }
    match picked {
        Some(picked) => {
            let faces = fontpick::face_count(&picked.bytes)
                .map(|n| n.to_string())
                .unwrap_or_else(|e| e.to_string());
            println!(
                "  取: {} index={} faces={faces} bytes={}",
                picked.path.display(),
                picked.index,
                picked.bytes.len()
            );
            for &ch in required {
                match fontpick::glyph_id(&picked.bytes, picked.index, ch) {
                    Ok(g) => println!("  glyph {ch} = {g}"),
                    Err(e) => println!("  glyph {ch} = 错误 {e}"),
                }
            }
            println!("FONTPROBE OK");
        }
        None => {
            println!(
                "FONTPROBE FAIL 本次没有解析到任何覆盖中日韩字的字体（不是「字体正常」）：\
                 界面上所有中文都会是豆腐块。可用 {} 指定一份字体文件。",
                fontpick::FONT_ENV
            );
            std::process::exit(1);
        }
    }
}
