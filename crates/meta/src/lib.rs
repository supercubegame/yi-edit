//! 扫描器的公共部分。
//!
//! 一条规矩写在 AGENTS.md 里只在被读到时有效，断言每次都跑 —— 这个 crate 就是
//! 把那些规矩搬进闸门的地方。它不参与产品，只有 tests/。
//!
//! 两个原则：
//! - **写扫描器，不写正则。** 正则的失败方式是「匹配到零个」，而那不是覆盖不足，
//!   是让整类输入凭空消失。
//! - **「某段里有没有 X」必须先把那段切出来再找。** 注释里的字面量会同时造成
//!   漏报和误报，而且两个方向都踩过。**第五次踩到的是 shell 脚本：**
//!   义务的完成判据拿字串在 verify.sh 里找，而我在注释里解释了一句那个判据是什么，
//!   于是一条根本没做完的义务被认定为已完成 —— 而那会逐人把它从清单上删掉。
//!   **第七次是工作流文件，方向反了一次：** 我在注释里写下「第一版用了 git add -A」，
//!   而那条负向断言正在找它，于是一条**已经修好**的规矩被判成还坏着。

use std::path::{Path, PathBuf};

/// 仓库根目录。从本 crate 的 manifest 目录往上两层。
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/meta 应该在仓库根下两层")
        .to_path_buf()
}

pub fn read(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {}：{e}", p.display()))
}

pub fn exists(rel: &str) -> bool {
    repo_root().join(rel).exists()
}

/// 列一个目录下的文件名（不递归），排序后返回。
pub fn list_dir(rel: &str) -> Vec<String> {
    let p = repo_root().join(rel);
    let mut out: Vec<String> = std::fs::read_dir(&p)
        .unwrap_or_else(|e| panic!("列不了 {}：{e}", p.display()))
        .map(|e| e.expect("目录项").file_name().to_string_lossy().to_string())
        .collect();
    out.sort();
    out
}

pub fn list_subdirs(rel: &str) -> Vec<String> {
    let root = repo_root().join(rel);
    let mut out: Vec<String> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("列不了 {}：{e}", root.display()))
        .filter_map(|e| {
            let e = e.expect("目录项");
            if e.file_type().ok()?.is_dir() {
                Some(e.file_name().to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();
    out.sort();
    out
}

/// 把 Rust 源码里的注释与字符串字面量换成等长的空白，只留可执行代码。
///
/// 为什么不直接删：保留长度让字节数自证（下面那条四成的断言）仍然有意义，
/// 也让行号不错位。
pub fn strip_comments_and_strings(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = vec![b' '; b.len()];
    let mut i = 0usize;
    while i < b.len() {
        // 行注释
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // 块注释（支持嵌套，Rust 的块注释是可嵌套的）
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            let mut depth = 1usize;
            i += 2;
            while i < b.len() && depth > 0 {
                if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        // 原始字符串 r"..." / r#"..."#
        if b[i] == b'r' && i + 1 < b.len() && (b[i + 1] == b'"' || b[i + 1] == b'#') {
            let mut j = i + 1;
            let mut hashes = 0usize;
            while j < b.len() && b[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < b.len() && b[j] == b'"' {
                j += 1;
                loop {
                    if j >= b.len() {
                        break;
                    }
                    if b[j] == b'"' {
                        let mut k = 0usize;
                        while k < hashes && j + 1 + k < b.len() && b[j + 1 + k] == b'#' {
                            k += 1;
                        }
                        if k == hashes {
                            j += 1 + hashes;
                            break;
                        }
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
        }
        // 普通字符串
        if b[i] == b'"' {
            i += 1;
            while i < b.len() {
                if b[i] == b'\\' {
                    i += 2;
                    continue;
                }
                if b[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        out[i] = b[i];
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// 把 shell 脚本里的注释去掉，只留可执行代码。
///
/// **它不懂引号。** 一个落在引号里的 `#` 也会被当成注释起点，于是后面的代码会被剥掉。
/// 方向是安全的一侧（宁可漏看不可误认）：对「完成判据」这类扫描来说，
/// 把真代码剥掉会让义务看起来「没做完」（最多多挂一轮），
/// 而把注释当成代码会让一条根本没做完的义务被删掉。后者不可接受。
pub fn strip_shell_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let bytes = line.as_bytes();
        let mut cut = line.len();
        for (i, c) in line.char_indices() {
            if c != '#' {
                continue;
            }
            let prev_is_space = i == 0 || bytes[i - 1].is_ascii_whitespace();
            if prev_is_space {
                cut = i;
                break;
            }
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

/// shell 脚本的可执行代码里包不包含某个子串。
pub fn shell_code_contains(src: &str, needle: &str) -> bool {
    strip_shell_comments(src).contains(needle)
}

/// 把 YAML 里的**整行注释**去掉，只留可执行部分。
///
/// 只剥整行注释（`#` 前面只有空白），不剥行尾注释：YAML 的值里 `#` 很常见
/// （比如 `uses: foo@<sha> # v1`），而误剥会把那一行的 SHA 检查废掉。
pub fn strip_yaml_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        if !line.trim_start().starts_with('#') {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// YAML 的非注释部分里包不包含某个子串。
pub fn yaml_code_contains(src: &str, needle: &str) -> bool {
    strip_yaml_comments(src).contains(needle)
}

/// 可执行代码里命中某个子串的行（带行号）。
pub fn hits_in_code(src: &str, needle: &str) -> Vec<(usize, String)> {
    let code = strip_comments_and_strings(src);
    code.lines()
        .enumerate()
        .filter(|(_, l)| l.contains(needle))
        .map(|(i, l)| (i + 1, l.trim().to_string()))
        .collect()
}

/// 把 Markdown 按二级标题切成段，返回 (标题, 正文)。
pub fn md_sections(src: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in src.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            out.push((title.trim().to_string(), String::new()));
        } else if let Some(last) = out.last_mut() {
            last.1.push_str(line);
            last.1.push('\n');
        }
    }
    out
}
