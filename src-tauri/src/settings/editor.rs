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

/// 编辑器节（`settings.json` 的嵌套对象 `editor`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditorSettings {
    /// 行操作默认值
    pub lines: LineOpsSettings,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            lines: LineOpsSettings::default(),
        }
    }
}
