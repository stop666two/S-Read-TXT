//! `settings.json` 模型（主配置）。
//! 字段、默认值与范围以 `docs/configuration.md` §2.1 为准（保持同步）。
//!
//! 注意：个别字段的 JSON 名称与 serde 的 camelCase 自动转换结果不同
//! （例如 `max_file_size_mb` 需显式映射为 `maxFileSizeMB`），
//! 一律以字段级 `#[serde(rename = ...)]` 为准并与文档对齐。

use serde::{Deserialize, Serialize};

use crate::settings::defaults;

/// 日志级别（命名对齐 RFC 5424；`Unknown` 用于向前兼容未知值）。
///
/// 语义：加载时未知/大小写异常取值归入 `Unknown`，再经 [`LogLevel::normalized`]
/// 归一为默认级别；保存前必先归一，故 `Unknown` 不会落盘。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// 仅错误
    Error,
    /// 警告及以上
    Warn,
    /// 信息及以上（默认）
    Info,
    /// 调试（最详细）
    Debug,
    /// 未知取值（向前兼容；载入时归一为默认）
    Unknown,
}

/// 手写反序列化：未知字符串宽容归入 `Unknown`，而不是让整份配置解析失败。
///
/// 实现：显式匹配（trim + 小写归一），不依赖 serde 对未知枚举值的默认报错行为。
impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "error" => Self::Error,
            "warn" => Self::Warn,
            "info" => Self::Info,
            "debug" => Self::Debug,
            _ => Self::Unknown,
        })
    }
}

impl Default for LogLevel {
    fn default() -> Self {
        defaults::DEFAULT_LOG_LEVEL
    }
}

impl LogLevel {
    /// 归一：`Unknown` 回退默认级别，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_LOG_LEVEL
        } else {
            self
        }
    }
}

/// 历史保留策略（`settings.json` 的嵌套对象 `history`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistorySettings {
    /// 保留条数上限（范围见 [`defaults::HISTORY_MAX_ENTRIES_RANGE`]）
    pub max_entries: u32,
    /// 保留天数（范围见 [`defaults::HISTORY_RETENTION_DAYS_RANGE`]）
    pub retention_days: u32,
}

impl Default for HistorySettings {
    fn default() -> Self {
        Self {
            max_entries: defaults::DEFAULT_HISTORY_MAX_ENTRIES,
            retention_days: defaults::DEFAULT_HISTORY_RETENTION_DAYS,
        }
    }
}

/// 主配置（`settings.json`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// 配置格式版本（保存时写入当前 [`defaults::SCHEMA_VERSION`]）
    pub schema_version: u32,
    /// 日志级别（环境变量 `SRT_LOG_LEVEL` 优先于本字段）
    pub log_level: LogLevel,
    /// 可打开文件大小上限（MB）。
    /// JSON 名显式固定为 `maxFileSizeMB`（serde camelCase 会自动生成 `maxFileSizeMb`，与文档不符）。
    #[serde(rename = "maxFileSizeMB")]
    pub max_file_size_mb: u32,
    /// 标签数量上限
    pub max_tabs: u32,
    /// 历史保留策略
    pub history: HistorySettings,
    /// 首次保存前是否生成 `.bak` 备份
    pub save_backup_enabled: bool,
    /// 是否显示首启引导（用户勾选「不再显示」后置 `false`）
    pub show_onboarding: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: defaults::SCHEMA_VERSION,
            log_level: defaults::DEFAULT_LOG_LEVEL,
            max_file_size_mb: defaults::DEFAULT_MAX_FILE_SIZE_MB,
            max_tabs: defaults::DEFAULT_MAX_TABS,
            history: HistorySettings::default(),
            save_backup_enabled: defaults::DEFAULT_SAVE_BACKUP_ENABLED,
            show_onboarding: defaults::DEFAULT_SHOW_ONBOARDING,
        }
    }
}
