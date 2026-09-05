//! 文件层。这里是允许碰 I/O 的一层，所以所有与磁盘有关的东西都往这里放。
//!
//! 三个不能让步的点：
//! 1. **不整读**。超过 `HUGE_FILE_THRESHOLD` 的文件只建行索引 + 按需读窗口。
//! 2. **保存要原子**。先写同目录的临时文件再 rename：写到一半断电不能把原文件搞成半截。
//! 3. **替换计数要真**。少替一个不会报错，只会得到一个看起来很正常的错文件，
//!    所以文件级结果必须与内存级结果逐字节对得上（tests/fileio.rs）。
#![forbid(unsafe_code)]

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use yi_edit_core::{
    replace_all, LineIndex, SearchOptions, StreamReplacer, StreamSearcher, CHUNK_SIZE,
    HUGE_FILE_THRESHOLD,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileInfo {
    pub len: u64,
    pub is_huge: bool,
}

pub fn info(path: &Path) -> io::Result<FileInfo> {
    let len = fs::metadata(path)?.len();
    Ok(FileInfo {
        len,
        is_huge: len > HUGE_FILE_THRESHOLD,
    })
}

pub fn read_all(path: &Path) -> io::Result<Vec<u8>> {
    fs::read(path)
}

/// 读一个窗口（大文件模式下只读可见行就用这个）。
/// 越过文件尾不是错误，返回的字节数可能少于 `len`。
pub fn read_range(path: &Path, start: u64, len: usize) -> io::Result<Vec<u8>> {
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(start))?;
    let mut buf = vec![0u8; len];
    let mut filled = 0usize;
    while filled < len {
        let n = f.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    buf.truncate(filled);
    Ok(buf)
}

/// 分块遍历文件。回调拿到 (chunk, 该块的起始绝对偏移)。
pub fn for_each_chunk<F>(path: &Path, chunk: usize, mut f: F) -> io::Result<u64>
where
    F: FnMut(&[u8], u64),
{
    let mut file = File::open(path)?;
    let chunk = chunk.max(1);
    let mut buf = vec![0u8; chunk];
    let mut base = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        f(&buf[..n], base);
        base += n as u64;
    }
    Ok(base)
}

/// 建行索引。**不整读文件**，只累加行首偏移。
pub fn index_lines(path: &Path) -> io::Result<LineIndex> {
    index_lines_chunked(path, CHUNK_SIZE)
}

pub fn index_lines_chunked(path: &Path, chunk: usize) -> io::Result<LineIndex> {
    let mut starts = vec![0usize];
    let total = for_each_chunk(path, chunk, |part, base| {
        LineIndex::extend(&mut starts, part, base as usize);
    })?;
    Ok(LineIndex::from_starts(starts, total as usize))
}

fn temp_sibling(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| String::from("yi-edit"));
    // 同目录：跳文件系统的 rename 不是原子的，那就不叫原子保存了。
    let mut p = path.to_path_buf();
    p.set_file_name(format!(".{name}.yi-edit-tmp"));
    p
}

/// 原子保存：先写临时文件 + fsync，再 rename 盖回去。
pub fn save_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = temp_sibling(path);
    {
        let f = File::create(&tmp)?;
        let mut w = BufWriter::new(f);
        w.write_all(bytes)?;
        w.flush()?;
        w.into_inner()
            .map_err(|e| io::Error::other(e.to_string()))?
            .sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// 流式搜索整个文件。`limit` 为 0 表示不限。
/// 到达 limit 时返回的第二项为 true（truncated）—— 「只找到这么多」和「只有这么多」
/// 不能长得一模一样，否则界面上的计数就是一条静默的谎。
pub fn find_offsets(
    path: &Path,
    needle: &[u8],
    opts: SearchOptions,
    limit: usize,
) -> io::Result<(Vec<u64>, bool)> {
    find_offsets_chunked(path, needle, opts, limit, CHUNK_SIZE)
}

pub fn find_offsets_chunked(
    path: &Path,
    needle: &[u8],
    opts: SearchOptions,
    limit: usize,
    chunk: usize,
) -> io::Result<(Vec<u64>, bool)> {
    let Some(mut searcher) = StreamSearcher::new(needle, opts) else {
        return Ok((Vec::new(), false));
    };
    let mut out: Vec<u64> = Vec::new();
    let mut truncated = false;
    for_each_chunk(path, chunk, |part, _base| {
        if truncated {
            return;
        }
        for hit in searcher.feed(part) {
            out.push(hit as u64);
            if limit > 0 && out.len() >= limit {
                truncated = true;
                return;
            }
        }
    })?;
    if !truncated {
        for hit in searcher.finish() {
            out.push(hit as u64);
            if limit > 0 && out.len() >= limit {
                truncated = true;
                break;
            }
        }
    }
    Ok((out, truncated))
}

/// 流式替换：src -> dst，返回替换次数。两个路径不能是同一个文件。
pub fn stream_replace(
    src: &Path,
    dst: &Path,
    needle: &[u8],
    repl: &[u8],
    opts: SearchOptions,
) -> io::Result<usize> {
    stream_replace_chunked(src, dst, needle, repl, opts, CHUNK_SIZE)
}

pub fn stream_replace_chunked(
    src: &Path,
    dst: &Path,
    needle: &[u8],
    repl: &[u8],
    opts: SearchOptions,
    chunk: usize,
) -> io::Result<usize> {
    let mut r = StreamReplacer::new(needle, repl, opts)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
    let out = File::create(dst)?;
    let mut w = BufWriter::new(out);
    let mut err: Option<io::Error> = None;
    for_each_chunk(src, chunk, |part, _base| {
        if err.is_some() {
            return;
        }
        if let Err(e) = w.write_all(&r.feed(part)) {
            err = Some(e);
        }
    })?;
    if let Some(e) = err {
        return Err(e);
    }
    w.write_all(&r.finish())?;
    w.flush()?;
    w.into_inner()
        .map_err(|e| io::Error::other(e.to_string()))?
        .sync_all()?;
    Ok(r.count())
}

/// 就地替换：写临时文件再 rename。大文件走这条，不需要把文件读进内存。
pub fn replace_in_place(
    path: &Path,
    needle: &[u8],
    repl: &[u8],
    opts: SearchOptions,
) -> io::Result<usize> {
    let tmp = temp_sibling(path);
    let n = stream_replace(path, &tmp, needle, repl, opts)?;
    fs::rename(&tmp, path)?;
    Ok(n)
}

/// 内存级替换（小文件路径）。摆在这里只为了让测试能拿两条路径直接对拃。
pub fn replace_in_memory(
    path: &Path,
    needle: &[u8],
    repl: &[u8],
    opts: SearchOptions,
) -> io::Result<usize> {
    let bytes = read_all(path)?;
    let (out, n) = replace_all(&bytes, needle, repl, opts);
    save_atomic(path, &out)?;
    Ok(n)
}
