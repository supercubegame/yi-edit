//! 新建与另存为。这一组里有两条是盯丢数据的：
//! 没路径时保存必须报错（而不是静默丢掉），只读模式另存为必须一个字节也不写。
//!
//! 另外一条看起来很小但不是：另存为 `.rs` 之后必须按 Rust 上色。
//! 不重算语言不会报错，只会让颜色停在旧语言上，而用户会以为高亮坏了。

use std::fs;
use std::path::{Path, PathBuf};

use yi_edit_core::{Lang, Pos};
use yi_edit_session::Editor;

struct Tmp(PathBuf);

impl Tmp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("yi-saveas-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("建临时目录");
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let p = self.path(name);
        fs::write(&p, text.as_bytes()).expect("写临时文件");
        p
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 刚启动的编辑器没有路径。敲了字再 Ctrl+S 必须得到一个**说得出原因**的错误，
/// 而不是一个看起来像成功的 Ok。
#[test]
fn saving_without_a_path_is_an_error_that_says_why() {
    let mut e = Editor::empty();
    assert!(e.path.is_none(), "刚启动的编辑器不应该已经有路径");
    let err = e.save().expect_err("没路径却保存成功了，内容到哪去了？");
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::InvalidInput,
        "错误种类不对：{err}"
    );
    assert!(
        err.to_string().contains("路径"),
        "错误里没说清该干什么：{err}"
    );
}

#[test]
fn save_as_with_an_empty_path_is_an_error() {
    let mut e = Editor::empty();
    let err = e.save_as(Path::new("")).expect_err("空路径却保存成功了");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    assert!(e.path.is_none(), "失败的另存为不该把路径改成空");
}

/// 新建与覆盖必须分得出来。静默盖掉别人的文件与新建一个文件在界面上长得一模一样，
/// 而前者会让人丢数据。
#[test]
fn save_as_reports_new_versus_overwrite() {
    let t = Tmp::new("overwrite");
    let fresh = t.path("fresh.txt");
    let mut e = Editor::empty();
    let saved = e.save_as(&fresh).expect("另存为");
    assert!(!saved.overwrote, "目标文件不存在，却报了覆盖");
    assert_eq!(saved.path, fresh);
    assert_eq!(saved.bytes, fs::metadata(&fresh).unwrap().len() as usize);
    assert!(
        saved.message().contains("新建"),
        "消息里没说是新建：{}",
        saved.message()
    );

    let again = e.save_as(&fresh).expect("再保存一次");
    assert!(
        again.overwrote,
        "文件已经在了，却没报覆盖（这一侧不能红就等于没有这个字段）"
    );
    assert!(
        again.message().contains("覆盖"),
        "消息里没说覆盖：{}",
        again.message()
    );
    assert_ne!(
        saved.message(),
        again.message(),
        "两种情形的提示完全一样，那个字段就是装饰"
    );
}

#[test]
fn save_as_writes_the_real_bytes_and_clears_the_dirty_flag() {
    let t = Tmp::new("bytes");
    let src = t.file("a.txt", "hello\n");
    let mut e = Editor::open(&src).expect("打开");
    e.cursor = Pos::new(0, 5);
    assert!(e.insert_text(", yi"));
    assert!(e.is_dirty());

    let dst = t.path("b.txt");
    e.save_as(&dst).expect("另存为");
    assert_eq!(fs::read_to_string(&dst).unwrap(), "hello, yi\n");
    assert!(!e.is_dirty(), "另存为之后还挂着未保存标记");
    assert_eq!(
        e.path.as_deref(),
        Some(dst.as_path()),
        "后续的 Ctrl+S 会写回旧文件"
    );
    // 原文件不能被动过。
    assert_eq!(
        fs::read_to_string(&src).unwrap(),
        "hello\n",
        "另存为却改了原文件"
    );
    assert_eq!(e.status_bar().name, "b.txt", "状态栏还写着旧文件名");
}

