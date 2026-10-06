//! `settings.json` 的 `editor` 节模型（编辑器批量与行操作默认值）。
//!
//! 字段、默认值与范围以 `docs/configuration.md` 为准（保持同步）。
//! 设计对齐：设置规格 §3（L 组，editor.lines.*）；枚举一律采用
//! 「未知值宽容解析 + 归一」模式（与 [`crate::settings::model::LogLevel`] 一致），
//! 避免单个异常取值导致整份配置回退默认。

use serde::{Deserialize, Serialize};

use crate::settings::defaults;

/// 行操作默认作用范围（对应设置 `editor.lines.defaultScope`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LineScopeKind {
    /// 全文（默认）
    #[serde(rename = "all")]
    All,
    /// 当前行
    #[serde(rename = "currentLine")]
    CurrentLine,
    /// 行范围（对话框内填写）
    #[serde(rename = "rowRange")]
    RowRange,
    /// 非空行
    #[serde(rename = "nonEmpty")]
    NonEmpty,
    /// 手动选区
    #[serde(rename = "selection")]
    Selection,
    /// 未知取值（向前兼容；载入时归一为默认）
    #[serde(rename = "unknown")]
    Unknown,
}

/// 手写反序列化：trim + 小写归一后匹配；未知 → `Unknown`。
impl<'de> Deserialize<'de> for LineScopeKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "all" => Self::All,
            "currentline" => Self::CurrentLine,
            "rowrange" => Self::RowRange,
            "nonempty" => Self::NonEmpty,
            "selection" => Self::Selection,
            _ => Self::Unknown,
        })
    }
}

impl Default for LineScopeKind {
    fn default() -> Self {
        defaults::DEFAULT_LINE_SCOPE
    }
}

impl LineScopeKind {
    /// 归一：`Unknown` 回退默认范围，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_LINE_SCOPE
        } else {
            self
        }
    }
}

/// 默认排序方式（设置 `editor.lines.sortMode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LineSortMode {
    /// 字典序（按 Unicode 码点，默认）
    #[serde(rename = "lex")]
    Lex,
    /// 自然排序（数字段按数值比较）
    #[serde(rename = "natural")]
    Natural,
    /// 按行长度（先长度、同长按字典序）
    #[serde(rename = "length")]
    Length,
    /// 未知取值（向前兼容；载入时归一为默认）
    #[serde(rename = "unknown")]
    Unknown,
}

impl<'de> Deserialize<'de> for LineSortMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "lex" => Self::Lex,
            "natural" => Self::Natural,
            "length" => Self::Length,
            _ => Self::Unknown,
        })
    }
}

impl Default for LineSortMode {
    fn default() -> Self {
        defaults::DEFAULT_LINE_SORT_MODE
    }
}

impl LineSortMode {
    /// 归一：`Unknown` 回退默认方式，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_LINE_SORT_MODE
        } else {
            self
        }
    }
}

/// 去重规则（设置 `editor.lines.dedupeMode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LineDedupeMode {
    /// 保留首次出现（默认）
    #[serde(rename = "keepFirst")]
    KeepFirst,
    /// 保留末次出现
    #[serde(rename = "keepLast")]
    KeepLast,
    /// 未知取值（向前兼容；载入时归一为默认）
    #[serde(rename = "unknown")]
    Unknown,
}

impl<'de> Deserialize<'de> for LineDedupeMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "keepfirst" => Self::KeepFirst,
            "keeplast" => Self::KeepLast,
            _ => Self::Unknown,
        })
    }
}

impl Default for LineDedupeMode {
    fn default() -> Self {
        defaults::DEFAULT_LINE_DEDUPE_MODE
    }
}

impl LineDedupeMode {
    /// 归一：`Unknown` 回退默认规则，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_LINE_DEDUPE_MODE
        } else {
            self
        }
    }
}

/// 缩进字符（设置 `editor.lines.indentStyle`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LineIndentStyle {
    /// 空格（默认）
    #[serde(rename = "spaces")]
    Spaces,
    /// 制表符
    #[serde(rename = "tab")]
    Tab,
    /// 未知取值（向前兼容；载入时归一为默认）
    #[serde(rename = "unknown")]
    Unknown,
}

impl<'de> Deserialize<'de> for LineIndentStyle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "spaces" => Self::Spaces,
            "tab" => Self::Tab,
            _ => Self::Unknown,
        })
    }
}

impl Default for LineIndentStyle {
    fn default() -> Self {
        defaults::DEFAULT_LINE_INDENT_STYLE
    }
}

impl LineIndentStyle {
    /// 归一：`Unknown` 回退默认字符，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_LINE_INDENT_STYLE
        } else {
            self
        }
    }
}

/// 大小写转换模式（设置 `editor.lines.caseDefault`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LineCaseMode {
    /// 转大写
    #[serde(rename = "upper")]
    Upper,
    /// 转小写（默认）
    #[serde(rename = "lower")]
    Lower,
    /// 标题式（每个单词首字母大写）
    #[serde(rename = "title")]
    Title,
    /// 未知取值（向前兼容；载入时归一为默认）
    #[serde(rename = "unknown")]
    Unknown,
}

