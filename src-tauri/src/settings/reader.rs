//! `reader.json` 模型（阅读排版子配置）。
//! 字段、默认值与范围以 `docs/configuration.md` §2.2 为准（保持同步）。

use serde::{Deserialize, Serialize};

use crate::settings::defaults;

/// 正文水平对齐（`left` 左对齐 / `justify` 两端对齐）。
///
/// 语义：加载时未知取值归入 `Unknown`，经 [`TextAlign::normalized`] 归一为默认。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    /// 左对齐（默认）
    Left,
    /// 两端对齐
    Justify,
    /// 未知取值（向前兼容；载入时归一为默认）
    Unknown,
}

/// 手写反序列化：未知字符串宽容归入 `Unknown`（避免整份配置回退）。
impl<'de> Deserialize<'de> for TextAlign {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "left" => Self::Left,
            "justify" => Self::Justify,
            _ => Self::Unknown,
        })
    }
}

impl Default for TextAlign {
    fn default() -> Self {
        defaults::DEFAULT_TEXT_ALIGN
    }
}

impl TextAlign {
    /// 归一：`Unknown` 回退默认对齐，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_TEXT_ALIGN
        } else {
            self
        }
    }
}

/// 排版参数（`reader.json` 的嵌套对象 `typography`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Typography {
    /// 正文字体（系统已安装字体名；自定义字体用 `custom:<文件名>`；空值载入时回退默认）
    pub font_family: String,
    /// 正文字号（px，范围见 [`defaults::FONT_SIZE_RANGE`]）
    pub font_size: u32,
    /// 行高倍数（范围见 [`defaults::LINE_HEIGHT_RANGE`]）
    pub line_height: f32,
    /// 正文限宽（px，范围见 [`defaults::CONTENT_WIDTH_RANGE`]）
    pub content_width: u32,
    /// 阅读区左右页边距（px，范围见 [`defaults::PAGE_PADDING_RANGE`]）
    pub page_padding: u32,
    /// 阅读区上下留白（px，范围见 [`defaults::PAGE_PADDING_Y_RANGE`]）
    pub page_padding_y: u32,
    /// 段间距（px，范围见 [`defaults::PARAGRAPH_SPACING_RANGE`]）
    pub paragraph_spacing: u32,
    /// 首行缩进（字符数，范围见 [`defaults::FIRST_LINE_INDENT_RANGE`]）
    pub first_line_indent: u32,
    /// 正文水平对齐
    pub text_align: TextAlign,
    /// 翻页平滑滚动（PgUp/PgDn）
    pub smooth_scroll: bool,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            font_family: defaults::DEFAULT_FONT_FAMILY.to_string(),
            font_size: defaults::DEFAULT_FONT_SIZE,
            line_height: defaults::DEFAULT_LINE_HEIGHT,
            content_width: defaults::DEFAULT_CONTENT_WIDTH,
            page_padding: defaults::DEFAULT_PAGE_PADDING,
            page_padding_y: defaults::DEFAULT_PAGE_PADDING_Y,
            paragraph_spacing: defaults::DEFAULT_PARAGRAPH_SPACING,
            first_line_indent: defaults::DEFAULT_FIRST_LINE_INDENT,
            text_align: defaults::DEFAULT_TEXT_ALIGN,
            smooth_scroll: defaults::DEFAULT_SMOOTH_SCROLL,
        }
    }
}

/// 状态栏元素显隐（`reader.json` 的嵌套对象 `statusBar`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatusBarSettings {
    /// 显示文件名与进度
    pub show_file_name: bool,
    /// 显示阅读百分比
    pub show_percent: bool,
    /// 显示文件大小
    pub show_size: bool,
    /// 显示编码切换按钮
    pub show_encoding: bool,
}

impl Default for StatusBarSettings {
    fn default() -> Self {
        Self {
            show_file_name: defaults::DEFAULT_STATUS_BAR_SHOW_FILE,
            show_percent: defaults::DEFAULT_STATUS_BAR_SHOW_PERCENT,
            show_size: defaults::DEFAULT_STATUS_BAR_SHOW_SIZE,
            show_encoding: defaults::DEFAULT_STATUS_BAR_SHOW_ENCODING,
        }
    }
}

/// 阅读排版配置（`reader.json`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderSettings {
    /// 配置格式版本（保存时写入当前 [`defaults::SCHEMA_VERSION`]）
    pub schema_version: u32,
    /// 主题 id（内置：light/dark/eye-green/paper-cream/high-contrast/minimal-gray；
    /// `system` = 跟随系统明暗；用户主题为 `data/themes/<id>.json`）。
    /// JSON 名沿用 `theme`（字段名变更由迁移负责，见 settings::migrate）。
    #[serde(rename = "theme")]
    pub theme_id: String,
    /// 主题切换过渡动画
    pub theme_anim_enabled: bool,
    /// 主题切换过渡时长（ms；0 = 无过渡）
    pub theme_anim_ms: u32,
    /// 排版参数
    pub typography: Typography,
    /// 状态栏元素显隐
    pub status_bar: StatusBarSettings,
}

impl Default for ReaderSettings {
    fn default() -> Self {
        Self {
            schema_version: defaults::SCHEMA_VERSION,
            theme_id: defaults::DEFAULT_THEME_ID.to_string(),
            theme_anim_enabled: defaults::DEFAULT_THEME_ANIM_ENABLED,
            theme_anim_ms: defaults::DEFAULT_THEME_ANIM_MS,
            typography: Typography::default(),
            status_bar: StatusBarSettings::default(),
        }
    }
}
