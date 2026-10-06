//! 隐私清除：按范围聚合清理本地数据（查看历史 / 会话 / 快照 / 批注 / 剪贴板 / 日志）。
//!
//! 设计：
//! - `usage_counts` 提供各类数据的条数/文件数，供设置界面回显；
//! - `clear` 按勾选范围执行删除，返回逐类数量与跳过数（被占用文件计入跳过，不阻断）；
//! - 全部操作限定在数据目录内；不涉及任何网络。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::clipboard_history;
use crate::history::store as history_store;
use crate::resources::{self, ClearScope};

/// 清理范围（勾选项）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyScope {
    /// 查看历史（history.jsonl）
    pub history: bool,
    /// 会话布局（session.json）
    pub session: bool,
    /// 版本快照（snapshots/）
    pub snapshots: bool,
    /// 书签/高亮/注释（annotations/）
    pub annotations: bool,
    /// 剪贴板历史（clipboard-history.json）
    pub clipboard: bool,
    /// 日志（logs/ 除占用中的当前文件）
    pub logs: bool,
}

/// 各类数据当前数量（供设置界面展示）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageCounts {
    /// 历史条目数
    pub history: u64,
    /// 会话中的窗口数
    pub session: u64,
    /// 快照文件数
    pub snapshots: u64,
    /// 批注文件数
    pub annotations: u64,
    /// 剪贴板条目数
    pub clipboard: u64,
    /// 日志文件数
    pub logs: u64,
}

/// 清理结果（逐类删除数量 + 跳过数）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyReport {
    /// 删除的历史条目数
    pub history: u64,
    /// 删除的会话文件数（0/1）
    pub session: u64,
    /// 删除的快照文件数
    pub snapshots: u64,
    /// 删除的批注文件数
    pub annotations: u64,
    /// 删除的剪贴板条目数
    pub clipboard: u64,
    /// 删除的日志文件数
    pub logs: u64,
    /// 被占用/失败跳过的文件数
    pub skipped: u64,
}

/// 统计目录内文件数（仅一层递归；目录不存在返回 0）。
fn count_files(dir: &Path) -> u64 {
    let Ok(read) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0_u64;
    for entry in read.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_file() {
            count += 1;
        } else if meta.is_dir() {
            count += count_files(&entry.path());
        }
    }
    count
}

/// 统计并删除目录全部内容（目录保留为空；返回 (删除文件数, 跳过数)）。
fn purge_dir(dir: &Path) -> (u64, u64) {
    let before = count_files(dir);
    let result = resources::clear_directory(dir);
    let skipped = result.skipped;
    (before.saturating_sub(skipped), skipped)
}

fn history_path(dir: &Path) -> PathBuf {
    history_store::history_path(dir)
}

fn session_path(dir: &Path) -> PathBuf {
    crate::session::store::session_path(dir)
}

fn clipboard_entries(dir: &Path, persist: bool) -> u64 {
    clipboard_history::list(dir, persist, u32::MAX).len() as u64
}

/// 统计各类数据数量。
pub fn usage_counts(dir: &Path, clipboard_persist: bool) -> UsageCounts {
    let history = std::fs::read_to_string(history_path(dir))
        .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count() as u64)
        .unwrap_or(0);
    let session = if session_path(dir).is_file() {
        crate::session::store::load(dir).windows.len() as u64
    } else {
        0
    };
    UsageCounts {
        history,
        session,
        snapshots: count_files(&dir.join("snapshots")),
        annotations: count_files(&dir.join("annotations")),
        clipboard: clipboard_entries(dir, clipboard_persist),
        logs: count_files(&dir.join("logs")),
    }
}

