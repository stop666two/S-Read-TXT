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
            paragraph_spacing: defaults::DEFAULT_PARAGRAPH_SPACING,
            first_line_indent: defaults::DEFAULT_FIRST_LINE_INDENT,
            text_align: defaults::DEFAULT_TEXT_ALIGN,
            smooth_scroll: defaults::DEFAULT_SMOOTH_SCROLL,
        }
    }
}

/// 背景图填充模式。
///
/// 语义与 [`Theme`] 相同：未知取值归入 `Unknown`，经 [`BackgroundFill::normalized`]
/// 归一为默认（覆盖）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundFill {
    /// 覆盖（等比缩放填满，可能裁剪）
    Cover,
    /// 包含（等比缩放完整显示，可能留边）
    Contain,
    /// 拉伸（铺满，不保持比例）
    Stretch,
    /// 平铺（原始尺寸重复）
    Tile,
    /// 未知取值（向前兼容；载入时归一为默认）
    Unknown,
}

/// 手写反序列化：未知字符串宽容归入 `Unknown`（避免整份配置回退）。
impl<'de> Deserialize<'de> for BackgroundFill {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "cover" => Self::Cover,
            "contain" => Self::Contain,
            "stretch" => Self::Stretch,
            "tile" => Self::Tile,
            _ => Self::Unknown,
        })
    }
}

impl Default for BackgroundFill {
    fn default() -> Self {
        defaults::DEFAULT_BACKGROUND_FILL
    }
}

impl BackgroundFill {
    /// 归一：`Unknown` 回退默认填充模式，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_BACKGROUND_FILL
        } else {
            self
        }
    }
}

/// 背景图设置（`reader.json` 的嵌套对象 `background`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BackgroundSettings {
    /// 是否启用背景图
    pub enabled: bool,
    /// 存储文件名（`data/backgrounds/` 内；`None` = 未选择，不出现在 JSON 中）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 不透明度（%，范围见 [`defaults::BACKGROUND_OPACITY_RANGE`]）
    pub opacity: u32,
    /// 填充模式
    pub fill: BackgroundFill,
    /// 模糊半径（px，范围见 [`defaults::BACKGROUND_BLUR_RANGE`]）
    pub blur: u32,
    /// 亮度调整（%，范围见 [`defaults::BACKGROUND_DIM_RANGE`]；负=暗化，正=亮化）
    pub dim: i32,
}

impl Default for BackgroundSettings {
    fn default() -> Self {
        Self {
            enabled: defaults::DEFAULT_BACKGROUND_ENABLED,
            file: None,
            opacity: defaults::DEFAULT_BACKGROUND_OPACITY,
            fill: defaults::DEFAULT_BACKGROUND_FILL,
            blur: defaults::DEFAULT_BACKGROUND_BLUR,
            dim: defaults::DEFAULT_BACKGROUND_DIM,
        }
    }
}

/// 四向页边距（px）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Margin4 {
    /// 上边距（px）
    pub top: u32,
    /// 右边距（px）
    pub right: u32,
    /// 下边距（px）
    pub bottom: u32,
    /// 左边距（px）
    pub left: u32,
}

impl Default for Margin4 {
    fn default() -> Self {
        Self {
            top: defaults::DEFAULT_MARGIN,
            right: defaults::DEFAULT_MARGIN,
            bottom: defaults::DEFAULT_MARGIN,
            left: defaults::DEFAULT_MARGIN,
        }
    }
}

/// 页边距设置（阅读 / 编辑两套独立，各四向）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MarginSettings {
    /// 阅读模式边距
    pub reading: Margin4,
    /// 编辑模式边距
    pub editing: Margin4,
}

impl Default for MarginSettings {
    fn default() -> Self {
        Self {
            reading: Margin4::default(),
            editing: Margin4::default(),
        }
    }
}

/// 阅读模式翻页方式（`reader.reading.pageMode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PageMode {
    /// 滚动（默认）
    Scroll,
    /// 分页（按视口高度翻页）
    Paged,
    /// 双页（分页并排两页）
    Double,
    /// 未知取值（向前兼容；载入时归一为默认）
    Unknown,
}

/// 手写反序列化：未知字符串宽容归入 `Unknown`（避免整份配置回退）。
impl<'de> Deserialize<'de> for PageMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "scroll" => Self::Scroll,
            "paged" | "page" => Self::Paged,
            "double" | "doublepage" | "two" => Self::Double,
            _ => Self::Unknown,
        })
    }
}

impl Default for PageMode {
    fn default() -> Self {
        defaults::DEFAULT_PAGE_MODE
    }
}

impl PageMode {
    /// 归一：`Unknown` 回退默认翻页方式。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_PAGE_MODE
        } else {
            self
        }
    }
}

/// 阅读模式配置（`reader.json` 的 `reading` 节点；P2-4）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReadingSettings {
    /// 分栏（1 = 单栏，2 = 双栏）
    pub columns: u32,
    /// 自动滚动速度（px/s）
    pub auto_scroll_speed: u32,
    /// 专注模式（隐藏界面干扰元素）
    pub focus_mode: bool,
    /// 打字机模式（当前行保持在视图中部）
    pub typewriter: bool,
    /// 护眼提醒间隔（分钟；0 = 关闭）
    pub eye_care_interval_min: u32,
    /// 番茄钟时长（分钟）
    pub pomodoro_min: u32,
    /// 阅读时长统计
    pub reading_stats: bool,
    /// 阅读进度记忆（关闭后不再记录与恢复阅读位置）
    pub progress_memory: bool,
    /// 翻页方式
    pub page_mode: PageMode,
    /// 翻页动画时长（ms；0 = 无动画）
    pub page_anim_ms: u32,
}

impl Default for ReadingSettings {
    fn default() -> Self {
        Self {
            columns: defaults::DEFAULT_READING_COLUMNS,
            auto_scroll_speed: defaults::DEFAULT_AUTO_SCROLL_SPEED,
            focus_mode: defaults::DEFAULT_FOCUS_MODE,
            typewriter: defaults::DEFAULT_TYPEWRITER,
            eye_care_interval_min: defaults::DEFAULT_EYE_CARE_INTERVAL_MIN,
            pomodoro_min: defaults::DEFAULT_POMODORO_MIN,
            reading_stats: defaults::DEFAULT_READING_STATS,
            progress_memory: defaults::DEFAULT_PROGRESS_MEMORY,
            page_mode: defaults::DEFAULT_PAGE_MODE,
            page_anim_ms: defaults::DEFAULT_PAGE_ANIM_MS,
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
    /// 页边距（阅读 / 编辑两套独立，各四向）
    pub margins: MarginSettings,
    /// 背景图
    pub background: BackgroundSettings,
    /// 阅读模式（分栏/自动滚动/专注/打字机/提醒/统计/翻页等；P2-4）
    pub reading: ReadingSettings,
}

impl Default for ReaderSettings {
    fn default() -> Self {
        Self {
            schema_version: defaults::SCHEMA_VERSION,
            theme_id: defaults::DEFAULT_THEME_ID.to_string(),
            theme_anim_enabled: defaults::DEFAULT_THEME_ANIM_ENABLED,
            theme_anim_ms: defaults::DEFAULT_THEME_ANIM_MS,
            typography: Typography::default(),
            margins: MarginSettings::default(),
            background: BackgroundSettings::default(),
            reading: ReadingSettings::default(),
        }
    }
}
