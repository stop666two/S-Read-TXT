//! 剪贴板历史存储：记录复制/剪切历史，供面板展示与回填。
//!
//! 两种模式（由 `editor.clipboard.persist` 决定）：
//! - 持久化开启：读写数据目录 `clipboard-history.json`（原子写）；
//! - 持久化关闭：仅进程内会话内存（退出即失，不落盘）。
//!
//! 规则：
//! - 上限 `editor.clipboard.historyLimit`（0 = 功能禁用：不记录也不返回）；
//! - 新条目置顶；相同文本先去重再置顶；空文本不记录；
//! - 单条超长文本截断至 [`CLIPBOARD_ENTRY_MAX_CHARS`] 字符（避免大对象写入历史文件）；
//! - 列表加载时按「去重（保留最新）+ 截断条数」归一。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::storage::json_io;
use crate::time_util;

/// 历史文件名（数据目录内）。
pub const CLIPBOARD_FILE_NAME: &str = "clipboard-history.json";
/// 单条文本最大存储字符数（超出截断）。
pub const CLIPBOARD_ENTRY_MAX_CHARS: usize = 100_000;
/// 历史文件 schema 版本（独立于设置版本；结构变化时递增）。
const CLIPBOARD_SCHEMA_VERSION: u32 = 1;

/// 剪贴板历史条目（序列化给前端，camelCase）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardEntry {
    /// 条目文本（存储形态）
    pub text: String,
    /// 记录时间（RFC 3339，UTC）
    pub at: String,
}

/// 历史文件结构。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ClipboardFile {
    /// 结构版本
    schema_version: u32,
    /// 条目（置顶为最新）
    entries: Vec<ClipboardEntry>,
}

/// 会话内存（持久化关闭时使用；进程退出即清除）。
static SESSION: Mutex<Vec<ClipboardEntry>> = Mutex::new(Vec::new());

/// 历史文件路径。
pub fn path(dir: &Path) -> PathBuf {
    dir.join(CLIPBOARD_FILE_NAME)
}

