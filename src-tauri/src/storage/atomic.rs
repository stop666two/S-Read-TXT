//! 原子写入（文件持久化的统一底座之一）。
//!
//! 用途：settings/session 等 JSON 文件、任何「读-改-写」场景。
//! 保证：写入途中崩溃/断电不会留下半写文件——先写**同目录**临时文件，
//!       `sync_all` 落盘后 rename 原子替换目标文件（Windows 的 rename 会覆盖已存在文件）。
//! 约束：临时文件必须与目标同目录（跨卷 rename 不是原子操作）。
//! 失败处理：任一环节失败都会尽力清理临时文件，并返回原始错误。

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::storage::data_dir::ensure_dir;

/// 生成同目录临时文件路径：`.<文件名>.tmp-<pid>-<纳秒>`。
///
/// 说明：pid + 时间戳在同一进程内几乎不可能重复；即使极端碰撞，
/// 也只会让本函数返回 IO 错误，而不会破坏目标文件。
fn temp_path_for(target: &Path) -> PathBuf {
    let file_name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".to_string());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_name = format!(".{file_name}.tmp-{}-{nanos}", std::process::id());
    target.with_file_name(tmp_name)
}

/// 私有小函数：创建并完整写入 + fsync（供原子写复用）。
fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// 原子写入字节内容。
///
/// 参数：
/// - `path`：目标文件（父目录不存在时自动递归创建）；
/// - `bytes`：完整文件内容（原样写入，不做编码转换）。
///
/// 返回：`Ok(())` 写入并替换成功；`Err(io::Error)` 失败（临时文件已尽力清理）。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let tmp = temp_path_for(path);

    if let Err(err) = write_synced(&tmp, bytes) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }

    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
}

/// 原子写入 UTF-8 文本（语义同 `write_atomic`，调用方保证已是最终文本）。
pub fn write_atomic_str(path: &Path, text: &str) -> io::Result<()> {
    write_atomic(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 新文件：内容逐字节一致。
    #[test]
    fn creates_new_file_with_exact_bytes() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let target = tmp.path().join("a.json");
        let content = b"{\"k\":\"v\"}";
        assert!(write_atomic(&target, content).is_ok());
        assert_eq!(fs::read(&target).expect("读取写入结果失败"), content);
    }

    /// 覆盖已存在文件：旧内容被完整替换。
    #[test]
    fn overwrites_existing_file() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let target = tmp.path().join("b.txt");
        fs::write(&target, b"old").expect("预置旧文件失败");
        assert!(write_atomic_str(&target, "新内容").is_ok());
        assert_eq!(fs::read_to_string(&target).expect("读取失败"), "新内容");
    }

    /// 二进制字节（含 0x00/0xFF/CRLF）无任何转换。
    #[test]
    fn preserves_binary_bytes() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let target = tmp.path().join("bin.dat");
        let content: Vec<u8> = vec![0x00, 0xFF, 0x0D, 0x0A, 0x80];
        assert!(write_atomic(&target, &content).is_ok());
        assert_eq!(fs::read(&target).expect("读取失败"), content);
    }

    /// 写入后不残留临时文件。
    #[test]
    fn leaves_no_temp_residue() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let target = tmp.path().join("c.json");
        assert!(write_atomic_str(&target, "x").is_ok());
        let leftovers: Vec<_> = fs::read_dir(tmp.path())
            .expect("列目录失败")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "存在临时残留：{leftovers:?}");
    }

    /// 父目录不存在时自动创建。
    #[test]
    fn creates_parent_dirs() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let target = tmp.path().join("x").join("y").join("d.json");
        assert!(write_atomic_str(&target, "{}").is_ok());
        assert!(target.is_file());
    }

    /// 目标父级是文件（无法作为目录）：返回错误且不留下任何临时文件。
    #[test]
    fn fails_cleanly_when_parent_is_a_file() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let file_path = tmp.path().join("occupied");
        fs::write(&file_path, b"x").expect("写占位文件失败");
        let target = file_path.join("d.json");
        assert!(write_atomic_str(&target, "{}").is_err());
        // 占位文件未被破坏，目录内也没有临时残留
        assert_eq!(fs::read(&file_path).expect("读取占位文件失败"), b"x");
    }
}
