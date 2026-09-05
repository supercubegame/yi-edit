//! 文件面板的断言。碰真实目录，不拿假数据冒充。

use std::fs;
use std::path::PathBuf;

use yi_edit_session::browser::{self, MAX_ENTRIES};

struct Tmp(PathBuf);

impl Tmp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("yi-browser-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("建临时目录");
        Self(dir)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 排序必须确定：目录先，同类按名（忽略大小写）。
/// 目录项的返回顺序本身不保证，而一个每次刷新都换顺序的面板比没有面板更难用。
#[test]
fn listing_is_deterministic_dirs_first_then_case_insensitive_name() {
    let t = Tmp::new("sort");
    for name in ["zeta.txt", "Alpha.rs", "beta.md", "alpha.rs"] {
        fs::write(t.0.join(name), b"x").unwrap();
    }
    for name in ["zz_dir", "Aa_dir"] {
        fs::create_dir(t.0.join(name)).unwrap();
    }

    let a = browser::list_dir(&t.0, false).expect("列目录");
    let b = browser::list_dir(&t.0, false).expect("再列一遍");
    assert_eq!(a, b, "两次列结果不一样，面板会每次刷新都换顺序");

    let names: Vec<&str> = a.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Aa_dir", "zz_dir", "Alpha.rs", "alpha.rs", "beta.md", "zeta.txt"],
        "排序不对"
    );
    // 夹具自证：真的同时有目录与文件，否则「目录先」那一条在测空气。
    assert_eq!(a.entries.iter().filter(|e| e.is_dir).count(), 2);
    assert_eq!(a.entries.iter().filter(|e| !e.is_dir).count(), 4);
}

/// 隐藏文件默认不列，但**不是静默丢掉**：计数要报出来，两侧都能红。
#[test]
fn hidden_entries_are_counted_not_silently_dropped() {
    let t = Tmp::new("hidden");
    fs::write(t.0.join("visible.txt"), b"x").unwrap();
    fs::write(t.0.join(".secret"), b"x").unwrap();
    fs::write(t.0.join(".another"), b"x").unwrap();

    let hide = browser::list_dir(&t.0, false).unwrap();
    assert_eq!(hide.entries.len(), 1, "隐藏文件没被过滤");
    assert_eq!(hide.hidden_skipped, 2, "跳过了却没报数量");

    let show = browser::list_dir(&t.0, true).unwrap();
    assert_eq!(show.entries.len(), 3, "请求显示隐藏文件却没显示");
    assert_eq!(show.hidden_skipped, 0, "没跳过却报了数量，这一侧也得能红");
}

#[test]
fn parent_is_reported_and_missing_dir_is_an_error() {
    let t = Tmp::new("parent");
    let sub = t.0.join("sub");
    fs::create_dir(&sub).unwrap();
    let l = browser::list_dir(&sub, false).unwrap();
    assert_eq!(l.parent.as_deref(), Some(t.0.as_path()), "没报父目录，面板就上不去");
    assert!(l.entries.is_empty());
    assert_eq!(l.truncated, 0);

    assert!(
        browser::list_dir(&t.0.join("nope"), false).is_err(),
        "不存在的目录应该报错，而不是默默给个空列表"
    );
}

#[test]
fn dir_for_points_at_the_containing_directory() {
    let t = Tmp::new("dirfor");
    let f = t.0.join("a.txt");
    fs::write(&f, b"x").unwrap();
    assert_eq!(browser::dir_for(&f).as_deref(), Some(t.0.as_path()));
}

/// 上限本身得是个像样的数，而且截断字段得存在。
/// 不真的建两万个文件（那会把快闸门拖成慢闸门），但也不假装验过了截断行为：
/// 这一条只声明自己验的是常量与字段，真实截断路径记在 docs/PITFALLS.md 里。
#[test]
fn the_entry_cap_is_sane_and_truncation_is_reportable() {
    assert!(MAX_ENTRIES >= 500, "上限 {MAX_ENTRIES} 太小，普通仓库目录就会被截");
    let t = Tmp::new("cap");
    let l = browser::list_dir(&t.0, false).unwrap();
    assert_eq!(l.truncated, 0, "空目录却报了截断");
}
