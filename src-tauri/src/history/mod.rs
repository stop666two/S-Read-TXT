//! history：历史记录存储（`history.jsonl`，每行一条 JSON）。
//!
//! 模块划分：
//! - `entry`：条目模型（字段以 `docs/configuration.md` §2.5 为准）
//! - `store`：追加 / 原子重写 / 载入（去重 + 修剪 + 自愈压缩）
//!
//! 剪枝时机：载入时（应用启动或命令调用）；条数上限与保留天数来自
//! `settings.json` 的 `history` 对象（`maxEntries` / `retentionDays`）。

pub mod entry;
pub mod store;
