//! 阅读时长统计：窗口聚焦且处于阅读态时累计秒数。
//!
//! 存储（`data/reading-stats.json`，独立 schema）：
//! ```json
//! { "schemaVersion": 1, "day": "2026-10-04", "todaySeconds": 0, "totalSeconds": 0 }
//! ```
//! 跨天自动重置 `todaySeconds`（以调用方传入的“今天”为准，便于测试注入）。
//! 计数上限：单次上报钳制 3600 秒（一小时），防异常值写坏统计。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage::json_io;

/// 统计文件 schema 版本（独立于设置 schema）。
const STATS_SCHEMA_VERSION: u32 = 1;
/// 单次上报秒数上限（防异常值）。
const MAX_ADD_SECONDS: u64 = 3600;
/// 文件名。
const STATS_FILE_NAME: &str = "reading-stats.json";
/// 当前天数上限（约 136 年，防溢出）。
const MAX_DAY_SECONDS: u64 = 4_294_967_296;

/// 阅读时长快照（IPC 返回体；camelCase）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingStats {
    /// 统计日期（`YYYY-MM-DD`，本地日界）
    pub day: String,
    /// 当日累计秒数
    pub today_seconds: u64,
    /// 历史累计秒数
    pub total_seconds: u64,
}

impl ReadingStats {
    fn fresh(day: &str) -> Self {
        Self {
            day: day.to_string(),
            today_seconds: 0,
            total_seconds: 0,
        }
    }
}

/// 统计文件磁盘结构（含 schemaVersion；与快照分离以避免版本字段泄漏到 IPC）。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatsFile {
    /// 文件格式版本
    schema_version: u32,
    /// 统计日期
    day: String,
    /// 当日累计秒数
    today_seconds: u64,
    /// 历史累计秒数
    total_seconds: u64,
}

/// 统计文件路径（`data/reading-stats.json`）。
pub fn stats_path(data_dir: &Path) -> PathBuf {
    data_dir.join(STATS_FILE_NAME)
}

/// 读取统计（缺失/损坏回退当日 0；跨天重置当日值，不落盘——由上报时写回）。
///
/// 参数：`data_dir` 数据目录；`today` 当前日期（`YYYY-MM-DD`，便于测试注入）。
pub fn load(data_dir: &Path, today: &str) -> ReadingStats {
    let Ok(Some(file)) = json_io::read_json_opt::<StatsFile>(&stats_path(data_dir)) else {
        return ReadingStats::fresh(today);
    };
    if file.schema_version != STATS_SCHEMA_VERSION {
        return ReadingStats::fresh(today);
    }
    let mut stats = ReadingStats {
        day: file.day,
        today_seconds: file.today_seconds.min(MAX_DAY_SECONDS),
        total_seconds: file.total_seconds,
    };
    if stats.day != today {
        stats.day = today.to_string();
        stats.today_seconds = 0;
    }
    stats
}

/// 写入统计（原子写）。
fn store(data_dir: &Path, stats: &ReadingStats) -> std::io::Result<()> {
    let file = StatsFile {
        schema_version: STATS_SCHEMA_VERSION,
        day: stats.day.clone(),
        today_seconds: stats.today_seconds,
        total_seconds: stats.total_seconds,
    };
    json_io::write_json_atomic(&stats_path(data_dir), &file)
}

/// 累计秒数并落盘，返回最新快照。
///
/// 参数：`data_dir`；`seconds` 新增秒数（0 → 仅返回当前值；超 3600 钳制）；`today` 当前日期。
/// 边界：跨天时先重置当日值再累计；`total_seconds` 恒增长（饱和加法）。
pub fn add_seconds(data_dir: &Path, seconds: u64, today: &str) -> std::io::Result<ReadingStats> {
    let mut stats = load(data_dir, today);
    let added = seconds.min(MAX_ADD_SECONDS);
    if added > 0 {
        stats.today_seconds = stats
            .today_seconds
            .saturating_add(added)
            .min(MAX_DAY_SECONDS);
        stats.total_seconds = stats.total_seconds.saturating_add(added);
        store(data_dir, &stats)?;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 首次上报：文件生成、当日与总量一致。
    #[test]
    fn first_add_creates_file() {
        let dir = tempfile::tempdir().expect("临时目录");
        let stats = add_seconds(dir.path(), 30, "2026-10-04").expect("上报");
        assert_eq!(stats.today_seconds, 30);
        assert_eq!(stats.total_seconds, 30);
        assert!(stats_path(dir.path()).is_file());
        assert_eq!(load(dir.path(), "2026-10-04"), stats);
    }

    /// 跨天：当日清零、总量保留、日期推进。
    #[test]
    fn day_rollover_resets_today_keeps_total() {
        let dir = tempfile::tempdir().expect("临时目录");
        add_seconds(dir.path(), 120, "2026-10-04").expect("第一天");
        let stats = add_seconds(dir.path(), 60, "2026-10-05").expect("第二天");
        assert_eq!(stats.day, "2026-10-05");
        assert_eq!(stats.today_seconds, 60);
        assert_eq!(stats.total_seconds, 180);
    }

    /// 单次上限：超 3600 钳制；0 秒不落盘仅返回。
    #[test]
    fn clamps_single_add_and_ignores_zero() {
        let dir = tempfile::tempdir().expect("临时目录");
        let stats = add_seconds(dir.path(), 99_999, "2026-10-04").expect("上报");
        assert_eq!(stats.today_seconds, 3600);
        let again = add_seconds(dir.path(), 0, "2026-10-04").expect("零上报");
        assert_eq!(again, stats);
    }

    /// 损坏文件回退当日 0（不崩溃）。
    #[test]
    fn corrupt_file_falls_back_to_zero() {
        let dir = tempfile::tempdir().expect("临时目录");
        std::fs::write(stats_path(dir.path()), b"{ broken").expect("写损坏文件");
        let stats = load(dir.path(), "2026-10-04");
        assert_eq!(stats.today_seconds, 0);
        assert_eq!(stats.total_seconds, 0);
    }
}
