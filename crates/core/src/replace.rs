//! 替换。两条路径：整缓冲区的 `replace_all`，和给任意大文件用的 `StreamReplacer`。
//!
//! 流式替换的危险全在**块边界**上：模式横跨两块时会静默丢失匹配，而「少替了一个」
//! 和「文件里本来就只有那些」在输出上一模一样 —— 典型的坏了不会喊。
//! 所以 tests/replace_stream.rs 里的承重断言是：**对每一种块大小**，流式结果必须
//! 逐字节等于整缓冲区结果，替换计数也必须相等。夹具自己还要先自证「真的存在
//! 横跨边界的匹配」，否则那条断言测的是空气。

use crate::consts::MAX_PATTERN_LEN;
use crate::search::{self, SearchOptions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceError {
    EmptyPattern,
    PatternTooLong { len: usize, max: usize },
}

impl std::fmt::Display for ReplaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplaceError::EmptyPattern => write!(f, "搜索内容为空"),
            ReplaceError::PatternTooLong { len, max } => {
                write!(f, "搜索内容太长：{len} 字节，上限 {max}")
            }
        }
    }
}

/// 整缓冲区替换。返回新缓冲区和替换次数。
pub fn replace_all(
    hay: &[u8],
    needle: &[u8],
    repl: &[u8],
    opts: SearchOptions,
) -> (Vec<u8>, usize) {
    if needle.is_empty() {
        return (hay.to_vec(), 0);
    }
    let mut out = Vec::with_capacity(hay.len());
    let mut pos = 0usize;
    let mut count = 0usize;
    while let Some(p) = search::find_from(hay, needle, pos, opts) {
        out.extend_from_slice(&hay[pos..p]);
        out.extend_from_slice(repl);
        pos = p + needle.len();
        count += 1;
    }
    out.extend_from_slice(&hay[pos..]);
    (out, count)
}

/// 为 whole_word 的**左**边界判断保留的上下文字节数。
/// 不保留的话，每块的第 0 字节都会被当成「行首/词首」，于是一个被切开的单词
/// 会被误判成整词匹配。这个坑不会自己喊，靠 tests 里的整词跨块用例抓。
const CTX: usize = 1;

/// 流式替换器：按块喂进去，边出结果边保留必要的尾部。
#[derive(Debug, Clone)]
pub struct StreamReplacer {
    needle: Vec<u8>,
    repl: Vec<u8>,
    opts: SearchOptions,
    pending: Vec<u8>,
    /// pending 开头有多少字节是「已经输出过、只留作上下文」的。
    ctx: usize,
    count: usize,
}

impl StreamReplacer {
    pub fn new(needle: &[u8], repl: &[u8], opts: SearchOptions) -> Result<Self, ReplaceError> {
        if needle.is_empty() {
            return Err(ReplaceError::EmptyPattern);
        }
        if needle.len() > MAX_PATTERN_LEN {
            return Err(ReplaceError::PatternTooLong {
                len: needle.len(),
                max: MAX_PATTERN_LEN,
            });
        }
        Ok(Self {
            needle: needle.to_vec(),
            repl: repl.to_vec(),
            opts,
            pending: Vec::new(),
            ctx: 0,
            count: 0,
        })
    }

    pub fn count(&self) -> usize {
        self.count
    }

    /// 必须保留、不能现在就输出的尾部字节数。
    fn hold(&self) -> usize {
        // 模式可能有 needle.len()-1 个字节已经进来了；整词匹配还要多留 1 个字节判右边界。
        self.needle.len() - 1 + usize::from(self.opts.whole_word)
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(chunk);
        self.process(false)
    }

    /// 冲干净剩下的部分。调用之后这个替换器不该再被 feed。
    pub fn finish(&mut self) -> Vec<u8> {
        self.process(true)
    }

    fn process(&mut self, flush: bool) -> Vec<u8> {
        let hold = if flush { 0 } else { self.hold() };
        if !flush && self.pending.len() < self.ctx + hold + 1 {
            return Vec::new();
        }
        // 只接受起点 < limit 的匹配：起点 >= limit 的匹配可能还没收全。
        let limit = self.pending.len().saturating_sub(hold);
        let mut out = Vec::new();
        let mut pos = self.ctx;
        while let Some(p) = search::find_from(&self.pending, &self.needle, pos, self.opts) {
            if p >= limit {
                break;
            }
            out.extend_from_slice(&self.pending[pos..p]);
            out.extend_from_slice(&self.repl);
            pos = p + self.needle.len();
            self.count += 1;
        }
        // pos 可能已经越过 limit（最后一个匹配跨过了 limit），这时不能再往回输出。
        let emit_to = limit.max(pos).min(self.pending.len());
        if emit_to > pos {
            out.extend_from_slice(&self.pending[pos..emit_to]);
        }
        let keep_from = emit_to.saturating_sub(CTX);
        self.pending.drain(..keep_from);
        self.ctx = emit_to - keep_from;
        out
    }
}
