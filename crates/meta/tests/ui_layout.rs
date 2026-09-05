//! 界面布局里的耦合参数，以及「那个交付阻断项真的接上了」。
//! 这些数字改一个必须重算另一个，而写在注释里的那句「改一个必须重算另一个」
//! 自己会腐化。

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

/// 找对某个字段的**真赋值**。
///
/// 为什么不能直接找「字段名 + 空格 + 等号」：实测踩过 —— Rust 的 match 守卫
/// `if self.editor_has_keys => ...` 里那个 `=>` 也含等号，于是五个守卫被当成了
/// 五处赋值，断言假红。这就是「子串存在但不在该在的位置上」那个形状。
fn assignments_to(src: &str, field: &str) -> Vec<String> {
    let needle = format!("{field} =");
    meta::hits_in_code(src, &needle)
        .into_iter()
        .filter(|(_, text)| {
            // 同一行可能既有守卫也有赋值，所以逐个出现位置看下一个字符，
            // 而不是简单地把含 `=>` 的行整行扇掉。
            let bytes = text.as_bytes();
            let mut from = 0usize;
            while let Some(rel) = text[from..].find(&needle) {
                let eq = from + rel + needle.len() - 1;
                match bytes.get(eq + 1) {
                    // `=>` 是 match 守卫，`==` 是比较，都不是赋值。
                    Some(b'>') | Some(b'=') => from = eq + 1,
                    _ => return true,
                }
            }
            false
        })
        .map(|(line, text)| format!("{line}: {text}"))
        .collect()
}

