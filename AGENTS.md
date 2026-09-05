# Yi Edit 工作规矩

## 开工顺序

1. 先跑 `./scripts/verify.sh`，它必须返回明确退出码。
2. 先修闸门再修产品：红灯先判断是产品、闸门还是运行环境。
3. 每个新增可验证行为配一条真实断言，不能只断言文件存在。
4. 核心逻辑尽量保持纯净：不碰 I/O、网络、进程、系统时间、未播种随机。
5. 改完必须跑闸门；CI 结果要回写到 PR 或提交评论，没回写就算未确认。

## 结构

- `crates/core`: 零依赖纯核心。
- `crates/meta`: 仓库结构、文档、耦合参数和纯度扫描。
- `crates/fileio`: 分块读写、内存映射、原子保存。
- `crates/app`: egui/eframe GUI。
- `scripts`: 闸门和报告脚本。
- `.github/workflows`: 快闸门、慢闸门和结果回写。

## 核心不变量

- 搜索结果不重叠，合法 UTF-8 的匹配边界必须是字符边界。
- 流式替换对任意块大小必须逐字节等于整缓冲区替换；夹具必须证明确有跨块匹配。
- 高亮 span 铺满整行、不重叠、边界位于字符边界。
- 撤销夹具的操作数必须小于 `MAX_UNDO`，否则测到的是丢弃策略。
- AGENTS.md 与 CLAUDE.md 必须逐字节相同，且不超过 200 行。

## 相互耦合的参数

- `CHUNK_OVERLAP == MAX_PATTERN_LEN - 1`。
- `HUGE_FILE_THRESHOLD % CHUNK_SIZE == 0`。
- `CHUNK_SIZE >= CHUNK_OVERLAP * 16`。
- 搜索块的保留区必须覆盖最长模式，否则边界匹配会静默丢失。

改一个必须重算另一个，并用等号断言钉住，不能只改解释文字。

## 提交前

```bash
./scripts/verify.sh
cargo test --workspace
cargo fmt --all -- --check
```

截图、真实 GPU、三平台安装包和用户体验仍需人工验收；机器闸门不能冒充这部分。

## 记录

已知限制和实测值放在 `docs/PITFALLS.md`，不要把经验档案不断堆进本文件。
