//! 自动保存快照与版本历史（P3-1，D 组 D-05…D-13）。
//!
//! 设计（与 `docs/configuration.md` §2.11 同步）：
//! - 快照以「保存时文档编码」的完整字节写入 `data/snapshots/<文件键>/`；
//! - 文件键 = 源文件路径小写后的 FNV-1a 64 位十六进制（与标注目录相互独立）；
//! - 文件名 = `{毫秒时间戳(13)}-{序号}.snap`（字典序 = 时间序，便于裁剪）；
//! - 去重：与最新快照内容逐字节相同则跳过（定时器高频调用无副作用）；
//! - 裁剪：`keep` 份数上限 + `max_mb` 容量上限（超限从最旧删起）；
//! - 崩溃恢复：优雅退出时写 `data/.clean-exit`；启动时 `take_clean_exit`
//!   返回「上次是否干净退出」并清除标记，由上层决定是否提示恢复。

use std::path::{Path, PathBuf};

use serde::Serialize;
use thiserror::Error;

use crate::storage::atomic::write_atomic;
use crate::textfile::editing::edit_doc::EditDoc;
use crate::textfile::editing::save::{document_bytes, SaveError};
use crate::time_util::now_unix_millis;

/// 快照目录名（数据目录下）。
pub const SNAPSHOTS_DIR: &str = "snapshots";
/// 干净退出标记文件名（数据目录下）。
pub const CLEAN_EXIT_FLAG: &str = ".clean-exit";
/// 单次列举上限（防御异常目录）。
const LIST_LIMIT: usize = 10_000;

/// 快照条目（IPC 返回体；`name` 为稳定文件名，恢复/删除以此定位）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    /// 文件名（`{毫秒}-{序号}.snap`）
    pub name: String,
    /// 创建时间（Unix 毫秒；取自文件名，避免依赖文件系统时间）
    pub created_millis: i64,
    /// 字节数
    pub bytes: u64,
}

/// 快照操作错误。
#[derive(Debug, Error)]
pub enum SnapshotError {
    /// 快照不存在
    #[error("快照不存在：{0}")]
    NotFound(String),
    /// 快照名非法（含路径分隔符或非预期字符）
    #[error("快照名非法")]
    InvalidName,
    /// IO 错误
    #[error("快照 IO 失败：{0}")]
    Io(#[from] std::io::Error),
    /// 文档编码失败（保存路径复用）
    #[error(transparent)]
    Save(#[from] SaveError),
}

/// 源文件路径 → 稳定目录键（FNV-1a 64；大小写不敏感，保持跨重启稳定）。
pub fn file_key(source_path: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source_path.to_lowercase().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// 快照目录（数据目录 / snapshots / 文件键）。
pub fn snapshots_dir(data_dir: &Path, source_path: &str) -> PathBuf {
    data_dir.join(SNAPSHOTS_DIR).join(file_key(source_path))
}

/// 快照名是否合法（数字、短横线与 `.snap` 后缀；无路径分隔符）。
fn is_valid_name(name: &str) -> bool {
    name.ends_with(".snap")
        && name.len() <= 64
        && name.chars().all(|ch| {
            ch.is_ascii_digit()
                || ch == '-'
                || ch == '.'
                || ch == 's'
                || ch == 'n'
                || ch == 'a'
                || ch == 'p'
        })
        && !name.contains(['/', '\\'])
}

/// 列出快照（新→旧）。
pub fn list(data_dir: &Path, source_path: &str) -> Vec<SnapshotInfo> {
    let dir = snapshots_dir(data_dir, source_path);
    let mut items: Vec<SnapshotInfo> = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return items;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_valid_name(&name) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let millis = name
            .split('-')
            .next()
            .and_then(|part| part.parse::<i64>().ok())
            .unwrap_or(0);
        items.push(SnapshotInfo {
            name,
            created_millis: millis,
            bytes: meta.len(),
        });
    }
    items.sort_by(|a, b| {
        b.created_millis
            .cmp(&a.created_millis)
            .then_with(|| b.name.cmp(&a.name))
    });
    items.truncate(LIST_LIMIT);
    items
}

/// 创建快照；内容与最新快照相同 → `Ok(None)`（去重跳过）。
///
/// 参数：`keep` 份数上限（≥1）、`max_mb` 容量上限（MB）。
pub fn create(
    data_dir: &Path,
    source_path: &str,
    doc: &EditDoc,
    keep: u32,
    max_mb: u32,
) -> Result<Option<SnapshotInfo>, SnapshotError> {
    let bytes = document_bytes(doc, doc.encoding())?;
    let dir = snapshots_dir(data_dir, source_path);
    std::fs::create_dir_all(&dir)?;
    // 去重：最新快照逐字节相同则跳过
    if let Some(latest) = list(data_dir, source_path).first() {
        if let Ok(previous) = std::fs::read(dir.join(&latest.name)) {
            if previous == bytes {
                return Ok(None);
            }
        }
    }
    let millis = now_unix_millis() as i64;
    let mut name = String::new();
    for suffix in 0u32..1000 {
        let candidate = format!("{millis:013}-{suffix:04}.snap");
        if !dir.join(&candidate).exists() {
            name = candidate;
            break;
        }
    }
    if name.is_empty() {
        return Err(SnapshotError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "同一毫秒内快照过多",
        )));
    }
    write_atomic(&dir.join(&name), &bytes)?;
    prune(data_dir, source_path, keep, max_mb)?;
    Ok(Some(SnapshotInfo {
        name,
        created_millis: millis,
        bytes: bytes.len() as u64,
    }))
}

