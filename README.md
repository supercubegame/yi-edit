# Yi Edit

极简跨平台 Rust 代码编辑器，目标是：代码高亮、任意大文件读写保存、快速搜索替换。

当前分支是 V1.0 bootstrap：纯核心已经落地，GUI / 大文件 I/O / 基准截图正在接入。先搭闸门，再接外壳，避免“写完才发现没验过”。

## 本地命令

```bash
cargo test --workspace
cargo fmt --all -- --check
```

## 设计取舍

- `crates/core` 零依赖、纯函数优先，负责搜索、流式替换、行索引、增量高亮、文档编辑和撤销。
- 大文件路径会使用分块读取和原子保存，不把整个文件复制进编辑缓冲区。
- V1 的截图和基准只记录真实测量值，第一轮不拍性能阈值。

## Status

V1.0 is under active construction. The first CI run is the source of truth for compilation and test compatibility.
