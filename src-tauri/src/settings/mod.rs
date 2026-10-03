//! settings：三类配置文件（`settings.json` / `reader.json` / `shortcuts.json`）的模型与读写。
//!
//! 模块划分：
//! - `model`：主配置模型（含嵌套 `history`）
//! - `reader`：阅读排版模型（主题 + `typography`）
//! - `shortcuts`：快捷键覆盖表模型
//! - `defaults`：默认值与取值范围（与 `docs/configuration.md` 同步）
//! - `store`：载入/保存/归一/自愈备份，以及快捷键生效表合并
//! - `registry`：设置项注册表（界面生成 / 导入校验 / 重置作用域的唯一元数据源）
//! - `migrate`：schema 版本迁移框架（迁移前备份、失败保留原文件）
//! - `bundle`：配置导出/导入（严格校验 + 备份 + 失败回滚）
//! - `reset`：设置重置（全部 / 分组 / 单项）
//! - `theme`：主题清单校验/解析/导入导出与单驻留缓存（P0-5）
//!
//! IPC 载荷见 [`SettingsSnapshot`]（返回）与 [`SettingsSaveRequest`]（保存入参）。

pub mod bundle;
pub mod defaults;
pub mod editor;
pub mod migrate;
pub mod model;
pub mod reader;
pub mod registry;
pub mod reset;
pub mod shortcut_io;
pub mod shortcuts;
pub mod status;
pub mod store;
pub mod theme;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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

/// IPC 保存请求（`save_settings` 入参；前端提交编辑后的完整配置）。
///
/// `shortcuts` 传「生效绑定」全表；后端只落盘与默认不同的覆盖项
/// （恢复默认 = 提交默认表 → 覆盖表清空）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSaveRequest {
    /// 主配置
    pub app: AppSettings,
    /// 阅读排版配置
    pub reader: ReaderSettings,
    /// 生效快捷键绑定表（动作 id → 组合键）
    pub shortcuts: BTreeMap<String, String>,
}
