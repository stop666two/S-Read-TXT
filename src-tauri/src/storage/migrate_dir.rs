//! 数据目录迁移：复制 → 校验 → 写指针 → 清理原目录；失败回退。
//!
//! 设计（p0-plan P0-10）：
//! - 指针文件：程序目录 `config.json`（见 `paths::DataDirPointer`），启动解析时
//!   优先于便携目录（`运行时覆盖 → SRT_DATA_DIR → 指针 → 程序目录/data`）；
//! - 前提：目标必须为空目录（防误覆盖）；目标不能等于/包含/被包含于当前目录；
//! - 复制：对被占用文件宽容跳过（`skipped` 计数，典型场景是 WebView2 缓存），
//!   其余任何失败 → 清理目标、保留原目录、返回错误；
//! - 校验：目标统计 == **复制记录**（复制时逐文件累计）；不使用实时源统计——
//!   源目录在迁移期间可能被 WebView2 等继续写入，实时比对存在竞态；
//! - 原目录清理：尽力删除；被占用时写入目标目录 `.cleanup.json` 标记，
//!   下次启动由 [`cleanup_pending`] 重试（此时 WebView2 尚未启动、文件锁已释放）。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage::{json_io, paths};

/// 清理标记文件名（位于新数据目录；启动时消费）
pub const CLEANUP_MARKER_FILE: &str = ".cleanup.json";

/// 迁移错误（`Display` 提供中文原因，供 IPC 消息与日志）。
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    /// 目标目录与当前数据目录相同
    #[error("目标目录与当前数据目录相同")]
    SameDir,
    /// 目标目录与当前数据目录互相包含
    #[error("目标目录不能位于当前数据目录内，反之亦然")]
    NestedTarget,
    /// 目标目录非空（防误覆盖）
    #[error("目标目录非空（请选择空目录或新建目录）：{0}")]
    TargetNotEmpty(String),
    /// 复制失败
    #[error("复制数据失败：{0}")]
    Copy(#[source] io::Error),
    /// 校验失败（复制不完整）
    #[error(
        "迁移校验失败（目标文件 {dst_files} ≠ 复制记录 {src_files}，目标字节 {dst_bytes} ≠ 复制记录 {src_bytes}）"
    )]
    Verify {
        /// 复制记录文件数（含跳过）
        src_files: u64,
        /// 目标文件数
        dst_files: u64,
        /// 复制记录字节数
        src_bytes: u64,
        /// 目标字节数
        dst_bytes: u64,
    },
    /// 写指针文件失败
    #[error("写入迁移指针失败：{0}")]
    Pointer(#[source] io::Error),
}

/// 迁移结果（IPC 返回体；camelCase）
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReport {
    /// 成功复制的文件数
    pub copied_files: u64,
    /// 成功复制的字节数
    pub copied_bytes: u64,
    /// 因被占用而跳过的文件数（WebView2 缓存等；不影响迁移生效）
    pub skipped: u64,
    /// 原目录是否已同步清理（`false` = 延迟到下次启动）
    pub old_removed: bool,
}

/// 清理标记（`.cleanup.json`；字段与格式版本独立于设置 schema）
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CleanupMarker {
    schema_version: u32,
    /// 待清理的旧数据目录绝对路径
    old_dir: String,
}

/// 复制统计（内部）
#[derive(Debug, Default, Clone, Copy)]
struct CopyStats {
    files: u64,
    bytes: u64,
    skipped: u64,
}

/// 递归复制目录树；被占用/复制失败的文件计入 `skipped`（不中断整体迁移）。
/// 字节数取 `fs::copy` 的返回值（实际写入量），确保校验与目标完全一致。
fn copy_tree(src: &Path, dst: &Path, stats: &mut CopyStats) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to, stats)?;
        } else {
            match fs::copy(&from, &to) {
                Ok(copied) => {
                    stats.files += 1;
                    stats.bytes += copied;
                }
                Err(_) => {
                    stats.skipped += 1;
                }
            }
        }
    }
    Ok(())
}

/// 递归统计目录树（文件数、字节数）。
fn tree_stats(path: &Path) -> io::Result<(u64, u64)> {
    let mut files = 0_u64;
    let mut bytes = 0_u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let (sub_files, sub_bytes) = tree_stats(&entry.path())?;
            files += sub_files;
            bytes += sub_bytes;
        } else {
            files += 1;
            bytes += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
        }
    }
    Ok((files, bytes))
}

