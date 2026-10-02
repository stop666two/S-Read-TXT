//! 历史条目模型（`history.jsonl` 的单行结构）。
//! 字段以 `docs/configuration.md` §2.5 为准（保持同步）。

use serde::{Deserialize, Serialize};

/// 一条历史记录（同一路径仅保留最新一条，去重在载入时执行）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryEntry {
    /// 文件绝对路径（去重键）
    pub path: String,
    /// 文件名（展示用，避免 UI 再解析路径）
    pub name: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 最近一次打开时的编码
    pub encoding: String,
    /// 最近打开时间（RFC 3339 UTC）
    pub opened_at: String,
    /// 上次阅读的显示行号（续读用）
    pub last_row: u64,
    /// 上次阅读进度（0–100，展示用）
    pub last_percent: f64,
}

impl Default for HistoryEntry {
    fn default() -> Self {
        Self {
            path: String::new(),
            name: String::new(),
            size: 0,
            encoding: String::new(),
            opened_at: String::new(),
            last_row: 0,
            last_percent: 0.0,
        }
    }
}
