//! 显示选项设置（`settings.json` 嵌套对象 `display`）。
//!
//! 字段、默认值与范围以 `docs/configuration.md` §2.1 为准（保持同步）。
//! 不可见字符标记为字符串 id 列表（白名单见
//! [`crate::settings::defaults::DISPLAY_INVISIBLE_IDS`]），归一在
//! [`crate::settings::store`] 完成。

use serde::{Deserialize, Serialize};

use crate::settings::defaults;

/// 折叠方式（编辑与阅读双模式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FoldingMode {
    /// 关闭（默认）
    #[serde(rename = "off")]
    Off,
    /// 按缩进
    #[serde(rename = "indent")]
    Indent,
    /// 按标题（内置章节正则）
    #[serde(rename = "heading")]
    Heading,
    /// 按正则（使用可编辑大纲正则）
    #[serde(rename = "regex")]
    Regex,
    /// 未知取值（向前兼容；载入时归一为默认）
    #[serde(rename = "unknown")]
    Unknown,
}

/// 手写反序列化：未知字符串宽容归入 `Unknown`（避免整份配置回退）。
impl<'de> Deserialize<'de> for FoldingMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "" | "off" | "none" => Self::Off,
            "indent" => Self::Indent,
            "heading" | "title" => Self::Heading,
            "regex" => Self::Regex,
            _ => Self::Unknown,
        })
    }
}

impl Default for FoldingMode {
    fn default() -> Self {
        defaults::DEFAULT_DISPLAY_FOLDING
    }
}

impl FoldingMode {
    /// 归一：`Unknown` 回退默认，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_DISPLAY_FOLDING
        } else {
            self
        }
    }
}

/// 显示选项（`settings.json` 的嵌套对象 `display`）。
///
/// 注意：容器级 `rename_all = "camelCase"` 使 JSON 键与注册表 id 一致
/// （`app.display.lineNumbers` / `app.display.rulerPosition` 等）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DisplaySettings {
    /// 显示行号（显示行序号，1 基；阅读与编辑模式通用）
    pub line_numbers: bool,
    /// 相对行号（相对编辑光标 / 阅读顶部行）
    pub relative_line_numbers: bool,
    /// 高亮当前行（编辑态 = 光标行；阅读态 = 顶部行）
    pub highlight_current_line: bool,
    /// 自动换行（关 = 长行水平滚动）
    pub word_wrap: bool,
    /// 显示标尺（正文列竖向参考线）
    pub ruler: bool,
    /// 标尺位置（px，相对正文列左缘；0–1000）
    pub ruler_position: u32,
    /// 缩进参考线（按行首空白宽度近似对齐）
    pub indent_guides: bool,
    /// 不可见字符标记（白名单：space/tab/newline/trailingSpace）
    pub invisible: Vec<String>,
    /// 滚动条标记（搜索命中 / 书签 / 修改位置）
    pub scrollbar_markers: bool,
    /// 折叠方式（关闭/按缩进/按标题/按正则）
    pub folding: FoldingMode,
    /// 大纲面板
    pub outline: bool,
    /// 面包屑
    pub breadcrumb: bool,
    /// 大纲正则（逐条正则；归一后空列表回退内置默认）
    pub outline_patterns: Vec<String>,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            line_numbers: defaults::DEFAULT_DISPLAY_LINE_NUMBERS,
            relative_line_numbers: defaults::DEFAULT_DISPLAY_RELATIVE_LINE_NUMBERS,
            highlight_current_line: defaults::DEFAULT_DISPLAY_HIGHLIGHT_CURRENT_LINE,
            word_wrap: defaults::DEFAULT_DISPLAY_WORD_WRAP,
            ruler: defaults::DEFAULT_DISPLAY_RULER,
            ruler_position: defaults::DEFAULT_DISPLAY_RULER_POSITION,
            indent_guides: defaults::DEFAULT_DISPLAY_INDENT_GUIDES,
            invisible: Vec::new(),
            scrollbar_markers: defaults::DEFAULT_DISPLAY_SCROLLBAR_MARKERS,
            folding: defaults::DEFAULT_DISPLAY_FOLDING,
            outline: defaults::DEFAULT_DISPLAY_OUTLINE,
            breadcrumb: defaults::DEFAULT_DISPLAY_BREADCRUMB,
            outline_patterns: defaults::DEFAULT_OUTLINE_PATTERNS
                .iter()
                .map(|pattern| (*pattern).to_string())
                .collect(),
        }
    }
}
