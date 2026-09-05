//! 流式搜索：按块喂数据，吐绝对偏移。
//!
//! 为什么不在 fileio 里直接循环调 `find_all`：跨块边界的处理（保留多少尾部、
//! 整词匹配需要多保留一个右边界字节、已输出区要留一个左上下文字节）是这个项目里
//! 最容易写错且最不会喊的一块。写两份就会各自长歪，所以搜索与替换共用同一套
//! pending/limit 不变量（见 replace.rs），且两边都有「对每种块大小等于整缓冲区」的断言。

use crate::search::{self, SearchOptions};

/// 已输出区保留的左上下文字节数。整词匹配判左边界靠它；不保留的话，每块的第 0 字节
/// 都会被当成词首，于是一个被切开的单词会被误判成整词匹配。
const CTX: usize = 1;

#[derive(Debug, Clone)]
pub struct StreamSearcher {
    needle: Vec<u8>,
    opts: SearchOptions,
    pending: Vec<u8>,
    /// pending[0] 对应的绝对偏移。
    base: usize,
    /// 下一次扫描的绝对起点（上一个匹配的末端，保证匹配不重叠）。
    next_scan: usize,
    hits: usize,
}

impl StreamSearcher {
    pub fn new(needle: &[u8], opts: SearchOptions) -> Option<Self> {
        if needle.is_empty() {
            return None;
        }
        Some(Self {
            needle: needle.to_vec(),
            opts,
            pending: Vec::new(),
            base: 0,
            next_scan: 0,
            hits: 0,
        })
    }

    pub fn hits(&self) -> usize {
        self.hits
    }

    fn hold(&self) -> usize {
        self.needle.len() - 1 + usize::from(self.opts.whole_word)
    }

    /// 喂一块，返回本次能确定的匹配绝对偏移。
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<usize> {
        self.pending.extend_from_slice(chunk);
        self.process(false)
    }

    /// 没数据了，把尾巴里剩下的匹配吐完。
    pub fn finish(&mut self) -> Vec<usize> {
        self.process(true)
    }

    fn process(&mut self, flush: bool) -> Vec<usize> {
        let hold = if flush { 0 } else { self.hold() };
        if self.pending.len() < hold {
            return Vec::new();
        }
        // 只接受起点绝对偏移 < limit 的匹配：再往后的匹配可能还没收全字节，
        // 整词匹配还差一个右边界字节。
        let limit = self.base + self.pending.len() - hold;
        let mut out = Vec::new();
        loop {
            let from = self.next_scan.saturating_sub(self.base);
            let Some(p) = search::find_from(&self.pending, &self.needle, from, self.opts) else {
                break;
            };
            let abs = self.base + p;
            if abs >= limit {
                break;
            }
            out.push(abs);
            self.hits += 1;
            self.next_scan = abs + self.needle.len();
        }
        let keep_abs = self.next_scan.max(limit).saturating_sub(CTX);
        let drop_to = keep_abs.saturating_sub(self.base).min(self.pending.len());
        self.pending.drain(..drop_to);
        self.base += drop_to;
        out
    }
}