/// 承重：另存为换了扩展名，语言与高亮缓存都要跟上。
/// **两侧都断**：同语言另存不得白白丢掉缓存（否则大文件每次 Ctrl+S 都要重算）。
#[test]
fn save_as_redetects_the_language_and_drops_the_stale_highlight_cache() {
    let t = Tmp::new("lang");
    let src = t.file("note.md", "# 标题\nfn main() {}\nlet x = 1;\nstruct S;\n");
    let mut e = Editor::open(&src).expect("打开");
    assert_eq!(e.lang, Lang::Markdown, "按扩展名认语言就错了");
    let _ = e.state_at(3);
    assert!(
        e.cached_state_count() >= 4,
        "高亮缓存没暖起来（{}），下面那条失效断言在测空气",
        e.cached_state_count()
    );

    let dst = t.path("code.rs");
    e.save_as(&dst).expect("另存为");
    assert_eq!(
        e.lang,
        Lang::Rust,
        "另存成 .rs 之后语言没变，颜色会一直停在 Markdown 上"
    );
    assert!(
        e.cached_state_count() <= 1,
        "语言变了却留着 {} 条旧语言的高亮状态",
        e.cached_state_count()
    );
    assert_eq!(e.status_bar().lang, Lang::Rust, "状态栏里的语言没跟上");

    // 对照侧：同语言另存不该丢缓存。只验上一侧的话，一个每次保存都清空缓存的实现也能完美交差。
    let _ = e.state_at(3);
    let warm = e.cached_state_count();
    assert!(warm >= 4);
    e.save_as(&t.path("code2.rs")).expect("同语言另存");
    assert_eq!(
        e.cached_state_count(),
        warm,
        "语言没变却丢了高亮缓存，大文件上每次 Ctrl+S 都要重算一遍"
    );
}

/// 新建必须是一份干净的文档。只把文本清空的话，上一份文档的撤销栈会留着，
/// 于是一下 Ctrl+Z 能把新文档「撤」回上一个文件的内容 —— 不报错，但人会以为自己敲错了。
#[test]
fn new_file_does_not_carry_the_previous_document_or_its_undo_stack() {
    let t = Tmp::new("newfile");
    let src = t.file("old.rs", "fn old() {}\n");
    let mut e = Editor::open(&src).expect("打开");
    e.cursor = Pos::new(0, 0);
    assert!(e.insert_text("// 改一下\n"));
    assert!(e.can_undo(), "夹具自证：这时候本来就应该有撤销栈");

    e.new_file();
    assert!(
        e.path.is_none(),
        "新文件却留着旧路径，Ctrl+S 会直接覆盖旧文件"
    );
    assert_eq!(e.line_count(), 1, "新文件不是空的");
    assert_eq!(e.line(0), "");
    assert!(!e.is_dirty(), "刚新建就挂着未保存标记");
    assert!(!e.can_undo(), "新文件里能撤销到上一个文件的内容");
    assert!(!e.can_redo());
    assert_eq!(e.undo(), None, "undo 居然还能动");
    assert_eq!(e.cursor, Pos::new(0, 0));
    assert!(e.selection().is_none());
    assert_eq!(e.lang, Lang::PlainText, "新文件还沿用着上一个文件的语言");
    assert_eq!(e.cached_state_count(), 0, "新文件却留着旧文档的高亮状态");
    assert_eq!(e.status_bar().name, "未命名");
    // 磁盘上的旧文件一个字节也不能变。
    assert_eq!(fs::read_to_string(&src).unwrap(), "fn old() {}\n");
}

/// 只读模式的另存为必须拒给，而且**一个字节也不能写**：
/// 写了一半再报错的话，磁盘上会多出一个看起来像备份的残文件。
#[test]
fn save_as_on_a_read_only_huge_file_refuses_and_writes_nothing() {
    let t = Tmp::new("huge");
    let src = t.file("big.txt", "line1\nline2\nline3\n");
    let mut e = Editor::open_with_threshold(&src, 0).expect("打开");
    assert!(e.is_huge(), "阈值设成 0 却没进大文件模式，这个参数是装饰");

    let dst = t.path("copy.txt");
    let err = e.save_as(&dst).expect_err("只读模式却能另存为");
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::Unsupported,
        "错误种类不对：{err}"
    );
    assert!(!dst.exists(), "报错了却已经建出了目标文件");
    assert_eq!(
        e.path.as_deref(),
        Some(src.as_path()),
        "失败的另存为却把路径改成了新目标"
    );
}
