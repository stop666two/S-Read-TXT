//! 历史文件读写（`history.jsonl`，每行一条 JSON）。
//!
//! 策略（`docs/configuration.md` §2.5）：
//! - 追加：[`append`] 直接落一行（崩溃最多损坏最后一行，载入时跳过并记日志）；
//! - 载入：[`load`] 解析 → 丢弃损坏行 → 按路径去重（保留最新）→ 修剪（条数 + 天数）
//!   → 时间倒序；若发生压缩（数量变化）则原子重写文件（自愈）；
//! - 重写：[`write_all`] 原子落盘（清空 = 写入空列表）。

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::history::entry::HistoryEntry;
use crate::settings::model::HistorySettings;
use crate::storage::{atomic, data_dir};
use crate::time_util;

/// 历史文件名
pub const FILE_HISTORY: &str = "history.jsonl";

/// 历史文件路径（数据目录内）
pub fn history_path(dir: &Path) -> PathBuf {
    dir.join(FILE_HISTORY)
}

/// 追加一条历史（单行 JSON；失败返回 IO 错误）。
pub fn append(dir: &Path, entry: &HistoryEntry) -> io::Result<()> {
    let path = history_path(dir);
    if let Some(parent) = path.parent() {
        data_dir::ensure_dir(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    let line = serde_json::to_string(entry)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()
}

/// 原子重写整个历史文件（清空 = 传空切片）。
pub fn write_all(dir: &Path, entries: &[HistoryEntry]) -> io::Result<()> {
    let mut buffer = String::new();
    for entry in entries {
        let line = serde_json::to_string(entry)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        buffer.push_str(&line);
        buffer.push('\n');
    }
    atomic::write_atomic_str(&history_path(dir), &buffer)
}

/// 载入历史（含自愈）：去重 + 修剪 + 时间倒序；必要时压缩重写文件。
pub fn load(dir: &Path, settings: &HistorySettings) -> Vec<HistoryEntry> {
    let raw = read_all(dir);
    let raw_count = raw.len();
    let mut entries = dedupe_latest(raw);
    entries.sort_by(|a, b| sort_key(b).cmp(&sort_key(a)));
    prune_by_days(&mut entries, settings.retention_days);
    prune_by_count(&mut entries, settings.max_entries);
    if entries.len() != raw_count {
        log::info!(
            target: "sread::history",
            "历史压缩：{raw_count} → {} 条",
            entries.len()
        );
        if let Err(err) = write_all(dir, &entries) {
            log::warn!(target: "sread::history", "历史压缩写盘失败：{err}");
        }
    }
    entries
}

/// 读取并解析全部行（损坏行/缺路径行跳过并记日志；文件缺失视为空历史）。
fn read_all(dir: &Path) -> Vec<HistoryEntry> {
    let text = match std::fs::read_to_string(history_path(dir)) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            log::warn!(target: "sread::history", "历史文件读取失败：{err}");
            return Vec::new();
        }
    };
    let mut entries = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<HistoryEntry>(line) {
            Ok(entry) if !entry.path.is_empty() => entries.push(entry),
            Ok(_) => log::warn!(
                target: "sread::history",
                "历史第 {} 行缺少路径，已跳过",
                index + 1
            ),
            Err(err) => log::warn!(
                target: "sread::history",
                "历史第 {} 行损坏，已跳过：{err}",
                index + 1
            ),
        }
    }
    entries
}

/// 按路径去重，保留打开时间最新的一条（时间不可解析视为最旧）。
fn dedupe_latest(entries: Vec<HistoryEntry>) -> Vec<HistoryEntry> {
    let mut latest: BTreeMap<String, HistoryEntry> = BTreeMap::new();
    for entry in entries {
        match latest.get(&entry.path) {
            Some(existing) if sort_key(existing) >= sort_key(&entry) => {}
            _ => {
                latest.insert(entry.path.clone(), entry);
            }
        }
    }
    latest.into_values().collect()
}

/// 排序键：UNIX 秒；不可解析视为最小值（便于统一丢弃）。
fn sort_key(entry: &HistoryEntry) -> i64 {
    time_util::unix_seconds(&entry.opened_at).unwrap_or(i64::MIN)
}

/// 按保留天数修剪（过期或时间不可解析的条目被移除）。
fn prune_by_days(entries: &mut Vec<HistoryEntry>, retention_days: u32) {
    let cutoff = time_util::now_unix_seconds() - i64::from(retention_days) * 86_400;
    entries.retain(|entry| sort_key(entry) >= cutoff);
}

