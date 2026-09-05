//! 界面布局里的耦合参数。这些数字改一个必须重算另一个，而写在注释里的
//! 那句「改一个必须重算另一个」自己会腐化。

use yi_edit_meta as meta;

fn theme_const(name: &str) -> f64 {
    let src = meta::read("crates/app/src/theme.rs");
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with('#') || t.starts_with("//") {
            continue;
        }
        let Some(rest) = t.strip_prefix("pub const ") else {
            continue;
        };
        let Some((decl, value)) = rest.split_once('=') else {
            continue;
        };
        let ident = decl.split(':').next().unwrap_or_default().trim();
        if ident != name {
            continue;
        }
        let v = value.trim().trim_end_matches(';').trim();
        return v
            .parse()
            .unwrap_or_else(|_| panic!("{name} 的值不是一个纯数字：{v:?}"));
    }
    panic!("theme.rs 里找不到 {name}");
}

/// 行号栏宽度必须装得下行号位数。装不下的表现是行号与正文叠在一起，
/// 而那一点只有看截图才发现得了 —— 所以它是一条断言而不是一句注释。
/// 0.65 是等宽字体宽高比的保守上界，14 是左右内边距。
#[test]
fn the_gutter_is_wide_enough_for_the_line_numbers() {
    let gutter = theme_const("GUTTER_W");
    let digits = theme_const("LINE_NO_DIGITS");
    let font = theme_const("FONT_SIZE");
    let needed = digits * font * 0.65 + 14.0;
    assert!(
        gutter >= needed,
        "行号栏 {gutter}px 装不下 {digits} 位行号（字号 {font}，至少需要 {needed:.1}px）"
    );
    // 反向：也不能宽得荒谬，否则正文区白白少一大块。
    assert!(
        gutter <= needed * 2.0,
        "行号栏 {gutter}px 远超必要宽度 {needed:.1}px，正文区被白白吃掉一大块"
    );
    // 行号位数要能装下百万行（基准里的 64MB 文件是 1,040,619 行，七位）。
    // 这条现在就应该红：它提醒的是一个真存在的局限，不是装饰。
    assert!(digits >= 6.0, "行号只留了 {digits} 位");
}

/// 侧边栏与跳转面板加起来不能吃掉最小窗口宽度的一半，
/// 否则把窗口缩到最小时编辑区会只剩一条缝。最小宽度写在 main.rs 里。
#[test]
fn the_side_panels_leave_room_for_the_editor() {
    let sidebar = theme_const("SIDEBAR_W");
    let jump = theme_const("JUMP_W");
    let gutter = theme_const("GUTTER_W");
    let main = meta::read("crates/app/src/main.rs");
    let min_w = main
        .lines()
        .find(|l| l.contains("with_min_inner_size"))
        .and_then(|l| {
            let start = l.find('[')? + 1;
            let end = l[start..].find(',')? + start;
            l[start..end].trim().parse::<f64>().ok()
        })
        .expect("main.rs 里读不到最小窗口宽度");
    let chrome = sidebar + jump + gutter;
    assert!(
        chrome < min_w / 2.0,
        "侧边栏+跳转面板+行号栏 = {chrome}px，超过最小窗口 {min_w}px 的一半，编辑区会只剩一条缝"
    );
}

/// 工具栏 / 查找栏 / 状态栏 高度加起来不能吃掉最小窗口高度的一半。
#[test]
fn the_bars_leave_room_for_the_text() {
    let bars = theme_const("TOOLBAR_H") + theme_const("FINDBAR_H") + theme_const("STATUS_H");
    let main = meta::read("crates/app/src/main.rs");
    let min_h = main
        .lines()
        .find(|l| l.contains("with_min_inner_size"))
        .and_then(|l| {
            let start = l.find(',')? + 1;
            let end = l[start..].find(']')? + start;
            l[start..end].trim().parse::<f64>().ok()
        })
        .expect("main.rs 里读不到最小窗口高度");
    assert!(
        bars < min_h / 2.0,
        "三根栏共 {bars}px，超过最小窗口 {min_h}px 的一半"
    );
}

/// **行距必须在 show_rows 之前清零。** 这就是底部留白那个 bug。
/// 写在闭包内部的话，egui 已经按「行高 + 默认行距」算完了几何，
/// 预留高度比真正画出来的多 可见行数 × 行距。
#[test]
fn the_row_spacing_is_zeroed_before_show_rows() {
    let src = meta::read("crates/app/src/ui.rs");
    let code: Vec<(usize, &str)> = src
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim_start().starts_with("//"))
        .collect();
    let zero = code
        .iter()
        .find(|(_, l)| l.contains("item_spacing.y = 0.0"))
        .map(|(i, _)| *i)
        .expect("ui.rs 里没有把行距清零，底部会出现留白");
    let rows = code
        .iter()
        .find(|(_, l)| l.contains("show_rows"))
        .map(|(i, _)| *i)
        .expect("ui.rs 里没有 show_rows");
    assert!(
        zero < rows,
        "行距清零（行 {}）写在了 show_rows（行 {}）后面，egui 已经按默认行距算完几何了",
        zero + 1,
        rows + 1
    );
}

/// 横向滚动要真的能滚：行宽不得直接取视口宽。
/// 取 available_width() 的话内容永远不会比视口宽，超长行被裁掉且滚不过去。
#[test]
fn the_row_width_reflects_content_not_the_viewport() {
    let src = meta::read("crates/app/src/ui.rs");
    let hits = meta::hits_in_code(&src, "row_w");
    assert!(
        hits.len() >= 3,
        "ui.rs 里只有 {} 处提到 row_w（应该是计算 + 传参 + 使用）：{hits:?}",
        hits.len()
    );
    // 负向：画行时不得再去问视口宽度。
    let draw_row = src
        .split("fn draw_row")
        .nth(1)
        .expect("ui.rs 里没有 draw_row");
    let body = draw_row.split("\n    fn ").next().unwrap_or(draw_row);
    let bad = meta::hits_in_code(body, "available_width");
    assert!(
        bad.is_empty(),
        "draw_row 里又去问视口宽度了，横向滚动会变回假的：{bad:?}"
    );
}

/// 新增的三块面板都得真的接在 UI 里，而不是只在 session 里写了逻辑。
/// 否则那些断言全绿而界面上什么都没有。
#[test]
fn the_new_panels_are_actually_wired_into_the_ui() {
    let src = meta::read("crates/app/src/ui.rs");
    for (needle, what) in [
        ("fn toolbar", "顶部工具栏"),
        ("fn sidebar", "左侧文件面板"),
        ("fn jump_panel", "右侧快速跳转面板"),
        ("fn status_bar", "底部状态栏"),
    ] {
        assert!(src.contains(needle), "{what}（{needle}）没接进 UI");
    }
    // 跳转面板必须用 session 里那份映射，不得在 UI 里自己再算一遍。
    assert!(
        src.contains("JumpMap"),
        "跳转面板没用 session 的 JumpMap，那部分映射断言就守不到它"
    );
    let bad = meta::hits_in_code(&src, "as f64 / ");
    assert!(
        bad.is_empty(),
        "ui.rs 里出现了浮点除法，跳转映射可能又在 UI 里重算了一遍：{bad:?}"
    );
}
