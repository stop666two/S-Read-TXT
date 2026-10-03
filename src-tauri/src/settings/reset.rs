//! 设置重置：按作用域（全部 / 分组 / 单项）恢复默认值。
//!
//! 需求对应：§一.3「支持单项重置、分组重置、全部重置（二次确认由前端负责）」。
//!
//! 实现：默认值取自模型的 `Default` 实现（单一事实源），按注册表的点分路径
//! 注入当前 JSON 树，再反序列化回模型统一保存；快捷键的单项重置 = 移除该动作
//! 的覆盖项（生效表回落默认），分组/全部重置 = 清空覆盖表。

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::settings::defaults;
use crate::settings::model::AppSettings;
use crate::settings::reader::ReaderSettings;
use crate::settings::registry;
use crate::settings::shortcuts::ShortcutSettings;
use crate::settings::store;

/// 重置作用域（IPC 入参；`kind` 为判别标签）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ResetScope {
    /// 全部恢复默认
    All,
    /// 按分组恢复默认（分组名见 [`registry::SPECS`]）
    Group {
        /// 分组名（如 `reader.typography`）
        name: String,
    },
    /// 单项恢复默认（设置项 id）
    Field {
        /// 设置项 id（如 `app.maxTabs`）
        id: String,
    },
}

/// 执行重置并落盘（三类配置统一保存；失败返回中文原因）。
pub fn reset_scope(dir: &Path, scope: &ResetScope) -> Result<(), String> {
    let mut app_value = serde_json::to_value(store::load_app_settings(dir))
        .map_err(|err| format!("读取主配置失败：{err}"))?;
    let mut reader_value = serde_json::to_value(store::load_reader_settings(dir))
        .map_err(|err| format!("读取排版配置失败：{err}"))?;
    let mut shortcuts = store::load_shortcuts(dir);
    let ids: Vec<String> = match scope {
        ResetScope::All => registry::SPECS
            .iter()
            .map(|spec| spec.id.to_string())
            .collect(),
        ResetScope::Group { name } => {
            let ids: Vec<String> = registry::specs_in_group(name)
                .map(|spec| spec.id.to_string())
                .collect();
            if ids.is_empty() {
                return Err(format!("未知设置分组：{name}"));
            }
            ids
        }
        ResetScope::Field { id } => vec![id.clone()],
    };
    for id in &ids {
        reset_one(&mut app_value, &mut reader_value, &mut shortcuts, id)?;
    }
    let app: AppSettings =
        serde_json::from_value(app_value).map_err(|err| format!("重置后解析主配置失败：{err}"))?;
    let reader: ReaderSettings = serde_json::from_value(reader_value)
        .map_err(|err| format!("重置后解析排版配置失败：{err}"))?;
    store::save_app_settings(dir, &app).map_err(|err| format!("保存主配置失败：{err}"))?;
    store::save_reader_settings(dir, &reader).map_err(|err| format!("保存排版配置失败：{err}"))?;
    store::save_shortcuts(dir, &shortcuts).map_err(|err| format!("保存快捷键失败：{err}"))?;
    Ok(())
}

