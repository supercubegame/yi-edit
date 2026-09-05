//! 编译入口与布局的断言。
//!
//! 这里守的不是「好不好看」（那只能人工验收），而是两件机器分得出来的事：
//! 存不存在第二份 UI 真身，以及 **PR 里承诺的区域是不是真的被画了**。
//! 后者吁得一条断言：`jump.rs` 自己的断言全部通过过，而它的输出从没上屏 ——
//! 一层没人走的逻辑配上一整套绿的断言，看起来比没写还像做完了。

use yi_edit_meta as meta;

fn app_ui_source() -> String {
    let main = meta::read("crates/app/src/main.rs");
    let path = main
        .lines()
        .find_map(|l| {
            l.trim()
                .strip_prefix("#[path = \"")
                .and_then(|s| s.strip_suffix("\"]"))
        })
        .unwrap_or("ui.rs");
    meta::read(&format!("crates/app/src/{path}"))
}

/// 从源码里读一个数字常量。先剥注释与字符串：注释里提到同名常量时会读错行。
fn source_number(rel: &str, name: &str) -> f64 {
    let src = meta::read(rel);
    let code = meta::strip_comments_and_strings(&src);
    let needle = format!("pub const {name}");
    let line = code
        .lines()
        .find(|l| l.trim_start().starts_with(&needle))
        .unwrap_or_else(|| panic!("{rel} 里没有 {name}"));
    let rhs = line
        .split('=')
        .nth(1)
        .unwrap_or_else(|| panic!("{name} 没有赋值：{line}"));
    let digits: String = rhs
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("{name} 的值解不出来：{rhs}"))
}

fn theme(name: &str) -> f64 {
    source_number("crates/app/src/theme.rs", name)
}

/// 最小窗口尺寸。它与面板宽度耦合，而两头分居两个文件 —— 正是最容易各自漂的形状。
fn min_window(axis: usize) -> f64 {
    let src = meta::read("crates/app/src/main.rs");
    let code = meta::strip_comments_and_strings(&src);
    let at = code
        .find("with_min_inner_size")
        .expect("main.rs 里没有 with_min_inner_size，那窗口可以被拘到任意小");
    let open = code[at..].find('[').expect("最小尺寸后面没有 [") + at;
    let close = code[open..].find(']').expect("最小尺寸没闭合") + open;
    let parts: Vec<&str> = code[open + 1..close].split(',').collect();
    let raw = parts
        .get(axis)
        .unwrap_or_else(|| panic!("最小尺寸里没有第 {axis} 项"));
    let digits: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    digits.parse().expect("最小尺寸解不出数")
}

#[test]
fn compiled_entry_has_real_ime_consumer_and_safe_file_panel() {
    let src = app_ui_source();
    assert!(src.contains("Event::Ime") && src.contains("IMERect") && src.contains("fn handle_ime"));
    assert!(
        src.contains("let snapshot = self.listing.clone()"),
        "文件面板必须先 clone 快照，避开闭包借用冲突"
    );
    assert!(src.contains("show_rows") || src.contains("ScrollArea"));
}

#[test]
fn compiled_entry_preserves_delivery_blockers() {
    let src = app_ui_source();
    for needle in [
        "Event::Copy",
        "Event::Cut",
        "Event::Paste",
        "select_all",
        "close_requested",
        "CancelClose",
        "shot.active()",
    ] {
        assert!(src.contains(needle), "compiled UI 缺少 {needle}");
    }
}

#[test]
fn main_points_at_the_runtime_and_legacy_copy_is_not_compiled() {
    let main = meta::read("crates/app/src/main.rs");
    assert!(main.contains("ui_safe.rs"), "main 没有指向 ui_safe.rs");
    assert!(!main.contains("ui_legacy.rs"), "main 不该编译归档副本");
    assert!(
        !meta::exists("crates/app/src/ui.rs"),
        "旧 ui.rs 还在，容易产生第二个真身"
    );
}

/// 四个区域都要真的被画。这一条不看效果，只看「那层纯逻辑的输出有没有被消费」。
#[test]
fn every_promised_region_is_actually_consumed_by_the_runtime() {
    let src = app_ui_source();
    let code = meta::strip_comments_and_strings(&src);
    // 跳转面板：坐标换算必须走 JumpMap，而不是在 UI 里重新拍一份浮点反算。
    for needle in ["JumpMap::new", "line_at", "viewport_band", "line_band"] {
        assert!(
            !meta::hits_in_code(&src, needle).is_empty(),
            "跳转面板没有走 {needle}，那层断言就守着一堆没人看的输出"
        );
    }
    assert!(
        code.contains("fn jump_panel") && code.contains("self.jump_panel(ui)"),
        "跳转面板定义了却没被调用，那与没写等价"
    );
    // 状态栏：三组字段都要上屏，否则 status.rs 里的徒劳。
    for needle in ["position_text", "size_text", "badges"] {
        assert!(
            !meta::hits_in_code(&src, needle).is_empty(),
            "状态栏没有画 {needle}"
        );
    }
    // 查找栏与开关。
    assert!(code.contains("fn find_bar"), "没有查找栏");
    assert!(
        code.contains("show_find") && code.contains("show_sidebar"),
        "两个面板的开关不全"
    );
    // 截断必须在界面上有标记：静默截断与「就这么多命中」长得一模一样。
    assert!(
        src.contains("结果不完整"),
        "命中被截断时界面上没有任何标记"
    );
}

