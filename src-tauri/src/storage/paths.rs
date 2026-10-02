//! 便携数据目录解析。
//!
//! 规则（设计文档 §7，便携模式）：
//! 1. 环境变量 `SRT_DATA_DIR`（去除首尾空白后非空）优先——供测试与特殊部署；
//! 2. 否则固定为「可执行文件所在目录 / data」，绝不写入程序目录之外。
//!
//! 说明：解析是纯函数式核心 + 薄封装，便于测试而不污染进程环境变量。

use std::path::{Path, PathBuf};

use serde::Serialize;

/// 数据目录来源（序列化给前端展示，camelCase：portable / envOverride）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DataDirOrigin {
    /// 便携模式：程序目录下的 data/（默认）
    Portable,
    /// 环境变量 `SRT_DATA_DIR` 覆盖
    EnvOverride,
}

/// 纯函数核心：按「环境变量覆盖 → 程序目录/data」解析数据目录。
///
/// 参数：
/// - `exe_path`：当前可执行文件路径（用于取父目录；取不到父目录时以当前目录为基准）
/// - `env_override`：`SRT_DATA_DIR` 的原始值（`None` 表示未设置）
///
/// 返回：`(数据目录, 来源)`。空白字符串视为未设置。
pub fn resolve_data_dir_with(
    exe_path: &Path,
    env_override: Option<&str>,
) -> (PathBuf, DataDirOrigin) {
    if let Some(dir) = env_override.map(str::trim).filter(|s| !s.is_empty()) {
        return (PathBuf::from(dir), DataDirOrigin::EnvOverride);
    }
    let base = exe_path.parent().unwrap_or_else(|| Path::new("."));
    (base.join("data"), DataDirOrigin::Portable)
}

/// 读取当前进程环境并解析数据目录（唯一的全局环境读取点）。
///
/// 返回：`(数据目录, 来源)`。
/// 边界：`current_exe()` 失败时退化为当前工作目录（仍保持“程序目录相对”的语义）。
pub fn resolve_data_dir() -> (PathBuf, DataDirOrigin) {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    resolve_data_dir_with(&exe, std::env::var("SRT_DATA_DIR").ok().as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 环境变量覆盖优先，并且去除首尾空白。
    #[test]
    fn env_override_takes_priority_and_trims() {
        let (dir, origin) = resolve_data_dir_with(
            Path::new("C:\\app\\s-read-txt.exe"),
            Some("  D:\\srt-data  "),
        );
        assert_eq!(dir, PathBuf::from("D:\\srt-data"));
        assert_eq!(origin, DataDirOrigin::EnvOverride);
    }

    /// 空白环境变量视为未设置，回退便携目录。
    #[test]
    fn blank_env_falls_back_to_portable() {
        let (dir, origin) =
            resolve_data_dir_with(Path::new("C:\\app\\s-read-txt.exe"), Some("   "));
        assert_eq!(dir, PathBuf::from("C:\\app\\data"));
        assert_eq!(origin, DataDirOrigin::Portable);
    }

    /// 默认便携目录 = 程序目录 / data。
    #[test]
    fn portable_dir_is_exe_parent_data() {
        let (dir, origin) =
            resolve_data_dir_with(Path::new("D:\\tools\\S-Read-TXT\\s-read-txt.exe"), None);
        assert_eq!(dir, PathBuf::from("D:\\tools\\S-Read-TXT\\data"));
        assert_eq!(origin, DataDirOrigin::Portable);
    }

    /// exe 路径无父目录时的退化行为：仍返回可用的相对 data 目录。
    #[test]
    fn missing_parent_falls_back_to_relative() {
        let (dir, _origin) = resolve_data_dir_with(Path::new("s-read-txt.exe"), None);
        assert_eq!(dir, PathBuf::from("data"));
    }
}
