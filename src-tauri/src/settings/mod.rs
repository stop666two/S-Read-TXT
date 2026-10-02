//! settings：三类配置文件（`settings.json` / `reader.json` / `shortcuts.json`）的模型与读写。
//!
//! 模块划分：
//! - `model`：主配置模型（含嵌套 `history`）
//! - `reader`：阅读排版模型（主题 + `typography`）
//! - `shortcuts`：快捷键覆盖表模型
//! - `defaults`：默认值与取值范围（与 `docs/configuration.md` 同步）
//! - `store`：载入/保存/归一/自愈备份，以及快捷键生效表合并
//!
//! IPC 聚合快照见 [`SettingsSnapshot`]（不落盘）。

pub mod defaults;
pub mod model;
pub mod reader;
pub mod shortcuts;
pub mod store;

use serde::Serialize;

use crate::settings::model::AppSettings;
use crate::settings::reader::ReaderSettings;
use crate::settings::shortcuts::ShortcutSettings;

/// IPC 聚合快照（`get_settings` 返回体；`shortcuts.bindings` 为**生效绑定**）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    /// 主配置
    pub app: AppSettings,
    /// 阅读排版配置
    pub reader: ReaderSettings,
    /// 快捷键（bindings = 默认 + 覆盖后的生效值）
    pub shortcuts: ShortcutSettings,
}