/// 按条数修剪（保留最新的前 N 条；调用前应已按时间倒序）。
fn prune_by_count(entries: &mut Vec<HistoryEntry>, max_entries: u32) {
    entries.truncate(max_entries as usize);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("创建临时目录失败")
    }

    fn entry(path: &str, opened_at: &str) -> HistoryEntry {
        HistoryEntry {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap_or(path).to_string(),
            size: 42,
            encoding: "UTF-8".to_string(),
            opened_at: opened_at.to_string(),
            last_row: 7,
            last_percent: 12.5,
        }
    }

    fn settings(max_entries: u32, retention_days: u32) -> HistorySettings {
        HistorySettings {
            max_entries,
            retention_days,
        }
    }

    /// 追加两条同路径 → 载入仅一条且保留最新。
    #[test]
    fn append_then_load_dedupes_latest() {
        let dir = data_dir();
        append(dir.path(), &entry("D:/a.txt", "2026-01-01T00:00:00Z")).expect("追加失败");
        append(dir.path(), &entry("D:/a.txt", "2026-02-01T00:00:00Z")).expect("追加失败");
        let loaded = load(dir.path(), &settings(10, 3650));
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].opened_at, "2026-02-01T00:00:00Z");
    }

    /// 条数修剪：只保留最新 N 条，且按时间倒序。
    #[test]
    fn prune_by_count_keeps_newest_in_desc_order() {
        let dir = data_dir();
        append(dir.path(), &entry("D:/1.txt", "2026-01-01T00:00:00Z")).expect("追加失败");
        append(dir.path(), &entry("D:/2.txt", "2026-03-01T00:00:00Z")).expect("追加失败");
        append(dir.path(), &entry("D:/3.txt", "2026-02-01T00:00:00Z")).expect("追加失败");
        let loaded = load(dir.path(), &settings(2, 3650));
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].path, "D:/2.txt");
        assert_eq!(loaded[1].path, "D:/3.txt");
    }

    /// 天数修剪：过期条目被移除，新条目保留。
    #[test]
    fn prune_by_days_drops_expired() {
        let dir = data_dir();
        append(dir.path(), &entry("D:/old.txt", "2020-01-01T00:00:00Z")).expect("追加失败");
        append(
            dir.path(),
            &entry("D:/fresh.txt", &time_util::now_rfc3339()),
        )
        .expect("追加失败");
        let loaded = load(dir.path(), &settings(10, 365));
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].path, "D:/fresh.txt");
    }

    /// 损坏行与缺路径行跳过，合法行保留。
    #[test]
    fn corrupt_lines_are_skipped() {
        let dir = data_dir();
        let valid_a = serde_json::to_string(&entry("D:/a.txt", "2026-01-01T00:00:00Z")).unwrap();
        let valid_b = serde_json::to_string(&entry("D:/b.txt", "2026-01-02T00:00:00Z")).unwrap();
        std::fs::write(
            history_path(dir.path()),
            format!("{valid_a}\n{{ not json\n{{\"name\":\"no-path\"}}\n{valid_b}\n"),
        )
        .expect("写历史失败");
        let loaded = load(dir.path(), &settings(10, 3650));
        assert_eq!(loaded.len(), 2);
    }

    /// 时间不可解析的条目在载入时被移除。
    #[test]
    fn unparsable_time_is_dropped() {
        let dir = data_dir();
        append(dir.path(), &entry("D:/bad.txt", "not-a-time")).expect("追加失败");
        let loaded = load(dir.path(), &settings(10, 3650));
        assert!(loaded.is_empty());
    }

    /// 原子重写与清空。
    #[test]
    fn write_all_and_clear() {
        let dir = data_dir();
        write_all(dir.path(), &[entry("D:/x.txt", "2026-01-01T00:00:00Z")]).expect("重写失败");
        assert_eq!(load(dir.path(), &settings(10, 3650)).len(), 1);
        write_all(dir.path(), &[]).expect("清空失败");
        assert!(load(dir.path(), &settings(10, 3650)).is_empty());
        assert!(
            history_path(dir.path()).exists(),
            "清空后文件应存在（空文件）"
        );
    }

    /// 文件缺失视为空历史。
    #[test]
    fn missing_file_returns_empty() {
        let dir = data_dir();
        assert!(load(dir.path(), &settings(10, 365)).is_empty());
    }
}