/// 失败回滚：清除目标目录内容（若目标原本不存在则整个移除；原本存在的空目录恢复为空）。
fn rollback_target(target: &Path, preexisted: bool) {
    let _ = fs::remove_dir_all(target);
    if preexisted {
        let _ = fs::create_dir_all(target);
    }
}

/// 执行迁移：复制校验后写指针并清理原目录；任一步失败 → 回滚目标、保留原目录。
///
/// 参数：
/// - `current`：当前数据目录（源）
/// - `target`：目标目录（必须为空或不存在）
/// - `pointer_file`：迁移指针文件路径（程序目录 `config.json`）
pub fn migrate_data_dir(
    current: &Path,
    target: &Path,
    pointer_file: &Path,
) -> Result<MigrationReport, MigrationError> {
    if target == current {
        return Err(MigrationError::SameDir);
    }
    if target.starts_with(current) || current.starts_with(target) {
        return Err(MigrationError::NestedTarget);
    }
    let preexisted = target.exists();
    if preexisted {
        let mut entries = fs::read_dir(target).map_err(MigrationError::Copy)?;
        if entries.next().is_some() {
            return Err(MigrationError::TargetNotEmpty(target.display().to_string()));
        }
    }
    fs::create_dir_all(target).map_err(MigrationError::Copy)?;

    let mut stats = CopyStats::default();
    if let Err(err) = copy_tree(current, target, &mut stats) {
        rollback_target(target, preexisted);
        return Err(MigrationError::Copy(err));
    }

    // 校验：目标统计 == 复制记录（源在迁移期间可能被 WebView2 等继续写入，
    // 因此不使用实时源统计；目标从空目录开始且期间无其他写入者，统计应恰好相等）。
    let (dst_files, dst_bytes) = tree_stats(target).map_err(MigrationError::Copy)?;
    if dst_files != stats.files || dst_bytes != stats.bytes {
        rollback_target(target, preexisted);
        return Err(MigrationError::Verify {
            src_files: stats.files,
            dst_files,
            src_bytes: stats.bytes,
            dst_bytes,
        });
    }

    if let Err(err) = paths::write_pointer(pointer_file, target) {
        rollback_target(target, preexisted);
        return Err(MigrationError::Pointer(err));
    }

    // 原目录清理：尽力同步；失败写延迟清理标记（下次启动重试）
    let old_removed = match fs::remove_dir_all(current) {
        Ok(()) => true,
        Err(_) => {
            let marker = target.join(CLEANUP_MARKER_FILE);
            let cleanup = CleanupMarker {
                schema_version: 1,
                old_dir: current.display().to_string(),
            };
            let _ = json_io::write_json_atomic(&marker, &cleanup);
            false
        }
    };

    Ok(MigrationReport {
        copied_files: stats.files,
        copied_bytes: stats.bytes,
        skipped: stats.skipped,
        old_removed,
    })
}