/// 读取快照字节。
pub fn read(data_dir: &Path, source_path: &str, name: &str) -> Result<Vec<u8>, SnapshotError> {
    if !is_valid_name(name) {
        return Err(SnapshotError::InvalidName);
    }
    let path = snapshots_dir(data_dir, source_path).join(name);
    if !path.is_file() {
        return Err(SnapshotError::NotFound(name.to_string()));
    }
    Ok(std::fs::read(path)?)
}

/// 删除快照；不存在时返回 `Ok(false)`（幂等）。
pub fn delete(data_dir: &Path, source_path: &str, name: &str) -> Result<bool, SnapshotError> {
    if !is_valid_name(name) {
        return Err(SnapshotError::InvalidName);
    }
    let path = snapshots_dir(data_dir, source_path).join(name);
    if !path.is_file() {
        return Ok(false);
    }
    std::fs::remove_file(path)?;
    Ok(true)
}

/// 裁剪：保留最新 `keep` 份且累计 ≤ `max_mb`，其余从最旧删除。
pub fn prune(
    data_dir: &Path,
    source_path: &str,
    keep: u32,
    max_mb: u32,
) -> Result<u64, SnapshotError> {
    let items = list(data_dir, source_path);
    let keep = (keep.max(1) as usize).min(items.len());
    let budget = u64::from(max_mb).saturating_mul(1024 * 1024);
    let mut accumulated: u64 = 0;
    let mut removed: u64 = 0;
    for (index, item) in items.iter().enumerate() {
        accumulated = accumulated.saturating_add(item.bytes);
        if index < keep && accumulated <= budget {
            continue;
        }
        let path = snapshots_dir(data_dir, source_path).join(&item.name);
        if std::fs::remove_file(path).is_ok() {
            removed += item.bytes;
        }
    }
    Ok(removed)
}

/// 写入「干净退出」标记（退出路径调用；失败仅记录，不阻塞退出）。
pub fn mark_clean_exit(data_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    write_atomic(&data_dir.join(CLEAN_EXIT_FLAG), b"1")
}

