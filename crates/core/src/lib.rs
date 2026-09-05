//! Yi Edit 纯核心。
//!
//! 铁律：这个 crate 里不许出现文件 I/O、网络、进程、环境变量、系统时间、未播种的随机。
//! 这条不是靠约定守的 —— crates/meta/tests/purity.rs 里有一个扫描器在守，
//! 而且它会先剥掉注释和字符串字面量再找，所以「在注释里提一下」既不会漏报也不会误报。
//!
//! 这样做的回报很具体：同样输入必然同样输出（可断言）、几万步操作能在毫秒内跑完（可压测）、
//! 同一份逻辑换任何外壳都能复用（GUI / 基准测试 / 未来的命令行都用这一份）。
//! 撤销功能也几乎是白送的：编辑是可逆算子，撤销就是一个算子栈。
#![forbid(unsafe_code)]

pub mod consts;
pub mod edit;
pub mod highlight;
pub mod lines;
pub mod replace;
pub mod search;

pub use consts::{CHUNK_OVERLAP, CHUNK_SIZE, HUGE_FILE_THRESHOLD, MAX_PATTERN_LEN, MAX_UNDO};
pub use edit::{Doc, EditOp, Eol, Pos};
pub use highlight::{highlight_line, lang_from_path, Lang, LineState, Span, TokenKind};
pub use lines::LineIndex;
pub use replace::{replace_all, ReplaceError, StreamReplacer};
pub use search::{find_all, find_before, find_from, SearchOptions};