/// 扫描器自证：真赋值必须被拓到，match 守卫与比较必须被放过。
/// 只验一侧的话，一个永远返回空列表的扫描器也能完美交差。
#[test]
fn the_assignment_scanner_tells_assignments_from_match_guards() {
    let real = "fn f() { self.flag = true; }\n";
    assert_eq!(
        assignments_to(real, "flag").len(),
        1,
        "真赋值没被拓到（漏报）"
    );
    let guard = "match e { E::A if self.flag => g(), _ => {} }\n";
    assert!(
        assignments_to(guard, "flag").is_empty(),
        "match 守卫的 => 被当成了赋值（误报）"
    );
    let cmp = "if self.flag == other { g(); }\n";
    assert!(
        assignments_to(cmp, "flag").is_empty(),
        "比较被当成了赋值（误报）"
    );
    // 同一行既有守卫又有赋值：不能因为看到 `=>` 就整行扇掉。
    let both = "E::A if self.flag => { self.flag = false; }\n";
    assert_eq!(
        assignments_to(both, "flag").len(),
        1,
        "同一行里的真赋值被守卫遮掉了（漏报）"
    );
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

/// 跳转面板的缩略图必须**按行**画。
///
/// 实测过的 bug：按像素采样（`step = h / 2`）时，13 行的文件里同一行会被画
/// 二十多遍叠成一个实心块 —— 就是用户截图里右上角那个蓝块。
#[test]
fn the_jump_thumbnail_iterates_lines_not_pixels() {
    let src = meta::read("crates/app/src/ui.rs");
    let panel = src
        .split("fn jump_panel")
        .nth(1)
        .expect("ui.rs 里没有 jump_panel");
    let body = panel.split("\n    fn ").next().unwrap_or(panel);
    // 负向：不得再出现把面板高度当循环次数的写法。
    for bad in ["h / 2", "0..step"] {
        let hits = meta::hits_in_code(body, bad);
        assert!(
            hits.is_empty(),
            "缩略图又回到按像素采样了（{bad}），短文件里会叠成实心块：{hits:?}"
        );
    }
    // 正向：循环变量必须是行号，且步长由行数与高度的比值定（行多于像素时才降级）。
    assert!(
        body.contains("while line_no < lines"),
        "缩略图没按行遍历"
    );
    assert!(
        body.contains("lines / h"),
        "步长没根据行数与面板高度的比值定，百万行文件会画一百万条线"
    );
}

/// **剪贴板三个事件都得接。** egui 把 Ctrl+C/V/X 转成 Event::Copy/Paste/Cut，
/// 不接的话快捷键完全没反应 —— 而一个不能粘贴的编辑器不能交付。
/// 这条只能验「接上了」；粘贴的真实行为在 session 那层有断言。
#[test]
fn the_clipboard_events_are_all_handled() {
    let src = meta::read("crates/app/src/ui.rs");
    for (needle, what) in [
        ("Event::Copy", "复制"),
        ("Event::Cut", "剪切"),
        ("Event::Paste", "粘贴"),
    ] {
        let hits = meta::hits_in_code(&src, needle);
        assert!(!hits.is_empty(), "{what}（{needle}）没接，快捷键会完全没反应");
    }
    // 复制与剪切得真的写进系统剪贴板，否则它们只是把文本拿出来丢掉了。
    let copy_out = meta::hits_in_code(&src, "copy_text");
    assert!(
        !copy_out.is_empty(),
        "拿到选区文本却没写进剪贴板（没有 copy_text）"
    );
    // 全选也得有，否则 Ctrl+A 会落到文本插入那条路上。
    assert!(
        !meta::hits_in_code(&src, "select_all").is_empty(),
        "Ctrl+A 全选没接"
    );
}

/// **关窗拦截必须存在，而且截图模式要能绕过它。**
/// 不拦的话直接叉掉就丢改动（数据丢失级）；
/// 不绕过的话 CI 里那一帧会被对话框卡住，而那会表现成「截图超时」。
#[test]
fn closing_with_unsaved_changes_is_intercepted_and_screenshots_bypass_it() {
    let src = meta::read("crates/app/src/ui.rs");
    for (needle, what) in [
        ("close_requested", "关窗请求的检测"),
        ("CancelClose", "取消关窗"),
        ("is_dirty", "脏标记判断"),
    ] {
        let hits = meta::hits_in_code(&src, needle);
        assert!(!hits.is_empty(), "{what}（{needle}）没写，关窗会静默丢掉修改");
    }
    // 截图模式的旁路：这一条不是为了好看，是为了不把自己的闸门卡死。
    assert!(
        !meta::hits_in_code(&src, "shot.active()").is_empty(),
        "关窗拦截没给截图模式留旁路，CI 里会被对话框卡住"
    );
    // 保存失败就不能退：否则「保存并退出」在保存失败时等于静默丢掉修改，
    // 而那正是这个对话框要防的事。
    let dialog = src
        .split("fn close_dialog")
        .nth(1)
        .expect("ui.rs 里没有 close_dialog");
    let body = dialog.split("\n    fn ").next().unwrap_or(dialog);
    assert!(
        body.contains("is_dirty"),
        "「保存并退出」没重新核脏标记，保存失败也会直接退"
    );
}

/// 焦点只能有一份真身。
///
/// 实测过的 bug：`editor_has_keys` 在 `handle_keys` **之前**被重置成 true，
/// 而真正的焦点判断在之后绘制时才发生 —— 也就是说在路径框里敲字会同时
/// 往正文里插一份。四个控件各写一遍 `has_focus()` 是同一个问题的另一面。
#[test]
fn keyboard_focus_has_exactly_one_source_of_truth() {
    let src = meta::read("crates/app/src/ui.rs");
    let writes = assignments_to(&src, "editor_has_keys");
    assert_eq!(
        writes.len(),
        2,
        "editor_has_keys 有 {} 处赋值（应该只有初始化与帧末一处）：{writes:?}",
        writes.len()
    );
    // 负向：不得回到「每个输入框自己报焦点」那种写法。
    let scattered = meta::hits_in_code(&src, "has_focus() {");
    assert!(
        scattered.is_empty(),
        "又回到了每个控件各报一遍焦点的写法：{scattered:?}"
    );
}
