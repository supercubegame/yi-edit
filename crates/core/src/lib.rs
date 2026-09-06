//! Yi Edit 纯核心。
//!
//! 铁律：这个 crate 里不许出现文件 I/O、网络、进程、环境变量、系统时间、未播种的随机。
//! 这条不是靠约定守的 —— crates/meta/tests/purity.rs 里有一个扫描器在守，
//! 而且它会先剥掉注释和字符串字面量再找，所以「在注释里提一下」既不会漏报也不会误报。
//!
//! 这条铁律直接影响了撤销的设计：分组不能靠「停顿超过 N 毫秒」（需要系统时间），
//! 只能靠输入内容与位置的结构 —— 而后者恰好是可断言的。
//!
//! 其余回报：同样输入必然同样输出（可断言）、几万步操作能在毫秒内跑完（可压测）、
//! 同一份逻辑换任何外壳都能复用（GUI / 基准测试 / 未来的命令行都用这一份）。
#![forbid(unsafe_code)]

pub mod consts;
pub mod edit;
pub mod elide;
pub mod highlight;
pub mod indent;
pub mod lines;
pub mod replace;
pub mod search;
pub mod stream;

pub use consts::{CHUNK_OVERLAP, CHUNK_SIZE, HUGE_FILE_THRESHOLD, MAX_PATTERN_LEN, MAX_UNDO};
pub use edit::{advance, Doc, EditOp, Eol, Pos, MAX_GROUP_CHARS};
pub use elide::{elide_middle, elide_path, fit_chars, ELLIPSIS};
pub use highlight::{highlight_line, lang_from_path, Lang, LineState, Span, TokenKind};
pub use indent::{
    bracket_pair_at, indent_unit, match_bracket, newline_edit, Mask, NewlineEdit,
    MAX_BRACKET_MATCH_BYTES,
};
pub use lines::LineIndex;
pub use replace::{replace_all, ReplaceError, StreamReplacer};
pub use search::{find_all, find_before, find_from, SearchOptions};
pub use stream::StreamSearcher;
