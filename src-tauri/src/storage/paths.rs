//! 便携数据目录解析。
//!
//! 规则（设计文档 §7，便携模式）：
//! 1. 环境变量 `SRT_DATA_DIR`（去除首尾空白后非空）优先——供测试与特殊部署；
//! 2. 否则固定为「可执行文件所在目录 / data」，绝不写入程序目录之外。
//!
//! 说明：解析是纯函数式核心 + 薄封装，便于测试而不污染进程环境变量。

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::Serialize;

/// 数据目录来源（序列化给前端展示，camelCase：portable / envOverride）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DataDirOrigin {
    /// 便携模式：程序目录下的 data/（默认）
    Portable,
    /// 环境变量 `SRT_DATA_DIR` 覆盖
    EnvOverride,
    /// 会话级运行时覆盖（数据目录不可写时由用户选择；仅本次运行有效）
    RuntimeOverride,
}

/// 会话级运行时覆盖（数据目录不可写时由用户选择；仅本次运行有效）。
/// 进程级单例：设置后所有后续解析（设置/历史/会话/日志）都改路至新目录。
static RUNTIME_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);

/// 设置运行时覆盖（调用方须先完成可写性探测）。
pub fn set_runtime_override(dir: PathBuf) {
    let mut guard = RUNTIME_OVERRIDE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(dir);
}

/// 清除运行时覆盖（测试/诊断用；生产流程仅在进程退出时自然失效）。
pub fn clear_runtime_override() {
    let mut guard = RUNTIME_OVERRIDE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = None;
}

/// 当前运行时覆盖（诊断/测试用）。
pub fn runtime_override() -> Option<PathBuf> {
    RUNTIME_OVERRIDE
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// 纯函数核心：按「运行时覆盖 → 环境变量覆盖 → 程序目录/data」解析数据目录。
///
/// 参数：
/// - `runtime`：会话级运行时覆盖（最高优先；`None` 表示未设置）
/// - `env_override`：`SRT_DATA_DIR` 的原始值（`None` 表示未设置）
/// - `exe_path`：当前可执行文件路径（用于取父目录；取不到父目录时以当前目录为基准）
///
/// 返回：`(数据目录, 来源)`。空白环境变量视为未设置。
pub fn resolve_data_dir_core(
    runtime: Option<&Path>,
    env_override: Option<&str>,
    exe_path: &Path,
) -> (PathBuf, DataDirOrigin) {
    if let Some(dir) = runtime {
        return (dir.to_path_buf(), DataDirOrigin::RuntimeOverride);
    }
    if let Some(dir) = env_override.map(str::trim).filter(|s| !s.is_empty()) {
        return (PathBuf::from(dir), DataDirOrigin::EnvOverride);
    }
    let base = exe_path.parent().unwrap_or_else(|| Path::new("."));
    (base.join("data"), DataDirOrigin::Portable)
}

/// 纯函数封装（无运行时覆盖；保持既有调用与测试语义）。
pub fn resolve_data_dir_with(
    exe_path: &Path,
    env_override: Option<&str>,
) -> (PathBuf, DataDirOrigin) {
    resolve_data_dir_core(None, env_override, exe_path)
}

/// 读取当前进程状态并解析数据目录（运行时覆盖 > 环境变量 > 便携目录）。
///
/// 边界：`current_exe()` 失败时退化为当前工作目录（仍保持“程序目录相对”的语义）。
pub fn resolve_data_dir() -> (PathBuf, DataDirOrigin) {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    resolve_data_dir_core(
        runtime_override().as_deref(),
        std::env::var("SRT_DATA_DIR").ok().as_deref(),
        &exe,
    )
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

    /// 运行时覆盖优先级最高（高于环境变量与便携目录）。
    #[test]
    fn runtime_override_beats_env_and_portable() {
        let (dir, origin) = resolve_data_dir_core(
            Some(Path::new("D:\\chosen-data")),
            Some("D:\\env-data"),
            Path::new("C:\\app\\s-read-txt.exe"),
        );
        assert_eq!(dir, PathBuf::from("D:\\chosen-data"));
        assert_eq!(origin, DataDirOrigin::RuntimeOverride);
    }

    /// 运行时覆盖与进程级单例的读写一致（设置/读取/清理）。
    #[test]
    fn runtime_override_singleton_roundtrip() {
        set_runtime_override(PathBuf::from("D:\\srt-runtime-override"));
        assert_eq!(
            runtime_override(),
            Some(PathBuf::from("D:\\srt-runtime-override"))
        );
        // 清理，避免影响其他读取全局状态的用例
        clear_runtime_override();
        assert_eq!(runtime_override(), None);
    }
}