/// 焦点守卫。不问焦点的话，在查找框里敲的每一个字同时会被写进文档，
/// 而这个 bug 不报错：用户搜完一次之后文件里多了一串垃圾。
#[test]
fn typing_into_a_text_field_cannot_reach_the_document() {
    let src = app_ui_source();
    let code = meta::strip_comments_and_strings(&src);
    assert!(
        code.contains("m.focused()"),
        "事件处理里没问过焦点归谁"
    );
    for guarded in ["Event::Text(text) if editor_focused", "Event::Paste(text) if editor_focused"] {
        assert!(
            code.contains(guarded),
            "{guarded} 没有被焦点守卫拦着"
        );
    }
}

#[test]
fn layout_constants_leave_room_for_text() {
    let theme_src = meta::read("crates/app/src/theme.rs");
    assert!(
        theme_src.contains("SIDEBAR_W")
            && theme_src.contains("JUMP_W")
            && theme_src.contains("GUTTER_W")
    );

    // 三个固定宽度加起来不能吃掉一半的最小窗口，否则把窗口拖到最小时
    // 编辑区只剩一条缝。上一版 220+96+72=388 > 720/2，它真的红过。
    let chrome_w = theme("SIDEBAR_W") + theme("JUMP_W") + theme("GUTTER_W");
    let min_w = min_window(0);
    println!("实测：固定宽度合计 {chrome_w}，最小窗口宽 {min_w}");
    assert!(
        chrome_w < min_w / 2.0,
        "侧栏+跳转+行号栈 {chrome_w} 已经吃掉最小窗口（{min_w}）的一半以上"
    );

    // 上下三条栏同理：编辑区在最小窗口下仍要拿到一半以上的高度。
    let chrome_h = theme("TOOLBAR_H") + theme("FINDBAR_H") + theme("STATUS_H");
    let min_h = min_window(1);
    println!("实测：上下栏合计 {chrome_h}，最小窗口高 {min_h}");
    assert!(
        chrome_h < min_h / 2.0,
        "工具栏+查找栏+状态栏 {chrome_h} 吃掉了最小窗口高（{min_h}）的一半以上"
    );
}

/// 行号栈宽度与行号位数的耦合，**两侧都断**：
/// 装不下会叠字，太宽会吃正文区。只断一侧的话，把它改成 500 也一样绿。
#[test]
fn the_gutter_width_and_the_line_number_digits_stay_coupled() {
    let gutter = theme("GUTTER_W");
    let digits = theme("LINE_NO_DIGITS");
    let font = theme("FONT_SIZE");
    // 等宽字体的数字字符宽约为字号的 0.6；右侧留 8px 间距。
    let needed = digits * font * 0.6 + 8.0;
    let too_wide = digits * font * 1.5;
    println!("实测：行号栈 {gutter}，位数 {digits}，字号 {font}，下限 {needed}，上限 {too_wide}");
    assert!(
        gutter >= needed,
        "行号栈 {gutter} 装不下 {digits} 位行号（需要 {needed}），行号会与正文叠在一起"
    );
    assert!(
        gutter <= too_wide,
        "行号栈 {gutter} 远超 {digits} 位行号所需（上限 {too_wide}），那些宽度是从正文区抢的"
    );
}

/// 竖带检查必须真的接进了闸门，而且它的必要性要有一个变异体证明：
/// 右侧整条没画的时候，四条横带与整图指标全部看不见它。
#[test]
fn the_vertical_band_check_is_wired_in_and_proven_necessary() {
    let wf = meta::read(".github/workflows/verify.yml");
    let vbands = wf.matches("--vband").count();
    assert!(
        vbands >= 2,
        "工作流里只有 {vbands} 处竖带，侧栏与跳转面板都要盯"
    );
    let checker = meta::read("crates/app/src/bin/shotcheck.rs");
    assert!(
        checker.contains("Axis::Vertical"),
        "检查器不认识竖带，工作流里那两个参数会直接报错"
    );
    assert!(
        checker.contains("right-blank"),
        "自测里没有「右侧置空」那个变异体，竖带未被证明有必要"
    );
    // 双向：变异体要先让横带那侧判绿，再让竖带判红。
    assert!(
        checker.contains("已确认横带与整图指标看不见右侧置空"),
        "变异体只验了一侧，那证不了竖带多拓到了东西"
    );
    assert!(
        checker.contains("竖带没拓到右侧置空，它是装饰"),
        "竖带那侧的判红自证不在"
    );
}
