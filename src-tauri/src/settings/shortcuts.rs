//! `shortcuts.json` 模型（快捷键绑定子配置）。
//!
//! 语义（见 `docs/configuration.md` §2.3）：
//! - `bindings` **仅存与默认不同的项**（覆盖表）；
//! - 生效绑定 = 默认表 + 覆盖表（合并函数在 `settings::store`）；
//! - 恢复默认 = 保存空覆盖表。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::settings::defaults;

/// 快捷键绑定覆盖表（`shortcuts.json`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ShortcutSettings {
    /// 配置格式版本（保存时写入当前 [`defaults::SCHEMA_VERSION`]）
    pub schema_version: u32,
    /// 动作 id → 组合键字符串（仅存被修改过的绑定）
    pub bindings: BTreeMap<String, String>,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            schema_version: defaults::SCHEMA_VERSION,
            bindings: BTreeMap::new(),
        }
    }
}