/// 重置单项：按 id 前缀分发到 app / reader / shortcuts 三类目标。
fn reset_one(
    app_value: &mut Value,
    reader_value: &mut Value,
    shortcuts: &mut ShortcutSettings,
    id: &str,
) -> Result<(), String> {
    if let Some(action) = id.strip_prefix("shortcuts.bindings.") {
        if !defaults::is_known_action(action) {
            return Err(format!("未知快捷键动作：{action}"));
        }
        shortcuts.bindings.remove(action);
        return Ok(());
    }
    if id == "shortcuts.bindings" {
        shortcuts.bindings.clear();
        return Ok(());
    }
    registry::spec_by_id(id).ok_or_else(|| format!("未知设置项：{id}"))?;
    let (relative, defaults_tree) = if let Some(relative) = id.strip_prefix("app.") {
        (
            relative,
            serde_json::to_value(AppSettings::default()).map_err(|err| format!("{err}"))?,
        )
    } else if let Some(relative) = id.strip_prefix("reader.") {
        (
            relative,
            serde_json::to_value(ReaderSettings::default()).map_err(|err| format!("{err}"))?,
        )
    } else {
        return Err(format!("未知设置项：{id}"));
    };
    let target = if id.starts_with("app.") {
        app_value
    } else {
        reader_value
    };
    let segments: Vec<&str> = relative.split('.').collect();
    match registry::navigate(&defaults_tree, relative).cloned() {
        Some(default_node) => {
            if !registry::set_at(target, &segments, default_node) {
                return Err(format!("设置项路径不存在：{id}"));
            }
        }
        // 默认值即「缺省」（如 `Option` 字段为 None 时序列化省略）：重置 = 移除字段
        None => {
            registry::remove_at(target, &segments);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单项重置：恢复默认值。
    #[test]
    fn reset_single_field_restores_default() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut app = AppSettings::default();
        app.max_tabs = 30;
        store::save_app_settings(dir.path(), &app).expect("保存");
        reset_scope(
            dir.path(),
            &ResetScope::Field {
                id: "app.maxTabs".to_string(),
            },
        )
        .expect("重置");
        assert_eq!(
            store::load_app_settings(dir.path()).max_tabs,
            defaults::DEFAULT_MAX_TABS
        );
    }

    /// 分组重置：排版组内全部字段回默认。
    #[test]
    fn reset_group_restores_typography() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut reader = ReaderSettings::default();
        reader.typography.font_size = 40;
        reader.typography.line_height = 2.5;
        store::save_reader_settings(dir.path(), &reader).expect("保存");
        reset_scope(
            dir.path(),
            &ResetScope::Group {
                name: "reader.typography".to_string(),
            },
        )
        .expect("重置");
        let reader = store::load_reader_settings(dir.path());
        assert_eq!(reader.typography.font_size, defaults::DEFAULT_FONT_SIZE);
        assert_eq!(reader.typography.line_height, defaults::DEFAULT_LINE_HEIGHT);
    }

    /// 全部重置：app/reader/快捷键覆盖全部回默认。
    #[test]
    fn reset_all_restores_everything() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut app = AppSettings::default();
        app.max_tabs = 30;
        store::save_app_settings(dir.path(), &app).expect("保存 app");
        let mut reader = ReaderSettings::default();
        reader.theme_id = "dark".to_string();
        store::save_reader_settings(dir.path(), &reader).expect("保存 reader");
        let mut shortcuts = ShortcutSettings {
            schema_version: defaults::SCHEMA_VERSION,
            bindings: std::collections::BTreeMap::new(),
        };
        shortcuts
            .bindings
            .insert("openFile".to_string(), "Ctrl+Shift+O".to_string());
        store::save_shortcuts(dir.path(), &shortcuts).expect("保存 shortcuts");
        reset_scope(dir.path(), &ResetScope::All).expect("重置");
        let snapshot = store::load_snapshot(dir.path());
        assert_eq!(snapshot.app.max_tabs, defaults::DEFAULT_MAX_TABS);
        assert_eq!(snapshot.reader.theme_id, defaults::DEFAULT_THEME_ID);
        assert_eq!(
            snapshot
                .shortcuts
                .bindings
                .get("openFile")
                .map(String::as_str),
            Some("Ctrl+O")
        );
        assert!(store::load_shortcuts(dir.path()).bindings.is_empty());
    }

    /// 单项重置快捷键：仅移除该动作覆盖项。
    #[test]
    fn reset_single_shortcut_action() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut shortcuts = ShortcutSettings {
            schema_version: defaults::SCHEMA_VERSION,
            bindings: std::collections::BTreeMap::new(),
        };
        shortcuts
            .bindings
            .insert("openFile".to_string(), "Ctrl+Shift+O".to_string());
        shortcuts
            .bindings
            .insert("save".to_string(), "Ctrl+Shift+S".to_string());
        store::save_shortcuts(dir.path(), &shortcuts).expect("保存");
        reset_scope(
            dir.path(),
            &ResetScope::Field {
                id: "shortcuts.bindings.openFile".to_string(),
            },
        )
        .expect("重置");
        let raw = store::load_shortcuts(dir.path());
        assert!(!raw.bindings.contains_key("openFile"));
        assert_eq!(
            raw.bindings.get("save").map(String::as_str),
            Some("Ctrl+Shift+S")
        );
    }

    /// 未知项 / 未知分组拒绝并给出原因。
    #[test]
    fn reset_unknown_targets_are_rejected() {
        let dir = tempfile::tempdir().expect("临时目录");
        let err = reset_scope(
            dir.path(),
            &ResetScope::Field {
                id: "app.notExist".to_string(),
            },
        )
        .expect_err("应拒绝");
        assert!(err.contains("未知设置项"), "{err}");
        let err = reset_scope(
            dir.path(),
            &ResetScope::Group {
                name: "no.such.group".to_string(),
            },
        )
        .expect_err("应拒绝");
        assert!(err.contains("未知设置分组"), "{err}");
        let err = reset_scope(
            dir.path(),
            &ResetScope::Field {
                id: "shortcuts.bindings.notAnAction".to_string(),
            },
        )
        .expect_err("应拒绝");
        assert!(err.contains("未知快捷键动作"), "{err}");
    }
}
