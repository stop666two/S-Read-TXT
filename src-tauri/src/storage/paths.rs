//! 便携数据目录解析。
//!
//! 规则（设计文档 §7，便携模式）：
//! 1. 环境变量 `SRT_DATA_DIR`（去除首尾空白后非空）优先——供测试与特殊部署；
//! 2. 否则固定为「可执行文件所在目录 / data」，绝不写入程序目录之外。
//!
//! 说明：解析是纯函数式核心 + 薄封装，便于测试而不污染进程环境变量。

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

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
    /// 持久化指针（程序目录 `config.json`；迁移功能写入，重启生效）
    Persisted,
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

/// 迁移指针文件名（位于程序目录）
pub const POINTER_FILE: &str = "config.json";

/// 迁移指针内容（`config.json`；P0-10 迁移写入，启动时读取）
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDirPointer {
    /// 指针格式版本（当前 1）
    pub schema_version: u32,
    /// 自定义数据目录绝对路径
    pub data_dir: String,
}

/// 指针文件路径（程序目录；exe 无父目录时退化为当前目录）
pub fn pointer_path(exe_path: &Path) -> PathBuf {
    exe_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(POINTER_FILE)
}

/// 读取持久化指针：缺失/损坏/版本不符/空值 → `None`（静默回退默认，不报错）。
pub fn read_pointer(exe_path: &Path) -> Option<PathBuf> {
    let raw = std::fs::read_to_string(pointer_path(exe_path)).ok()?;
    let pointer: DataDirPointer = serde_json::from_str(&raw).ok()?;
    if pointer.schema_version != 1 {
        return None;
    }
    let dir = pointer.data_dir.trim();
    if dir.is_empty() {
        return None;
    }
    Some(PathBuf::from(dir))
}

/// 写入持久化指针（原子写；由迁移流程调用）。
pub fn write_pointer(pointer_file: &Path, data_dir: &Path) -> std::io::Result<()> {
    let pointer = DataDirPointer {
        schema_version: 1,
        data_dir: data_dir.display().to_string(),
    };
    crate::storage::json_io::write_json_atomic(pointer_file, &pointer)
}

/// 纯函数核心：按「运行时覆盖 → 环境变量覆盖 → 持久化指针 → 程序目录/data」解析。
///
/// 参数：
/// - `runtime`：会话级运行时覆盖（最高优先；`None` 表示未设置）
/// - `env_override`：`SRT_DATA_DIR` 的原始值（`None` 表示未设置）
/// - `persisted`：持久化指针目标（`config.json`；`None` 表示未设置）
/// - `exe_path`：当前可执行文件路径（用于取父目录；取不到父目录时以当前目录为基准）
///
/// 返回：`(数据目录, 来源)`。空白环境变量视为未设置。
pub fn resolve_data_dir_core(
    runtime: Option<&Path>,
    env_override: Option<&str>,
    persisted: Option<&Path>,
    exe_path: &Path,
) -> (PathBuf, DataDirOrigin) {
    if let Some(dir) = runtime {
        return (dir.to_path_buf(), DataDirOrigin::RuntimeOverride);
    }
    if let Some(dir) = env_override.map(str::trim).filter(|s| !s.is_empty()) {
        return (PathBuf::from(dir), DataDirOrigin::EnvOverride);
    }
    if let Some(dir) = persisted {
        return (dir.to_path_buf(), DataDirOrigin::Persisted);
    }
    let base = exe_path.parent().unwrap_or_else(|| Path::new("."));
    (base.join("data"), DataDirOrigin::Portable)
}

/// 纯函数封装（无运行时覆盖/指针；保持既有调用与测试语义）。
pub fn resolve_data_dir_with(
    exe_path: &Path,
    env_override: Option<&str>,
) -> (PathBuf, DataDirOrigin) {
    resolve_data_dir_core(None, env_override, None, exe_path)
}

/// 读取当前进程状态并解析数据目录（运行时覆盖 > 环境变量 > 指针文件 > 便携目录）。
///
/// 边界：`current_exe()` 失败时退化为当前工作目录（仍保持“程序目录相对”的语义）。
pub fn resolve_data_dir() -> (PathBuf, DataDirOrigin) {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let persisted = read_pointer(&exe);
    resolve_data_dir_core(
        runtime_override().as_deref(),
        std::env::var("SRT_DATA_DIR").ok().as_deref(),
        persisted.as_deref(),
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

    /// 运行时覆盖优先级最高（高于环境变量、指针与便携目录）。
    #[test]
    fn runtime_override_beats_env_and_portable() {
        let (dir, origin) = resolve_data_dir_core(
            Some(Path::new("D:\\chosen-data")),
            Some("D:\\env-data"),
            Some(Path::new("D:\\pointer-data")),
            Path::new("C:\\app\\s-read-txt.exe"),
        );
        assert_eq!(dir, PathBuf::from("D:\\chosen-data"));
        assert_eq!(origin, DataDirOrigin::RuntimeOverride);
    }

    /// 指针次优先：无运行时/环境覆盖时使用指针目标，优先于便携目录。
    #[test]
    fn persisted_pointer_beats_portable() {
        let (dir, origin) = resolve_data_dir_core(
            None,
            None,
            Some(Path::new("D:\\pointer-data")),
            Path::new("C:\\app\\s-read-txt.exe"),
        );
        assert_eq!(dir, PathBuf::from("D:\\pointer-data"));
        assert_eq!(origin, DataDirOrigin::Persisted);
    }

    /// 环境变量覆盖优先于指针（测试/特殊部署语义不变）。
    #[test]
    fn env_override_beats_persisted_pointer() {
        let (dir, origin) = resolve_data_dir_core(
            None,
            Some("D:\\env-data"),
            Some(Path::new("D:\\pointer-data")),
            Path::new("C:\\app\\s-read-txt.exe"),
        );
        assert_eq!(dir, PathBuf::from("D:\\env-data"));
        assert_eq!(origin, DataDirOrigin::EnvOverride);
    }

    /// 指针读写往返；缺失/损坏/版本不符/空值 → None（回退默认）。
    #[test]
    fn pointer_roundtrip_and_corrupt_fallback() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let exe = tmp.path().join("s-read-txt.exe");
        let target = tmp.path().join("chosen-data");
        write_pointer(&pointer_path(&exe), &target).expect("写指针失败");
        assert_eq!(read_pointer(&exe), Some(target));

        let pointer_file = pointer_path(&exe);
        std::fs::write(&pointer_file, b"{ broken").expect("写损坏指针失败");
        assert_eq!(read_pointer(&exe), None);
        std::fs::write(&pointer_file, r#"{"schemaVersion":99,"dataDir":"D:\\x"}"#).expect("写失败");
        assert_eq!(read_pointer(&exe), None);
        std::fs::write(&pointer_file, r#"{"schemaVersion":1,"dataDir":"   "}"#).expect("写失败");
        assert_eq!(read_pointer(&exe), None);
        std::fs::remove_file(&pointer_file).expect("删指针失败");
        assert_eq!(read_pointer(&exe), None);
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
