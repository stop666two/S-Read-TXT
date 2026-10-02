//! 日志模块：JSON 行文件日志 + 大小轮转 + 级别开关 + 本地链路上下文。
//!
//! 权威格式说明见 `docs/configuration.md` §2.6（保持同步）。
//! 级别优先级：`SRT_LOG_LEVEL` 环境变量 > `settings.json` 的 `logLevel` > 默认 `info`。

pub mod context;
pub mod file_logger;

use std::path::{Path, PathBuf};
use std::sync::Once;

use crate::settings::model::LogLevel;

pub use file_logger::{FacadeLogger, FileLogger, RotationConfig};

/// 日志目录：`<数据目录>/logs`。
pub fn logs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

/// 安装全局 logger（进程级仅一次；重复调用为无操作）。
///
/// 返回：`Ok(())` 即使全局已安装（幂等）；`Err(io::Error)` 表示日志文件无法创建，
///       此时应用继续运行（降级为无文件日志）。
pub fn init(data_dir: &Path, settings_level: LogLevel) -> std::io::Result<()> {
    static INSTALLED: Once = Once::new();
    let level = resolve_level_with(
        settings_level,
        std::env::var("SRT_LOG_LEVEL").ok().as_deref(),
    );
    let logger = FileLogger::open(&logs_dir(data_dir), RotationConfig::default())?;
    INSTALLED.call_once(|| {
        if let Err(err) = log::set_boxed_logger(Box::new(FacadeLogger(logger))) {
            // 仅可能发生在“全局 logger 已被安装”（例如测试环境）
            eprintln!("[s-read-txt] 全局日志安装失败（可能已安装）：{err}");
        } else {
            log::set_max_level(level);
        }
    });
    Ok(())
}

/// 级别解析（纯函数，便于测试）：
/// 环境变量为空白或非法值时忽略，回退 `settings.json` 的级别。
pub fn resolve_level_with(settings_level: LogLevel, env_value: Option<&str>) -> log::LevelFilter {
    let from_env = env_value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(parse_level);
    from_env.unwrap_or_else(|| to_filter(settings_level))
}

/// 解析级别字符串（大小写不敏感；非法值返回 None）。
fn parse_level(raw: &str) -> Option<log::LevelFilter> {
    match raw.to_ascii_lowercase().as_str() {
        "error" => Some(log::LevelFilter::Error),
        "warn" => Some(log::LevelFilter::Warn),
        "info" => Some(log::LevelFilter::Info),
        "debug" => Some(log::LevelFilter::Debug),
        _ => None,
    }
}

/// 配置枚举 → 过滤级别（`Unknown` 防御性回退 `Info`）。
pub fn to_filter(level: LogLevel) -> log::LevelFilter {
    match level {
        LogLevel::Error => log::LevelFilter::Error,
        LogLevel::Warn => log::LevelFilter::Warn,
        LogLevel::Info | LogLevel::Unknown => log::LevelFilter::Info,
        LogLevel::Debug => log::LevelFilter::Debug,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 环境变量覆盖 settings，且大小写不敏感。
    #[test]
    fn env_value_overrides_settings() {
        assert_eq!(
            resolve_level_with(LogLevel::Error, Some("debug")),
            log::LevelFilter::Debug
        );
        assert_eq!(
            resolve_level_with(LogLevel::Error, Some("DEBUG")),
            log::LevelFilter::Debug
        );
    }

    /// 空白/非法环境变量回退 settings；未设置时直接用 settings。
    #[test]
    fn blank_or_invalid_env_falls_back_to_settings() {
        assert_eq!(
            resolve_level_with(LogLevel::Warn, Some("  ")),
            log::LevelFilter::Warn
        );
        assert_eq!(
            resolve_level_with(LogLevel::Warn, Some("bogus")),
            log::LevelFilter::Warn
        );
        assert_eq!(
            resolve_level_with(LogLevel::Debug, None),
            log::LevelFilter::Debug
        );
    }

    /// 日志目录 = 数据目录 / logs。
    #[test]
    fn logs_dir_appends_logs_folder() {
        let base = Path::new("D:\\app\\data");
        assert_eq!(logs_dir(base), base.join("logs"));
    }
}
