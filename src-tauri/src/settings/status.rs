//! 状态栏设置（`settings.json` 嵌套对象 `status`）。
//!
//! 字段、默认值与范围以 `docs/configuration.md` §2.1 为准（保持同步）。
//! 显示项为字符串 id 列表（白名单见 [`crate::settings::defaults::STATUS_ITEM_IDS`]），
//! 顺序即界面展示顺序；归一在 [`crate::settings::store`] 完成。

use serde::{Deserialize, Deserializer, Serialize};

use crate::settings::defaults;

/// 计数模式：状态栏字数的展示口径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CountMode {
    /// 字素簇（默认；用户感知的「字符」，emoji/组合字符按一个计）
    Grapheme,
    /// Unicode 码点
    Codepoint,
    /// UTF-8 字节
    Byte,
    /// 未知取值（向前兼容；载入时归一为默认）
    Unknown,
}

/// 手写反序列化：宽容未知与别名（`graphemes`/`codepoints`/`bytes`）。
impl<'de> Deserialize<'de> for CountMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "grapheme" | "graphemes" => Self::Grapheme,
            "codepoint" | "codepoints" => Self::Codepoint,
            "byte" | "bytes" => Self::Byte,
            _ => Self::Unknown,
        })
    }
}

impl Default for CountMode {
    fn default() -> Self {
        defaults::DEFAULT_STATUS_COUNT_MODE
    }
}

impl CountMode {
    /// 归一：`Unknown` 回退默认。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_STATUS_COUNT_MODE
        } else {
            self
        }
    }
}

/// 状态栏设置（`settings.json` 的嵌套对象 `status`）。
///
/// 注意：容器级 `rename_all = "camelCase"` 使 JSON 键与注册表 id 一致
/// （`app.status.countMode` / `app.status.clickableGoto` 等）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct StatusSettings {
    /// 显示项与顺序（白名单 id；空列表归一为默认组合）
    pub items: Vec<String>,
    /// 计数模式（字素簇/码点/字节）
    pub count_mode: CountMode,
    /// Tab 显示宽度（1–16；缩进参考线等展示口径）
    pub tab_width: u32,
    /// 行列信息可点击跳转
    pub clickable_goto: bool,
    /// 编码可点击切换
    pub clickable_encoding: bool,
    /// 换行符可点击切换
    pub clickable_eol: bool,
    /// 未选择文本时的提示（≤16 字符）
    pub empty_selection_text: String,
}

impl Default for StatusSettings {
    fn default() -> Self {
        Self {
            items: defaults::DEFAULT_STATUS_ITEMS
                .iter()
                .map(|id| (*id).to_string())
                .collect(),
            count_mode: defaults::DEFAULT_STATUS_COUNT_MODE,
            tab_width: defaults::DEFAULT_STATUS_TAB_WIDTH,
            clickable_goto: true,
            clickable_encoding: true,
            clickable_eol: true,
            empty_selection_text: defaults::DEFAULT_STATUS_EMPTY_SELECTION.to_string(),
        }
    }
}
