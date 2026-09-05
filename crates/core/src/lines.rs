//! 行索引：从字节里找出每行的起点。
//!
//! 大文件模式下不整读文件，索引是**分块**喂进来的，所以除了 `build` 还有
//! `extend` / `from_starts` 两个入口。「分块构建的结果必须等于整块构建的结果」
//! 这条在 tests/lines.rs 里对所有块大小做了断言。

use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineIndex {
    starts: Vec<usize>,
    len: usize,
}

impl Default for LineIndex {
    fn default() -> Self {
        Self {
            starts: vec![0],
            len: 0,
        }
    }
}

impl LineIndex {
    /// 整块构建。空输入也有一行（空行），这是编辑器的通行语义。
    pub fn build(bytes: &[u8]) -> Self {
        let mut starts = vec![0usize];
        Self::extend(&mut starts, bytes, 0);
        Self {
            starts,
            len: bytes.len(),
        }
    }

    /// 分块构建：把 chunk 里的行首（绝对偏移 = base + i + 1）追加进 starts。
    /// starts 必须已经含有 0（第 0 行的起点），否则后面所有区间都会错位一行。
    pub fn extend(starts: &mut Vec<usize>, chunk: &[u8], base: usize) {
        debug_assert!(!starts.is_empty(), "starts 必须以 0 开头");
        for (i, b) in chunk.iter().enumerate() {
            if *b == b'\n' {
                starts.push(base + i + 1);
            }
        }
    }

    pub fn from_starts(starts: Vec<usize>, len: usize) -> Self {
        debug_assert_eq!(starts.first().copied(), Some(0), "starts 必须以 0 开头");
        Self { starts, len }
    }

    pub fn len_bytes(&self) -> usize {
        self.len
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    pub fn starts(&self) -> &[usize] {
        &self.starts
    }

    /// 第 line 行的字节区间，**含**行尾换行符。
    pub fn line_span(&self, line: usize) -> Option<Range<usize>> {
        let start = *self.starts.get(line)?;
        let end = self.starts.get(line + 1).copied().unwrap_or(self.len);
        Some(start..end.max(start))
    }

    /// 把 \n 和 \r\n 从区间末尾去掉。渲染一行时用这个。
    pub fn trim_eol(bytes: &[u8], span: Range<usize>) -> Range<usize> {
        let mut end = span.end.min(bytes.len());
        let start = span.start.min(end);
        if end > start && bytes[end - 1] == b'\n' {
            end -= 1;
        }
        if end > start && bytes[end - 1] == b'\r' {
            end -= 1;
        }
        start..end
    }

    /// 偏移落在第几行。二分查找，O(log n)。
    pub fn line_of_offset(&self, off: usize) -> usize {
        match self.starts.binary_search(&off) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }
}
