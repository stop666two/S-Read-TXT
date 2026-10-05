//! 数据目录保障与可写性探测。
//!
//! 用途（设计文档 §7）：
//! - 启动时探测程序目录/data 是否可写；
//! - 不可写时由上层引导用户选择可写目录（会话级生效）；
//! - 所有持久化模块在写入前可调用 `ensure_dir` 保障目录存在。
//!
//! 实现说明：可写性采用「真实写探针」而不是权限位推断——
//! 权限位在 Windows（ACL/只读属性/受控文件夹访问）下的判断会导致误判。

use std::fs;
use std::path::Path;

/// 探针文件名（带点前缀，降低误删用户文件概率；探测后即删除）。
const PROBE_FILE_NAME: &str = ".srt-write-probe";

/// 保障目录存在（递归创建；已存在时无操作）。
///
/// 参数：`dir` 目标目录。
/// 返回：`Ok(())` 目录可用；`Err(io::Error)` 创建失败（附系统错误）。
pub fn ensure_dir(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)
}

/// 探测目录是否可写：创建目录 → 写入探针文件 → 删除探针。
///
/// 参数：`dir` 目标目录。
/// 返回：`Ok(())` 可写；`Err(io::Error)` 不可写（错误信息含具体原因，供前端提示与日志）。
/// 边界：探针删除失败不视为不可写（例如被杀软临时锁定），仅忽略，
///       因为「能写入」已经成立；残留探针文件由下次探测覆盖。
pub fn probe_writable(dir: &Path) -> std::io::Result<()> {
    ensure_dir(dir)?;
    let probe = dir.join(PROBE_FILE_NAME);
    fs::write(&probe, b"s-read-txt write probe")?;
    let _ = fs::remove_file(&probe);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 临时目录探测：可写路径返回 Ok。
    #[test]
    fn writable_temp_dir_passes() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        assert!(probe_writable(tmp.path()).is_ok());
    }

    /// 目标路径的父级是文件（无法创建目录）时探测失败。
    #[test]
    fn probe_under_file_fails() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let file_path = tmp.path().join("not-a-dir");
        fs::write(&file_path, b"x").expect("写占位文件失败");
        // 以「文件路径 / data」作为目录探测，create_dir_all 必然失败
        let bad_dir = file_path.join("data");
        assert!(probe_writable(&bad_dir).is_err());
    }

    /// ensure_dir 递归创建成功且重复调用幂等。
    #[test]
    fn ensure_dir_is_idempotent() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let nested = tmp.path().join("a").join("b").join("c");
        assert!(ensure_dir(&nested).is_ok());
        assert!(ensure_dir(&nested).is_ok());
        assert!(nested.is_dir());
    }

    /// 探测成功后不应残留探针文件。
    #[test]
    fn probe_leaves_no_residue() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        assert!(probe_writable(tmp.path()).is_ok());
        let probe = tmp.path().join(PROBE_FILE_NAME);
        assert!(!probe.exists());
    }
}
