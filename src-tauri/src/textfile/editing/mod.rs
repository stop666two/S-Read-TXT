//! 编辑引擎（片表后端，设计文档 §5 / D17–D19）。
//!
//! 模块组成：
//! - [`fenwick`]：片段前缀和（字节 / 换行单元两个维度）
//! - [`piece`]：片段模型与换行单元统计（跨片 CRLF 拆分安全）
//! - [`edit_doc`]：文档视图（行列映射 / 取行 / 编辑 / 撤销重做）
//! - [`save`]：保存链（编码询问流后端 / .bak / 原子写 / 冲突检测）
//! - [`search`]：查找与替换（普通文本 / 大小写开关 / 流式扫描）
//!
//! 行模型（编辑态）：按「逻辑行」计数——换行族（`\n`/`\r\n`/`\r`）分隔；
//! 显示层与只读模式共用 8KB 行内分块（超长逻辑行按显示分段虚拟渲染，
//! 见 [`DISPLAY_SEGMENT_BYTES`]），光标/选区在逻辑行坐标上跨段连续。
//!
//! 内存模型（设计 §5.5）：原文片表零复制（mmap 不动）；新增文本只进
//! 只增缓冲；行数元数据 = 每片段一个 `u64`。

pub mod batch;
pub mod edit_doc;
pub mod fenwick;
pub mod piece;
pub mod save;
pub mod search;

/// 显示分段大小（与只读模式的行内 8KB 分块共用同一常量，保证两种模式视觉一致）。
///
/// 编辑视图对超过该值的逻辑行按此粒度生成显示段（段边界字符对齐）；
/// 光标/选区仍以逻辑行坐标表达，由分段表做双向映射。
pub use crate::textfile::line_index::MAX_ROW_BYTES as DISPLAY_SEGMENT_BYTES;

/// 撤销历史字节上限（新增 + 删除涉及字节数之和；与步数上限同时生效，先到先裁剪）。
pub const UNDO_MAX_BYTES: u64 = 50 * 1024 * 1024;

/// 撤销历史步数上限。
pub const UNDO_MAX_STEPS: usize = 1000;