/// 启动时消费延迟清理标记：重试删除旧数据目录。
///
/// 返回：成功清理的旧目录路径（用于日志）；无标记/未成功时返回 `None`。
/// 说明：删除失败会保留标记（下次启动再试）；标记本身损坏时直接移除标记。
pub fn cleanup_pending(data_dir: &Path) -> Option<PathBuf> {
    let marker = data_dir.join(CLEANUP_MARKER_FILE);
    let content = fs::read_to_string(&marker).ok()?;
    let parsed: CleanupMarker = match serde_json::from_str(&content) {
        Ok(parsed) => parsed,
        Err(_) => {
            let _ = fs::remove_file(&marker);
            return None;
        }
    };
    if parsed.schema_version != 1 || parsed.old_dir.trim().is_empty() {
        let _ = fs::remove_file(&marker);
        return None;
    }
    let old_dir = PathBuf::from(parsed.old_dir.trim());
    match fs::remove_dir_all(&old_dir) {
        Ok(()) => {
            let _ = fs::remove_file(&marker);
            Some(old_dir)
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            let _ = fs::remove_file(&marker);
            Some(old_dir)
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造带内容的源目录（含子目录与空子目录）。
    fn seed_source(dir: &Path) {
        fs::create_dir_all(dir.join("sub")).expect("建子目录失败");
        fs::create_dir_all(dir.join("empty")).expect("建空目录失败");
        fs::write(dir.join("a.txt"), b"alpha").expect("写文件失败");
        fs::write(dir.join("sub").join("b.txt"), b"beta-beta").expect("写文件失败");
    }

    /// 正常迁移：文件齐全、指针写入、原目录删除。
    #[test]
    fn migrate_moves_files_writes_pointer_and_removes_old() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let current = tmp.path().join("data");
        let target = tmp.path().join("moved");
        let pointer = tmp.path().join("config.json");
        seed_source(&current);

        let report = migrate_data_dir(&current, &target, &pointer).expect("迁移失败");
        assert_eq!(report.copied_files, 2);
        assert_eq!(report.copied_bytes, 5 + 9);
        assert_eq!(report.skipped, 0);
        assert!(report.old_removed, "原目录应被同步清理");
        assert!(!current.exists(), "原目录应删除");
        assert_eq!(
            fs::read_to_string(target.join("sub").join("b.txt")).expect("读取失败"),
            "beta-beta"
        );
        assert!(target.join("empty").is_dir(), "空目录应保留");
        let pointer_dir = paths::read_pointer(&tmp.path().join("app.exe")).expect("指针应可读");
        assert_eq!(pointer_dir, target);
    }

    /// 目标非空 → 拒绝且原目录不受影响。
    #[test]
    fn non_empty_target_is_rejected() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let current = tmp.path().join("data");
        let target = tmp.path().join("occupied");
        seed_source(&current);
        fs::create_dir_all(&target).expect("建目标失败");
        fs::write(target.join("stray.txt"), b"keep").expect("写障碍失败");

        let err = migrate_data_dir(&current, &target, &tmp.path().join("config.json"))
            .expect_err("应拒绝");
        assert!(matches!(err, MigrationError::TargetNotEmpty(_)));
        assert!(current.join("a.txt").is_file(), "原目录应保留");
        assert_eq!(
            fs::read_to_string(target.join("stray.txt")).expect("读取失败"),
            "keep"
        );
    }

    /// 相同目录 / 互相包含 → 拒绝。
    #[test]
    fn same_or_nested_dirs_are_rejected() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let current = tmp.path().join("data");
        seed_source(&current);
        let pointer = tmp.path().join("config.json");
        assert!(matches!(
            migrate_data_dir(&current, &current, &pointer),
            Err(MigrationError::SameDir)
        ));
        assert!(matches!(
            migrate_data_dir(&current, &current.join("inner"), &pointer),
            Err(MigrationError::NestedTarget)
        ));
        assert!(matches!(
            migrate_data_dir(&current, &tmp.path(), &pointer),
            Err(MigrationError::NestedTarget)
        ));
    }

    /// 指针写入失败 → 回滚目标、原目录不受影响。
    #[test]
    fn pointer_failure_rolls_back_target() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let current = tmp.path().join("data");
        let target = tmp.path().join("moved");
        seed_source(&current);
        // 指针路径的父级是文件 → 写指针必然失败
        let blocker = tmp.path().join("blocker-file");
        fs::write(&blocker, b"x").expect("写障碍失败");
        let pointer = blocker.join("config.json");

        let err = migrate_data_dir(&current, &target, &pointer).expect_err("应失败");
        assert!(matches!(err, MigrationError::Pointer(_)));
        assert!(current.join("a.txt").is_file(), "原目录应保留");
        assert!(!target.join("a.txt").exists(), "目标应回滚");
    }

    /// 延迟清理：标记存在时删除旧目录并移除标记；标记损坏则仅移除标记。
    #[test]
    fn cleanup_pending_removes_marker_and_old_dir() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let data = tmp.path().join("data");
        let old = tmp.path().join("old-dir");
        fs::create_dir_all(&old).expect("建旧目录失败");
        fs::write(old.join("f.txt"), b"x").expect("写文件失败");
        let marker = data.join(CLEANUP_MARKER_FILE);
        fs::create_dir_all(&data).expect("建数据目录失败");
        let payload = CleanupMarker {
            schema_version: 1,
            old_dir: old.display().to_string(),
        };
        fs::write(
            &marker,
            serde_json::to_string_pretty(&payload).expect("序列化失败"),
        )
        .expect("写标记失败");

        let removed = cleanup_pending(&data).expect("应清理");
        assert_eq!(removed, old);
        assert!(!old.exists());
        assert!(!marker.exists(), "标记应移除");

        // 损坏标记：直接移除标记、不报错
        fs::write(&marker, b"{ broken").expect("写损坏标记失败");
        assert!(cleanup_pending(&data).is_none());
        assert!(!marker.exists(), "损坏标记应被移除");
    }
}
