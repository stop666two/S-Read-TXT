//! 查找历史：最近查询词的轻量持久化。
//!
//! 存储：`data/find-history.json`（便携模式；schemaVersion 独立于设置版本）。
//! 规则：
//! - 最新查询在最前；同文本去重（移动式）；
//! - 条数上限来自设置 `app.find.historyLimit`（0 = 不留历史，内存与磁盘均不保存）；
//! - 单条按字符截断到 [`ITEM_MAX_CHARS`]；空白查询不记录；
//! - 文件损坏/缺失回退空列表（不阻塞查找功能）。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 历史文件名（数据目录下）。
pub const FIND_HISTORY_FILE_NAME: &str = "find-history.json";

/// 单条最大字符数（超长查询截断保存）。
pub const ITEM_MAX_CHARS: usize = 512;

/// 文件 schema 版本（与设置版本无关）。
const HISTORY_SCHEMA_VERSION: u32 = 1;

/// 磁盘结构（`find-history.json`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FindHistoryFile {
    /// 文件格式版本
    schema_version: u32,
    /// 查询词列表（最新在前）
    entries: Vec<String>,
}

impl Default for FindHistoryFile {
    fn default() -> Self {
        Self {
            schema_version: HISTORY_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

/// 历史文件路径。
pub fn find_history_path(dir: &Path) -> PathBuf {
    dir.join(FIND_HISTORY_FILE_NAME)
}

/// 读取历史（缺失/损坏/版本不符 → 空）。
pub fn load(dir: &Path) -> Vec<String> {
    let path = find_history_path(dir);
    let file: FindHistoryFile =
        crate::storage::json_io::load_json_or_default(&path, normalize_entries);
    file.entries
}

/// 覆盖写入历史（原子写）。
pub fn store(dir: &Path, entries: &[String]) -> std::io::Result<()> {
    let file = FindHistoryFile {
        schema_version: HISTORY_SCHEMA_VERSION,
        entries: entries.to_vec(),
    };
    crate::storage::json_io::write_json_atomic(&find_history_path(dir), &file)
}

/// 归一：去空白、逐项截断、保序去重（最新在前语义由写入方维护）。
fn normalize_entries(file: &mut FindHistoryFile) {
    file.schema_version = HISTORY_SCHEMA_VERSION;
    let mut seen: Vec<String> = Vec::new();
    for item in std::mem::take(&mut file.entries) {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let clipped: String = trimmed.chars().take(ITEM_MAX_CHARS).collect();
        if seen.iter().any(|existing| existing == &clipped) {
            continue;
        }
        seen.push(clipped);
    }
    file.entries = seen;
    // 防御：磁盘异常超大时按 IN_MEMORY_CAP 截断（正常上限由设置层控制）。
    if file.entries.len() > IN_MEMORY_CAP {
        log::warn!("查找历史条目异常过多（> {IN_MEMORY_CAP}），已截断");
        file.entries.truncate(IN_MEMORY_CAP);
    }
}

/// 内存防御上限（设置上限更大时按设置写入，但读取侧兜底）。
const IN_MEMORY_CAP: usize = 1_000;

/// 追加一条查询（最新在前、去重、按 `limit` 截断）；`limit == 0` 时不记录。
pub fn add(dir: &Path, limit: u32, query: &str) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return load(dir);
    }
    let clipped: String = trimmed.chars().take(ITEM_MAX_CHARS).collect();
    let mut entries = load(dir);
    entries.retain(|existing| existing != &clipped);
    entries.insert(0, clipped);
    entries.truncate(limit as usize);
    if let Err(error) = store(dir, &entries) {
        log::warn!("查找历史写入失败：{error}");
    }
    entries
}

/// 读取历史（`limit == 0` 返回空；否则按上限截断返回）。
pub fn list(dir: &Path, limit: u32) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let mut entries = load(dir);
    entries.truncate(limit as usize);
    entries
}

/// 清空历史（落盘空列表；`limit == 0` 时也移除残留文件内容）。
pub fn clear(dir: &Path) -> Vec<String> {
    if let Err(error) = store(dir, &[]) {
        log::warn!("查找历史清空失败：{error}");
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 追加：最新在前、去重移动、上限截断。
    #[test]
    fn add_moves_latest_to_front_and_caps() {
        let dir = tempfile::tempdir().expect("临时目录失败");
        add(dir.path(), 3, "alpha");
        add(dir.path(), 3, "beta");
        add(dir.path(), 3, "alpha");
        let entries = add(dir.path(), 3, "gamma");
        assert_eq!(entries, vec!["gamma", "alpha", "beta"]);
        let entries = add(dir.path(), 3, "delta");
        assert_eq!(entries, vec!["delta", "gamma", "alpha"]);
    }

    /// 上限 0：不记录、不读取。
    #[test]
    fn zero_limit_disables_history() {
        let dir = tempfile::tempdir().expect("临时目录失败");
        assert!(add(dir.path(), 0, "alpha").is_empty());
        assert!(list(dir.path(), 0).is_empty());
        assert!(!find_history_path(dir.path()).exists());
    }

    /// 损坏文件回退空；空白查询不记录但返回现有列表。
    #[test]
    fn corrupt_file_falls_back_and_blank_query_is_ignored() {
        let dir = tempfile::tempdir().expect("临时目录失败");
        std::fs::write(find_history_path(dir.path()), b"{ broken").expect("写损坏文件失败");
        assert!(load(dir.path()).is_empty());
        add(dir.path(), 10, "keep");
        let entries = add(dir.path(), 10, "   ");
        assert_eq!(entries, vec!["keep"]);
    }

    /// 超长查询截断保存；清空移除内容。
    #[test]
    fn long_query_is_clipped_and_clear_empties() {
        let dir = tempfile::tempdir().expect("临时目录失败");
        let long = "x".repeat(ITEM_MAX_CHARS + 50);
        let entries = add(dir.path(), 10, &long);
        assert_eq!(entries[0].chars().count(), ITEM_MAX_CHARS);
        assert!(clear(dir.path()).is_empty());
        assert!(list(dir.path(), 10).is_empty());
    }
}