/// 读取并清除「干净退出」标记。
///
/// 返回：`true` = 上次为干净退出（标记存在）；`false` = 首次运行或上次异常退出。
pub fn take_clean_exit(data_dir: &Path) -> bool {
    let flag = data_dir.join(CLEAN_EXIT_FLAG);
    if flag.is_file() {
        let _ = std::fs::remove_file(flag);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{create, delete, list, mark_clean_exit, read, take_clean_exit};
    use crate::textfile::editing::edit_doc::EditDoc;

    fn open_doc(dir: &std::path::Path, contents: &[u8]) -> (std::path::PathBuf, EditDoc) {
        let path = dir.join("snap.txt");
        let mut file = std::fs::File::create(&path).expect("创建文件失败");
        file.write_all(contents).expect("写入失败");
        drop(file);
        let doc = EditDoc::open(&path, None, 100).expect("打开失败");
        (path, doc)
    }

    /// 创建 → 列举 → 读取字节一致。
    #[test]
    fn create_list_read_roundtrip() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let (path, doc) = open_doc(temp.path(), b"hello\n");
        let created = create(temp.path(), &path.to_string_lossy(), &doc, 50, 200)
            .expect("创建失败")
            .expect("应有快照");
        assert!(created.name.ends_with(".snap"));
        let items = list(temp.path(), &path.to_string_lossy());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, created.name);
        let bytes = read(temp.path(), &path.to_string_lossy(), &created.name).expect("读取失败");
        assert_eq!(bytes, b"hello\n");
    }

    /// 内容未变 → 去重跳过。
    #[test]
    fn identical_content_is_skipped() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let (path, doc) = open_doc(temp.path(), b"same\n");
        assert!(create(temp.path(), &path.to_string_lossy(), &doc, 50, 200)
            .expect("创建失败")
            .is_some());
        assert!(create(temp.path(), &path.to_string_lossy(), &doc, 50, 200)
            .expect("创建失败")
            .is_none());
    }

    /// 裁剪：份数上限生效，删最旧。
    #[test]
    fn prune_keeps_newest_within_count() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let (path, mut doc) = open_doc(temp.path(), b"v1\n");
        create(temp.path(), &path.to_string_lossy(), &doc, 2, 200).expect("创建失败");
        std::thread::sleep(std::time::Duration::from_millis(5));
        doc.restore_from_snapshot_bytes(b"v2\n").expect("恢复失败");
        create(temp.path(), &path.to_string_lossy(), &doc, 2, 200).expect("创建失败");
        std::thread::sleep(std::time::Duration::from_millis(5));
        doc.restore_from_snapshot_bytes(b"v3\n").expect("恢复失败");
        create(temp.path(), &path.to_string_lossy(), &doc, 2, 200).expect("创建失败");
        let items = list(temp.path(), &path.to_string_lossy());
        assert_eq!(items.len(), 2);
        let newest = read(temp.path(), &path.to_string_lossy(), &items[0].name).expect("读取失败");
        assert_eq!(newest, b"v3\n");
    }

    /// 删除幂等 + 非法名拒绝。
    #[test]
    fn delete_is_idempotent_and_rejects_bad_names() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let (path, doc) = open_doc(temp.path(), b"x\n");
        let created = create(temp.path(), &path.to_string_lossy(), &doc, 50, 200)
            .expect("创建失败")
            .expect("应有快照");
        assert!(delete(temp.path(), &path.to_string_lossy(), &created.name).expect("删除失败"));
        assert!(!delete(temp.path(), &path.to_string_lossy(), &created.name).expect("删除失败"));
        assert!(read(temp.path(), &path.to_string_lossy(), "../evil.snap").is_err());
    }

    /// 干净退出标记：首读 false → 写入后 true → 再读 false。
    #[test]
    fn clean_exit_flag_roundtrip() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        assert!(!take_clean_exit(temp.path()));
        mark_clean_exit(temp.path()).expect("标记失败");
        assert!(take_clean_exit(temp.path()));
        assert!(!take_clean_exit(temp.path()));
    }
}
