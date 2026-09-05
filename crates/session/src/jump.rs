//! 右侧快速跳转面板的映射。纯函数，不碰 UI。
//!
//! 为什么这一小块值得单独抽出来：它是整个面板里唯一会**静默算错**的东西。
//! 点一下跳到第 300 行而实际到了 320 行，不会报错、不会卡死、截图也看不出来，
//! 只会让用户以为是自己记错了位置。
//!
//! **故意不用浮点反算。** `(y / h * n) as usize` 在百万行上会因舍入差一行，
//! 而且往返不一致（line -> y -> line' != line）。这里的做法是：正向用整数算出
//! 每行的像素区间，反向在那个单调序列上二分 —— 于是往返一致是构造上保证的，
//! 不是靠调参数碰对的。（浮点写法在实测语料上真的会差行，有一条对照断言盯着。）

/// 跳转面板的几何。`height_px` 是面板可绘区高度，`lines` 是文件总行数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JumpMap {
    height_px: u32,
    lines: usize,
}

impl JumpMap {
    /// 高度为 0 或行数为 0 时返回 None：那种时候面板根本不应该响应点击，
    /// 而不是假装自己能算（除零或全部映到第 0 行）。
    pub fn new(height_px: u32, lines: usize) -> Option<Self> {
        if height_px == 0 || lines == 0 {
            return None;
        }
        Some(Self { height_px, lines })
    }

    pub fn height_px(self) -> u32 {
        self.height_px
    }

    pub fn lines(self) -> usize {
        self.lines
    }

    /// 第 `line` 行在面板上的像素区间 `[top, bottom)`。全整数运算。
    ///
    /// 区间制（而不是单个 y）是故意的：行数比像素多时多行会挤在同一行像素上，
    /// 那时候 `top == bottom`，而这一点必须能被表达出来而不是偷偷舍入。
    pub fn line_band(self, line: usize) -> Option<(u32, u32)> {
        if line >= self.lines {
            return None;
        }
        let h = self.height_px as u64;
        let n = self.lines as u64;
        let top = (line as u64 * h / n) as u32;
        let bottom = ((line as u64 + 1) * h / n) as u32;
        Some((top, bottom))
    }

    /// 面板上第 `y` 行像素对应哪一行。在单调的 band 序列上二分。
    ///
    /// **顶部与底部两个像素显式夹到文首 / 文末。** 这不是为了让断言变绿，
    /// 而是一个真 bug：行数比像素多时（百万行文件的常态），第 0 行的像素带是**空的**
    /// （top == bottom == 0），于是二分会跳过它 —— h=800 n=1000 时点最顶端到第 1 行，
    /// h=1 n=1000 时直接到第 999 行。拖到顶应该到文首、拖到底应该到文末，
    /// 这本来就是缩略图的语义。
    ///
    /// h >= n 时这两个夹都是**空操作**（那时候二分本来就会给出同样的结果），
    /// 所以往返一致不受影响。测试里钉了这一点。
    pub fn line_at(self, y: u32) -> usize {
        if y == 0 {
            return 0;
        }
        if y >= self.height_px - 1 {
            return self.lines - 1;
        }
        let h = self.height_px as u64;
        let n = self.lines as u64;
        // 找最小的 line 使得 bottom(line) > y。bottom 单调不减，所以可二分。
        let mut lo = 0u64;
        let mut hi = n - 1;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let bottom = (mid + 1) * h / n;
            if bottom > y as u64 {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        lo as usize
    }

    /// 可见窗口在面板上的高亮区间 `[top, bottom)`。
    /// 至少一像素高：否则在百万行文件上那个指示器会完全消失，而看起来像没实现。
    pub fn viewport_band(self, first_line: usize, visible_lines: usize) -> (u32, u32) {
        let first = first_line.min(self.lines.saturating_sub(1));
        let last = (first + visible_lines.max(1)).min(self.lines);
        let top = self.line_band(first).map(|(t, _)| t).unwrap_or(0);
        let bottom = self
            .line_band(last.saturating_sub(1))
            .map(|(_, b)| b)
            .unwrap_or(self.height_px);
        (top, bottom.max(top + 1).min(self.height_px))
    }
}
