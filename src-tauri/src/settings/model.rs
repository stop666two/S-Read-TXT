//! `settings.json` 模型（主配置）。
//! 字段、默认值与范围以 `docs/configuration.md` §2.1 为准（保持同步）。
//!
//! 注意：个别字段的 JSON 名称与 serde 的 camelCase 自动转换结果不同
//! （例如 `max_file_size_mb` 需显式映射为 `maxFileSizeMB`），
//! 一律以字段级 `#[serde(rename = ...)]` 为准并与文档对齐。

use serde::{Deserialize, Serialize};

use crate::settings::defaults;
use crate::settings::display::DisplaySettings;
use crate::settings::editor::EditorSettings;
use crate::settings::file::FileSettings;
use crate::settings::status::StatusSettings;

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

/// 界面语言（BCP 47 语言标签；命名对齐国际标准）。
///
/// 语义与 [`LogLevel`] 相同：加载时未知/大小写异常取值归入 `Unknown`，
/// 再经 [`Language::normalized`] 归一为默认（简体中文）；保存前必先归一，
/// 故 `Unknown` 不会落盘。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Language {
    /// 简体中文（zh-CN，默认）
    #[serde(rename = "zh-CN")]
    ZhCn,
    /// 英语（en）
    #[serde(rename = "en")]
    En,
    /// 未知取值（向前兼容；载入时归一为默认）
    #[serde(rename = "unknown")]
    Unknown,
}

/// 手写反序列化：未知字符串宽容归入 `Unknown`（避免整份配置回退）。
/// 接受常见变体（`zh`、`zh-Hans`、`en-US` 等），统一小写后匹配。
impl<'de> Deserialize<'de> for Language {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "zh-cn" | "zh" | "zh-hans" => Self::ZhCn,
            "en" | "en-us" | "en-gb" => Self::En,
            _ => Self::Unknown,
        })
    }
}

impl Default for Language {
    fn default() -> Self {
        defaults::DEFAULT_LOCALE
    }
}

impl Language {
    /// 归一：`Unknown` 回退默认语言，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_LOCALE
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

/// 启动行为（`settings.json` 的嵌套对象 `startup`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StartupSettings {
    /// 启动时恢复上次会话（重新打开标签）
    pub restore_session: bool,
    /// 启动时恢复窗口位置与大小
    pub restore_window: bool,
}

impl Default for StartupSettings {
    fn default() -> Self {
        Self {
            restore_session: defaults::DEFAULT_STARTUP_RESTORE_SESSION,
            restore_window: defaults::DEFAULT_STARTUP_RESTORE_WINDOW,
        }
    }
}

/// 查找范围默认值（`app.find.defaultScope`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FindScope {
    /// 全文
    Document,
    /// 选区（进入查找时存在选中文本）
    Selection,
    /// 行区间（由查找条指定行号范围）
    RowRange,
    /// 未知取值（向前兼容；载入时归一为默认）
    Unknown,
}

/// 手写反序列化：宽容未知/别名取值（`all`→全文、`range`→行区间），其余归 `Unknown`。
impl<'de> Deserialize<'de> for FindScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "document" | "all" => Self::Document,
            "selection" | "selected" => Self::Selection,
            "rowrange" | "row-range" | "range" => Self::RowRange,
            _ => Self::Unknown,
        })
    }
}

impl Default for FindScope {
    fn default() -> Self {
        defaults::DEFAULT_FIND_SCOPE
    }
}

impl FindScope {
    /// 归一：`Unknown` 回退默认（全文），其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_FIND_SCOPE
        } else {
            self
        }
    }
}

/// 查找与替换设置（`settings.json` 的嵌套对象 `find`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FindSettings {
    /// 默认区分大小写
    pub case_sensitive: bool,
    /// 默认全词匹配（仅字面模式生效）
    pub whole_word: bool,
    /// 循环查找：到达文末后从文首继续
    pub wrap_around: bool,
    /// 高亮全部匹配
    pub highlight_all: bool,
    /// 显示匹配计数
    pub match_count: bool,
    /// 替换前预览确认
    pub replace_preview: bool,
    /// 查找范围默认值
    pub default_scope: FindScope,
    /// 查找历史条数上限（0 = 不留历史）
    pub history_limit: u32,
    /// 匹配高亮颜色（空串 = 跟随主题内置色）
    pub highlight_color: String,
    /// 是否启用多文件（工作区）搜索
    pub multifile_enabled: bool,
    /// 多文件搜索并发数（同时扫描的只读标签数，1–16）
    pub multifile_concurrency: u32,
}

