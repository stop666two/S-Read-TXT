//! `reader.json` 模型（阅读排版子配置）。
//! 字段、默认值与范围以 `docs/configuration.md` §2.2 为准（保持同步）。

use serde::{Deserialize, Serialize};

use crate::settings::defaults;

/// 主题（`system` 由前端监听系统明暗解析为浅色/深色）。
///
/// 语义：加载时未知/大小写异常取值归入 `Unknown`，再经 [`Theme::normalized`]
/// 归一为默认（跟随系统）；保存前必先归一，故 `Unknown` 不会落盘。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    /// 浅色
    Light,
    /// 深色
    Dark,
    /// 护眼（米黄）
    Eye,
    /// 跟随系统（默认）
    System,
    /// 未知取值（向前兼容；载入时归一为默认）
    Unknown,
}

/// 手写反序列化：未知字符串宽容归入 `Unknown`（与 `LogLevel` 同理，避免整份配置回退）。
impl<'de> Deserialize<'de> for Theme {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            "eye" => Self::Eye,
            "system" => Self::System,
            _ => Self::Unknown,
        })
    }
}

impl Default for Theme {
    fn default() -> Self {
        defaults::DEFAULT_THEME
    }
}

impl Theme {
    /// 归一：`Unknown` 回退默认主题，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_THEME
        } else {
            self
        }
    }
}

/// 排版参数（`reader.json` 的嵌套对象 `typography`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Typography {
    /// 正文字体（系统已安装字体名；空值载入时回退默认）
    pub font_family: String,
    /// 正文字号（px，范围见 [`defaults::FONT_SIZE_RANGE`]）
    pub font_size: u32,
    /// 行高倍数（范围见 [`defaults::LINE_HEIGHT_RANGE`]）
    pub line_height: f32,
    /// 正文限宽（px，范围见 [`defaults::CONTENT_WIDTH_RANGE`]）
    pub content_width: u32,
    /// 阅读区页边距（px，范围见 [`defaults::PAGE_PADDING_RANGE`]）
    pub page_padding: u32,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            font_family: defaults::DEFAULT_FONT_FAMILY.to_string(),
            font_size: defaults::DEFAULT_FONT_SIZE,
            line_height: defaults::DEFAULT_LINE_HEIGHT,
            content_width: defaults::DEFAULT_CONTENT_WIDTH,
            page_padding: defaults::DEFAULT_PAGE_PADDING,
        }
    }
}

/// 阅读排版配置（`reader.json`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderSettings {
    /// 配置格式版本（保存时写入当前 [`defaults::SCHEMA_VERSION`]）
    pub schema_version: u32,
    /// 主题
    pub theme: Theme,
    /// 排版参数
    pub typography: Typography,
}

impl Default for ReaderSettings {
    fn default() -> Self {
        Self {
            schema_version: defaults::SCHEMA_VERSION,
            theme: defaults::DEFAULT_THEME,
            typography: Typography::default(),
        }
    }
}