impl<'de> Deserialize<'de> for LineCaseMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "upper" => Self::Upper,
            "lower" => Self::Lower,
            "title" => Self::Title,
            _ => Self::Unknown,
        })
    }
}

impl Default for LineCaseMode {
    fn default() -> Self {
        defaults::DEFAULT_LINE_CASE_MODE
    }
}

impl LineCaseMode {
    /// 归一：`Unknown` 回退默认模式，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_LINE_CASE_MODE
        } else {
            self
        }
    }
}

/// 行操作默认值（`settings.json` 的嵌套对象 `editor.lines`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LineOpsSettings {
    /// 默认作用范围
    pub default_scope: LineScopeKind,
    /// 默认排序方式
    pub sort_mode: LineSortMode,
    /// 去重规则
    pub dedupe_mode: LineDedupeMode,
    /// 去重是否忽略大小写
    pub dedupe_ignore_case: bool,
    /// 去重是否模糊匹配（NFKC + 忽略空白）
    pub dedupe_fuzzy: bool,
    /// 缩进宽度（空格数，范围见 [`defaults::LINE_INDENT_WIDTH_RANGE`]）
    pub indent_width: u32,
    /// 缩进字符
    pub indent_style: LineIndentStyle,
    /// 大小写转换默认模式
    pub case_default: LineCaseMode,
    /// 列编辑默认分隔符（首字符集，最长 [`defaults::LINE_COLUMN_DELIMITER_MAX_CHARS`]）
    pub column_delimiter: String,
    /// 高风险操作默认先预览
    pub preview: bool,
    /// 行操作是否默认跳过空行
    pub skip_empty_lines: bool,
}

impl Default for LineOpsSettings {
    fn default() -> Self {
        Self {
            default_scope: defaults::DEFAULT_LINE_SCOPE,
            sort_mode: defaults::DEFAULT_LINE_SORT_MODE,
            dedupe_mode: defaults::DEFAULT_LINE_DEDUPE_MODE,
            dedupe_ignore_case: defaults::DEFAULT_LINE_DEDUPE_IGNORE_CASE,
            dedupe_fuzzy: defaults::DEFAULT_LINE_DEDUPE_FUZZY,
            indent_width: defaults::DEFAULT_LINE_INDENT_WIDTH,
            indent_style: defaults::DEFAULT_LINE_INDENT_STYLE,
            case_default: defaults::DEFAULT_LINE_CASE_MODE,
            column_delimiter: defaults::DEFAULT_LINE_COLUMN_DELIMITER.to_string(),
            preview: defaults::DEFAULT_LINE_PREVIEW,
            skip_empty_lines: defaults::DEFAULT_LINE_SKIP_EMPTY,
        }
    }
}

/// 矩形（列）选择修饰键。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RectModifier {
    /// `Alt`（默认）
    Alt,
    /// `Ctrl+Alt`
    CtrlAlt,
    /// 未知取值（向前兼容；载入时归一为默认）
    Unknown,
}

/// 手写反序列化：未知字符串宽容归入 `Unknown`。
impl<'de> Deserialize<'de> for RectModifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "alt" => Self::Alt,
            "ctrlalt" | "ctrl+alt" => Self::CtrlAlt,
            _ => Self::Unknown,
        })
    }
}

impl Default for RectModifier {
    fn default() -> Self {
        defaults::DEFAULT_MULTI_CURSOR_RECT_MODIFIER
    }
}

impl RectModifier {
    /// 归一：`Unknown` 回退默认，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_MULTI_CURSOR_RECT_MODIFIER
        } else {
            self
        }
    }
}

/// 多光标设置（`editor.multiCursor`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MultiCursorSettings {
    /// 是否启用多光标与矩形选择
    pub enabled: bool,
    /// 矩形选择修饰键
    pub rect_modifier: RectModifier,
    /// 多光标数量上限（性能保护）
    pub max_count: u32,
}

impl Default for MultiCursorSettings {
    fn default() -> Self {
        Self {
            enabled: defaults::DEFAULT_MULTI_CURSOR_ENABLED,
            rect_modifier: defaults::DEFAULT_MULTI_CURSOR_RECT_MODIFIER,
            max_count: defaults::DEFAULT_MULTI_CURSOR_MAX_COUNT,
        }
    }
}

/// 剪贴板历史设置（`editor.clipboard`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClipboardSettings {
    /// 历史上限（条；0 = 禁用，范围见 [`defaults::CLIPBOARD_HISTORY_LIMIT_RANGE`]）
    pub history_limit: u32,
    /// 是否持久化到数据目录（关闭时仅进程内会话内存）
    pub persist: bool,
    /// 单条文本字符上限（超出截断；范围见 [`defaults::CLIPBOARD_ENTRY_MAX_CHARS_RANGE`]）
    pub entry_max_chars: u32,
}

