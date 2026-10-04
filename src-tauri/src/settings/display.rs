//! 显示选项设置（`settings.json` 嵌套对象 `display`；P2-2）。
//!
//! 字段、默认值与范围以 `docs/configuration.md` §2.1 为准（保持同步）。
//! 不可见字符标记为字符串 id 列表（白名单见
//! [`crate::settings::defaults::DISPLAY_INVISIBLE_IDS`]），归一在
//! [`crate::settings::store`] 完成。

use serde::{Deserialize, Serialize};

use crate::settings::defaults;

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
        }
    }
}