/// 归一：去重（同文本保留最新=列表中首次出现者）+ 截断到上限；上限 0 返回空。
fn normalize(entries: Vec<ClipboardEntry>, limit: u32) -> Vec<ClipboardEntry> {
    if limit == 0 {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for entry in entries {
        if entry.text.is_empty() || !seen.insert(entry.text.clone()) {
            continue;
        }
        out.push(entry);
        if out.len() >= limit as usize {
            break;
        }
    }
    out
}

/// 读取文件条目（缺失/损坏时按空处理并记日志）。
fn load_file(dir: &Path) -> Vec<ClipboardEntry> {
    match json_io::read_json_opt::<ClipboardFile>(&path(dir)) {
        Ok(Some(file)) => file.entries,
        Ok(None) => Vec::new(),
        Err(err) => {
            log::warn!(target: "sread::clipboard", "读取剪贴板历史失败，按空处理：{err}");
            Vec::new()
        }
    }
}

/// 原子写文件（失败记日志，不影响返回值语义）。
fn save_file(dir: &Path, entries: &[ClipboardEntry]) {
    let file = ClipboardFile {
        schema_version: CLIPBOARD_SCHEMA_VERSION,
        entries: entries.to_vec(),
    };
    if let Err(err) = json_io::write_json_atomic(&path(dir), &file) {
        log::warn!(target: "sread::clipboard", "写入剪贴板历史失败：{err}");
    }
}

/// 当前原始条目（未归一）：按模式取文件或会话内存。
fn snapshot(dir: &Path, persist: bool) -> Vec<ClipboardEntry> {
    if persist {
        load_file(dir)
    } else {
        SESSION
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

/// 保存条目到对应介质。
fn store(dir: &Path, persist: bool, entries: &[ClipboardEntry]) {
    if persist {
        save_file(dir, entries);
    } else if let Ok(mut guard) = SESSION.lock() {
        *guard = entries.to_vec();
    }
}

/// 当前列表（已归一；上限 0 时为空且不读文件）。
pub fn list(dir: &Path, persist: bool, limit: u32) -> Vec<ClipboardEntry> {
    if limit == 0 {
        return Vec::new();
    }
    normalize(snapshot(dir, persist), limit)
}

/// 追加一条（空文本或禁用时原样返回当前列表）。
pub fn add(dir: &Path, persist: bool, limit: u32, text: &str) -> Vec<ClipboardEntry> {
    if limit == 0 || text.is_empty() {
        return list(dir, persist, limit);
    }
    let text = if text.chars().count() > CLIPBOARD_ENTRY_MAX_CHARS {
        text.chars().take(CLIPBOARD_ENTRY_MAX_CHARS).collect()
    } else {
        text.to_string()
    };
    let entry = ClipboardEntry {
        text,
        at: time_util::now_rfc3339(),
    };
    let mut entries = snapshot(dir, persist);
    entries.retain(|item| item.text != entry.text);
    entries.insert(0, entry);
    let entries = normalize(entries, limit);
    store(dir, persist, &entries);
    entries
}

/// 删除指定下标（越界无操作）。
pub fn remove(dir: &Path, persist: bool, limit: u32, index: usize) -> Vec<ClipboardEntry> {
    let mut entries = list(dir, persist, limit);
    if index < entries.len() {
        entries.remove(index);
    }
    store(dir, persist, &entries);
    entries
}

/// 清空（上限 0 时也清理对应介质中的旧数据）。
pub fn clear(dir: &Path, persist: bool) -> Vec<ClipboardEntry> {
    store(dir, persist, &[]);
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("创建临时目录失败")
    }

    /// 追加 + 列表：新条目置顶，文件落盘。
    #[test]
    fn add_and_list_roundtrip() {
        let tmp = data_dir();
        add(tmp.path(), true, 10, "a");
        add(tmp.path(), true, 10, "b");
        let entries = list(tmp.path(), true, 10);
        assert_eq!(
            entries.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
            vec!["b", "a"]
        );
        assert!(path(tmp.path()).is_file());
        assert!(!entries[0].at.is_empty());
    }

    /// 重复文本去重并置顶。
    #[test]
    fn duplicate_moves_to_top() {
        let tmp = data_dir();
        add(tmp.path(), true, 10, "a");
        add(tmp.path(), true, 10, "b");
        add(tmp.path(), true, 10, "a");
        let entries = list(tmp.path(), true, 10);
        assert_eq!(
            entries.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    /// 超过上限时截断最旧条目。
    #[test]
    fn limit_truncates_oldest() {
        let tmp = data_dir();
        add(tmp.path(), true, 2, "a");
        add(tmp.path(), true, 2, "b");
        add(tmp.path(), true, 2, "c");
        let entries = list(tmp.path(), true, 2);
        assert_eq!(
            entries.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
            vec!["c", "b"]
        );
    }

    /// 上限 0 = 禁用：不记录、不写文件、列表为空。
    #[test]
    fn zero_limit_disables() {
        let tmp = data_dir();
        assert!(add(tmp.path(), true, 0, "a").is_empty());
        assert!(list(tmp.path(), true, 0).is_empty());
        assert!(!path(tmp.path()).exists());
    }

    /// 单条超长文本按字符截断到上限。
    #[test]
    fn long_text_is_truncated() {
        let tmp = data_dir();
        let long = "x".repeat(CLIPBOARD_ENTRY_MAX_CHARS + 5);
        let entries = add(tmp.path(), true, 10, &long);
        assert_eq!(entries[0].text.chars().count(), CLIPBOARD_ENTRY_MAX_CHARS);
    }

    /// 删除与清空。
    #[test]
    fn remove_and_clear() {
        let tmp = data_dir();
        add(tmp.path(), true, 10, "a");
        add(tmp.path(), true, 10, "b");
        let after_remove = remove(tmp.path(), true, 10, 0);
        assert_eq!(
            after_remove
                .iter()
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>(),
            vec!["a"]
        );
        let after_oob = remove(tmp.path(), true, 10, 99);
        assert_eq!(after_oob.len(), 1);
        assert!(clear(tmp.path(), true).is_empty());
        assert!(list(tmp.path(), true, 10).is_empty());
    }

    /// 会话模式：不落盘，列表来自进程内存。
    #[test]
    fn session_mode_is_not_persisted() {
        let tmp = data_dir();
        clear(tmp.path(), false);
        add(tmp.path(), false, 10, "session-only-entry");
        assert!(!path(tmp.path()).exists());
        let entries = list(tmp.path(), false, 10);
        assert_eq!(entries[0].text, "session-only-entry");
        clear(tmp.path(), false);
        assert!(list(tmp.path(), false, 10).is_empty());
    }

    /// 旧文件含重复/空条目时按归一清理。
    #[test]
    fn normalize_cleans_legacy_duplicates() {
        let tmp = data_dir();
        let raw = r#"{"schemaVersion":1,"entries":[{"text":"x","at":"t1"},{"text":"x","at":"t2"},{"text":"","at":"t3"},{"text":"y","at":"t4"}]}"#;
        std::fs::write(path(tmp.path()), raw).expect("写种子文件失败");
        let entries = list(tmp.path(), true, 10);
        assert_eq!(
            entries.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(),
            vec!["x", "y"]
        );
        assert_eq!(entries[0].at, "t1");
    }
}