impl Default for FindSettings {
    fn default() -> Self {
        Self {
            case_sensitive: defaults::DEFAULT_FIND_CASE_SENSITIVE,
            whole_word: defaults::DEFAULT_FIND_WHOLE_WORD,
            wrap_around: defaults::DEFAULT_FIND_WRAP_AROUND,
            highlight_all: defaults::DEFAULT_FIND_HIGHLIGHT_ALL,
            match_count: defaults::DEFAULT_FIND_MATCH_COUNT,
            replace_preview: defaults::DEFAULT_FIND_REPLACE_PREVIEW,
            default_scope: defaults::DEFAULT_FIND_SCOPE,
            history_limit: defaults::DEFAULT_FIND_HISTORY_LIMIT,
            highlight_color: defaults::DEFAULT_FIND_HIGHLIGHT_COLOR.to_string(),
            multifile_enabled: defaults::DEFAULT_FIND_MULTIFILE_ENABLED,
            multifile_concurrency: defaults::DEFAULT_FIND_MULTIFILE_CONCURRENCY,
        }
    }
}

/// 正则设置（`settings.json` 的嵌套对象 `regex`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RegexSettings {
    /// 单次扫描超时（毫秒；超时中断并提示，不修改内容）
    pub timeout_ms: u32,
    /// 常用正则库（逐项编译校验；可增删改排序）
    pub library: Vec<String>,
}

impl Default for RegexSettings {
    fn default() -> Self {
        Self {
            timeout_ms: defaults::DEFAULT_REGEX_TIMEOUT_MS,
            library: Vec::new(),
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
    /// 语义：只读阈值——超过此大小以只读模式打开（可浏览、不可编辑）。
    #[serde(rename = "maxFileSizeMB")]
    pub max_file_size_mb: u32,
    /// 硬上限（MB）：超过此大小拒绝打开（绝对上限，正常应 ≥ 只读阈值）。
    /// JSON 名显式固定为 `hardLimitMB`（与 `maxFileSizeMB` 同理）。
    #[serde(rename = "hardLimitMB")]
    pub hard_limit_mb: u32,
    /// 标签数量上限
    pub max_tabs: u32,
    /// 历史保留策略
    pub history: HistorySettings,
    /// 首次保存前是否生成 `.bak` 备份
    pub save_backup_enabled: bool,
    /// 是否显示首启引导（用户勾选「不再显示」后置 `false`）
    pub show_onboarding: bool,
    /// 界面语言（BCP 47 标签：`zh-CN` / `en`）
    pub locale: Language,
    /// 状态栏显示与交互
    pub status: StatusSettings,
    /// 显示选项（行号/高亮/标尺/不可见字符等，P2-2）
    pub display: DisplaySettings,
    /// 启动行为
    pub startup: StartupSettings,
    /// 编辑器默认值（行操作等）
    pub editor: EditorSettings,
    /// 文件与快照设置
    pub file: FileSettings,
    /// 查找与替换默认值
    pub find: FindSettings,
    /// 正则设置
    pub regex: RegexSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: defaults::SCHEMA_VERSION,
            log_level: defaults::DEFAULT_LOG_LEVEL,
            max_file_size_mb: defaults::DEFAULT_MAX_FILE_SIZE_MB,
            hard_limit_mb: defaults::DEFAULT_HARD_LIMIT_MB,
            max_tabs: defaults::DEFAULT_MAX_TABS,
            history: HistorySettings::default(),
            save_backup_enabled: defaults::DEFAULT_SAVE_BACKUP_ENABLED,
            show_onboarding: defaults::DEFAULT_SHOW_ONBOARDING,
            locale: defaults::DEFAULT_LOCALE,
            status: StatusSettings::default(),
            display: DisplaySettings::default(),
            startup: StartupSettings::default(),
            editor: EditorSettings::default(),
            file: FileSettings::default(),
            find: FindSettings::default(),
            regex: RegexSettings::default(),
        }
    }
}