/// 按范围清理；返回逐类数量。任何一类失败不阻断其余范围。
pub fn clear(dir: &Path, scope: PrivacyScope, clipboard_persist: bool) -> PrivacyReport {
    let mut report = PrivacyReport::default();

    if scope.history {
        report.history = std::fs::read_to_string(history_path(dir))
            .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count() as u64)
            .unwrap_or(0);
        if history_store::write_all(dir, &[]).is_err() {
            report.skipped += 1;
            report.history = 0;
        }
    }

    if scope.session {
        let path = session_path(dir);
        if path.is_file() {
            match std::fs::remove_file(&path) {
                Ok(()) => report.session = 1,
                Err(_) => report.skipped += 1,
            }
        }
    }

    if scope.snapshots {
        let (removed, skipped) = purge_dir(&dir.join("snapshots"));
        report.snapshots = removed;
        report.skipped += skipped;
    }

    if scope.annotations {
        let (removed, skipped) = purge_dir(&dir.join("annotations"));
        report.annotations = removed;
        report.skipped += skipped;
    }

    if scope.clipboard {
        report.clipboard = clipboard_entries(dir, clipboard_persist);
        clipboard_history::clear(dir, clipboard_persist);
    }

    if scope.logs {
        report.logs = count_files(&dir.join("logs"));
        let result = resources::clear_scope(dir, ClearScope::Logs);
        report.skipped += result.skipped as u64;
        report.logs = report.logs.saturating_sub(result.skipped as u64);
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(dir: &Path) {
        std::fs::create_dir_all(dir.join("logs")).expect("日志目录");
        std::fs::create_dir_all(dir.join("snapshots").join("ab")).expect("快照目录");
        std::fs::create_dir_all(dir.join("annotations")).expect("批注目录");
        std::fs::write(dir.join("history.jsonl"), "{\"a\":1}\n{\"b\":2}\n").expect("历史");
        std::fs::write(
            dir.join("session.json"),
            "{\"schemaVersion\":4,\"windows\":[{\"label\":\"main\"}]}",
        )
        .expect("会话");
        std::fs::write(dir.join("snapshots").join("ab").join("1.snap"), "x").expect("快照文件");
        std::fs::write(dir.join("annotations").join("h.json"), "{}").expect("批注文件");
        std::fs::write(
            dir.join("clipboard-history.json"),
            "{\"schemaVersion\":1,\"entries\":[{\"text\":\"a\",\"at\":\"t\"}]}",
        )
        .expect("剪贴板");
        std::fs::write(dir.join("logs").join("app.log.1"), "old").expect("日志");
        std::fs::write(dir.join("logs").join("app.log"), "new").expect("当前日志");
    }

    #[test]
    fn counts_and_clear_cover_all_scopes() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let dir = tmp.path();
        seed(dir);
        let counts = usage_counts(dir, true);
        assert_eq!(counts.history, 2);
        assert_eq!(counts.session, 1);
        assert_eq!(counts.snapshots, 1);
        assert_eq!(counts.annotations, 1);
        assert_eq!(counts.clipboard, 1);
        assert_eq!(counts.logs, 2);

        let report = clear(
            dir,
            PrivacyScope {
                history: true,
                session: true,
                snapshots: true,
                annotations: true,
                clipboard: true,
                logs: true,
            },
            true,
        );
        assert_eq!(report.history, 2);
        assert_eq!(report.session, 1);
        assert_eq!(report.snapshots, 1);
        assert_eq!(report.annotations, 1);
        assert_eq!(report.clipboard, 1);
        assert_eq!(report.logs, 2);

        assert!(!session_path(dir).exists());
        assert_eq!(count_files(&dir.join("snapshots")), 0);
        assert_eq!(count_files(&dir.join("annotations")), 0);
        assert!(std::fs::read_to_string(dir.join("history.jsonl"))
            .expect("历史文件")
            .trim()
            .is_empty());
        let after = usage_counts(dir, true);
        assert_eq!(after.clipboard, 0);
        assert_eq!(after.logs, 0);
    }

    #[test]
    fn partial_scope_only_touches_selected() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let dir = tmp.path();
        seed(dir);
        let report = clear(
            dir,
            PrivacyScope {
                history: true,
                session: false,
                snapshots: false,
                annotations: false,
                clipboard: false,
                logs: false,
            },
            true,
        );
        assert_eq!(report.history, 2);
        assert!(session_path(dir).exists());
        assert_eq!(count_files(&dir.join("snapshots")), 1);
    }
}