impl Default for ClipboardSettings {
    fn default() -> Self {
        Self {
            history_limit: defaults::DEFAULT_CLIPBOARD_HISTORY_LIMIT,
            persist: defaults::DEFAULT_CLIPBOARD_PERSIST,
            entry_max_chars: defaults::DEFAULT_CLIPBOARD_ENTRY_MAX_CHARS,
        }
    }
}

/// 时间戳插入格式（`editor.insert.timestampFormat`）。
///
/// 宽容反序列化：未知取值归入 [`TimestampFormat::Unknown`]，保存前经
/// [`TimestampFormat::normalized`] 回退默认；`Unknown` 不会落盘。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TimestampFormat {
    /// 本地日期时间（`YYYY-MM-DD HH:mm:ss`，默认）
    LocalDateTime,
    /// 仅日期（`YYYY-MM-DD`）
    DateOnly,
    /// 仅时间（`HH:mm:ss`）
    TimeOnly,
    /// ISO 8601（本地时间带偏移，如 `2026-10-03T14:30:00+08:00`）
    Iso8601,
    /// RFC 3339（UTC，`2026-10-03T06:30:00Z`）
    Rfc3339Utc,
    /// 未知取值（向前兼容；载入时归一为默认）
    Unknown,
}

/// 手写反序列化：未知字符串宽容归入 `Unknown`（避免整份配置回退）。
impl<'de> Deserialize<'de> for TimestampFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "localdatetime" | "local" => Self::LocalDateTime,
            "dateonly" | "date" => Self::DateOnly,
            "timeonly" | "time" => Self::TimeOnly,
            "iso8601" | "iso" => Self::Iso8601,
            "rfc3339utc" | "rfc3339" | "utc" => Self::Rfc3339Utc,
            _ => Self::Unknown,
        })
    }
}

impl Default for TimestampFormat {
    fn default() -> Self {
        defaults::DEFAULT_TIMESTAMP_FORMAT
    }
}

impl TimestampFormat {
    /// 归一：`Unknown` 回退默认，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_TIMESTAMP_FORMAT
        } else {
            self
        }
    }
}

/// 时间戳插入设置（`editor.insert`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InsertSettings {
    /// 插入格式（见 [`TimestampFormat`]）
    pub timestamp_format: TimestampFormat,
}

impl Default for InsertSettings {
    fn default() -> Self {
        Self {
            timestamp_format: defaults::DEFAULT_TIMESTAMP_FORMAT,
        }
    }
}

/// 括号匹配/自动缩进设置（`editor.autoPairs`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoPairsSettings {
    /// 总开关（关闭后其余分项不生效）
    pub enabled: bool,
    /// 自动补对（含选区包裹、右符号跳过、空对退格）
    pub auto_close: bool,
    /// 回车自动继承行首缩进
    pub auto_indent: bool,
    /// 光标旁括号与其配对括号高亮
    pub highlight_match: bool,
}

impl Default for AutoPairsSettings {
    fn default() -> Self {
        Self {
            enabled: defaults::DEFAULT_AUTO_PAIRS_ENABLED,
            auto_close: defaults::DEFAULT_AUTO_PAIRS_AUTO_CLOSE,
            auto_indent: defaults::DEFAULT_AUTO_PAIRS_AUTO_INDENT,
            highlight_match: defaults::DEFAULT_AUTO_PAIRS_HIGHLIGHT_MATCH,
        }
    }
}

/// 清理类操作设置（`editor.cleanup`；控制「一键清理」的参与项）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CleanupSettings {
    /// 一键清理包含：删除行尾空白
    pub trailing_whitespace: bool,
    /// 一键清理包含：合并重复空行
    pub collapse_blank_lines: bool,
    /// 一键清理包含：统一末尾换行（文件不以换行结尾时补一个）
    pub trailing_newline: bool,
}

impl Default for CleanupSettings {
    fn default() -> Self {
        Self {
            trailing_whitespace: defaults::DEFAULT_CLEANUP_TRAILING_WHITESPACE,
            collapse_blank_lines: defaults::DEFAULT_CLEANUP_COLLAPSE_BLANK_LINES,
            trailing_newline: defaults::DEFAULT_CLEANUP_TRAILING_NEWLINE,
        }
    }
}

/// 编辑器节（`settings.json` 的嵌套对象 `editor`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditorSettings {
    /// 行操作默认值
    pub lines: LineOpsSettings,
    /// 多光标设置
    pub multi_cursor: MultiCursorSettings,
    /// 剪贴板历史设置
    pub clipboard: ClipboardSettings,
    /// 时间戳插入设置
    pub insert: InsertSettings,
    /// 括号匹配/自动缩进设置
    pub auto_pairs: AutoPairsSettings,
    /// 清理类操作设置
    pub cleanup: CleanupSettings,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            lines: LineOpsSettings::default(),
            multi_cursor: MultiCursorSettings::default(),
            clipboard: ClipboardSettings::default(),
            insert: InsertSettings::default(),
            auto_pairs: AutoPairsSettings::default(),
            cleanup: CleanupSettings::default(),
        }
    }
}
