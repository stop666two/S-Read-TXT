//! 配置版本迁移：把旧版 `settings.json` / `reader.json` / `shortcuts.json`
//! 升级到当前 [`defaults::SCHEMA_VERSION`]。
//!
//! 策略（与需求 §一「版本迁移、失败自动备份、坏配置回退默认」对应）：
//! - 文件缺失 → `Missing`（首次运行，无需迁移）；
//! - 版本等于当前 → `UpToDate`；
//! - 版本高于当前（未来版本）→ `FutureVersion`，**不触碰原文件**
//!   （加载层会因未知结构回退默认，用户换回新版应用即可恢复）；
//! - 版本低于当前 → 依次应用 [`STEPS`] 迁移链，写回前先备份 `<文件>.v<旧版本>.bak`；
//!   写回使用原子写；失败 → `Failed` 且原文件保持不变；
//! - JSON 解析失败 → `Unreadable`（加载层另有 `.corrupt-*` 备份逻辑，互不冲突）。
//!
//! 备份命名说明：同一旧版本重复迁移会覆盖同名备份（内容一致），不累积垃圾文件。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::settings::defaults;
use crate::settings::store;
use crate::storage::atomic;

/// 配置文件中记录 schema 版本的字段名。
pub const FIELD_SCHEMA_VERSION: &str = "schemaVersion";

/// 单个配置文件的迁移结果（供启动日志与测试断言）。
#[derive(Debug, Clone, PartialEq)]
pub enum MigrationStatus {
    /// 文件不存在（首次运行）
    Missing,
    /// 已是当前版本
    UpToDate {
        /// 文件中的版本号
        version: u32,
    },
    /// 迁移成功（`backup` 为迁移前备份路径）
    Migrated {
        /// 迁移前版本
        from: u32,
        /// 迁移前备份路径
        backup: PathBuf,
    },
    /// 文件版本高于当前支持（保持原样）
    FutureVersion {
        /// 文件中的版本号
        version: u32,
    },
    /// 文件存在但 JSON 无法解析（保持原样）
    Unreadable,
    /// 迁移过程中写回/备份失败（保持原文件）
    Failed,
}

/// 迁移步骤签名：把 JSON 值从 `from` 版本改造成 `from + 1` 版本的形状。
type MigrationStep = fn(&mut Value);

/// 迁移链（必须连续覆盖 1..SCHEMA_VERSION 的每一步）。
///
/// v1 → v2：仅新增字段（`hardLimitMB` / `startup` / `statusBar` / 排版扩展等），
/// 由模型的 serde 默认值在反序列化时补齐，本步骤只做版本号提升；
/// v2 → v3：主题升级为 id 体系（`eye` → `paper-cream`）；
/// v3 → v4：新增 `editor.lines` 节（字段补齐式）。
const STEPS: &[(u32, MigrationStep)] = &[
    (1, v1_to_v2),
    (2, v2_to_v3),
    (3, v3_to_v4),
    (4, v4_to_v5),
    (5, v5_to_v6),
    (6, v6_to_v7),
    (7, v7_to_v8),
    (8, v8_to_v9),
    (9, v9_to_v10),
    (10, v10_to_v11),
    (11, v11_to_v12),
    (12, v12_to_v13),
    (13, v13_to_v14),
    (14, v14_to_v15),
    (15, v15_to_v16),
    (16, v16_to_v17),
];

/// v7 → v8：新增 `editor.insert` / `editor.autoPairs` / `editor.cleanup` 字段（serde default 补齐）。
fn v7_to_v8(_value: &mut Value) {}

/// v8 → v9：新增 `find.multifileEnabled` / `find.multifileConcurrency` 字段（serde default 补齐）。
fn v8_to_v9(_value: &mut Value) {}

/// v9 → v10：新增 `status` 节（serde default 补齐）；移除已被 `app.status.items`
/// 取代的 `reader.statusBar` 节（旧包导入时先迁移后校验，保证兼容）。
fn v9_to_v10(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("statusBar");
    }
}

/// v10 → v11：新增 `display` 节（字段补齐由 serde default 处理）。
fn v10_to_v11(_value: &mut Value) {}

/// v11 → v12：页边距改为「阅读/编辑两套 × 四向」——把 `typography.pagePadding(Y)`
/// 映射到 `margins.reading` 与 `margins.editing`（左/右 = pagePadding，上/下 = pagePaddingY），
/// 并移除旧键；缺失时由 serde 默认补齐。
fn v11_to_v12(value: &mut Value) {
    let Some(root) = value.as_object_mut() else {
        return;
    };
    let Some(typography) = root.get_mut("typography").and_then(Value::as_object_mut) else {
        return;
    };
    let pad = typography
        .remove("pagePadding")
        .and_then(|raw| raw.as_u64());
    let pad_y = typography
        .remove("pagePaddingY")
        .and_then(|raw| raw.as_u64());
    if pad.is_none() && pad_y.is_none() {
        return;
    }
    let pad = pad.unwrap_or(defaults::DEFAULT_MARGIN as u64);
    let pad_y = pad_y.unwrap_or(defaults::DEFAULT_MARGIN as u64);
    let margin = serde_json::json!({ "top": pad_y, "right": pad, "bottom": pad_y, "left": pad });
    root.insert(
        "margins".into(),
        serde_json::json!({ "reading": margin.clone(), "editing": margin }),
    );
}

/// v1 → v2：字段补齐式迁移（无结构变换）。
fn v1_to_v2(_value: &mut Value) {}

/// v3 → v4：新增 `editor.lines` 节（字段补齐式迁移，无结构变换）。
fn v3_to_v4(_value: &mut Value) {}

/// v4 → v5：新增 `editor.multiCursor`（字段补齐由 serde 默认值完成，无显式改写）。
fn v4_to_v5(_value: &mut Value) {}

/// v5 → v6：`editor.clipboard` 节新增（字段补齐由 serde 默认值完成）。
fn v5_to_v6(_value: &mut Value) {}

/// v6→v7：新增查找与正则设置节（`find`/`regex`）；字段由 serde 默认值补齐，无需改写。
fn v6_to_v7(_value: &mut Value) {}

/// v2 → v3：主题 id 体系升级——旧内置「护眼（米黄）」`eye` 迁移为 `paper-cream`。
/// 对不含 `theme` 字段的配置（settings.json / shortcuts.json）为空操作。
fn v2_to_v3(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if object.get("theme").and_then(Value::as_str) == Some("eye") {
        object.insert("theme".to_string(), Value::from("paper-cream"));
    }
}

/// v12 → v13：新增 `reading` 节（字段补齐由 serde default 处理）。
fn v12_to_v13(_value: &mut Value) {}

/// v13→v14：新增显示折叠/大纲/面包屑字段（serde 默认补齐，空操作）。
fn v13_to_v14(_value: &mut Value) {}
fn v14_to_v15(_value: &mut Value) {}

/// v15→v16：新增会话恢复内容细项（serde 默认补齐，空操作）。
fn v15_to_v16(_value: &mut Value) {}

/// v16→v17：新增 a11y / system / update 三节（serde 默认补齐，空操作）。
fn v16_to_v17(_value: &mut Value) {}

/// 把 JSON 值从 `from` 版本沿迁移链推进到当前版本，并把 `schemaVersion` 字段改为当前值。
///
/// 返回：`Ok(())` 迁移完成；`Err(中文原因)` 版本超前或迁移链缺步。
pub fn apply_chain(value: &mut Value, from: u32) -> Result<(), String> {
    if from > defaults::SCHEMA_VERSION {
        return Err(format!(
            "配置版本 v{from} 高于当前支持的 v{}",
            defaults::SCHEMA_VERSION
        ));
    }
    let mut current = from.max(1);
    while current < defaults::SCHEMA_VERSION {
        let step = STEPS
            .iter()
            .find(|(version, _)| *version == current)
            .map(|(_, step)| *step);
        match step {
            Some(step) => {
                step(value);
                current += 1;
            }
            None => {
                return Err(format!("缺少 v{current} → v{} 的迁移步骤", current + 1));
            }
        }
    }
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            FIELD_SCHEMA_VERSION.to_string(),
            Value::from(defaults::SCHEMA_VERSION),
        );
    }
    Ok(())
}

/// 迁移单个配置文件（幂等：已是最新版本时不写文件）。
pub fn migrate_file(path: &Path) -> MigrationStatus {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return MigrationStatus::Missing,
    };
    let mut value: Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return MigrationStatus::Unreadable,
    };
    // 无版本字段的历史文件按 v1 处理（本项目 v1 起就带 schemaVersion，此处仅防御）。
    let version = value
        .get(FIELD_SCHEMA_VERSION)
        .and_then(Value::as_u64)
        .unwrap_or(1) as u32;
    if version > defaults::SCHEMA_VERSION {
        return MigrationStatus::FutureVersion { version };
    }
    if version == defaults::SCHEMA_VERSION {
        return MigrationStatus::UpToDate { version };
    }
    if let Err(err) = apply_chain(&mut value, version) {
        log::warn!(target: "sread::settings", "配置迁移中止（{}）：{err}", path.display());
        return MigrationStatus::Failed;
    }
    let backup = backup_path_for(path, version);
    if let Err(err) = std::fs::copy(path, &backup) {
        log::warn!(
            target: "sread::settings",
            "配置迁移备份失败（{} → {}）：{err}",
            path.display(),
            backup.display()
        );
        return MigrationStatus::Failed;
    }
    let text = match serde_json::to_string_pretty(&value) {
        Ok(text) => text,
        Err(err) => {
            log::warn!(target: "sread::settings", "配置迁移序列化失败（{}）：{err}", path.display());
            return MigrationStatus::Failed;
        }
    };
    if let Err(err) = atomic::write_atomic_str(path, &format!("{text}\n")) {
        log::warn!(target: "sread::settings", "配置迁移写回失败（{}）：{err}", path.display());
        return MigrationStatus::Failed;
    }
    MigrationStatus::Migrated {
        from: version,
        backup,
    }
}

/// 迁移全部三类配置文件（启动时调用一次；返回 `(文件名, 结果)` 供日志）。
pub fn migrate_all(dir: &Path) -> Vec<(&'static str, MigrationStatus)> {
    let files: [(&'static str, PathBuf); 3] = [
        (store::FILE_APP_SETTINGS, store::app_settings_path(dir)),
        (
            store::FILE_READER_SETTINGS,
            store::reader_settings_path(dir),
        ),
        (store::FILE_SHORTCUTS, store::shortcuts_path(dir)),
    ];
    files
        .into_iter()
        .map(|(name, path)| (name, migrate_file(&path)))
        .collect()
}

/// 迁移前备份路径：`<文件名>.v<旧版本>.bak`（同目录）。
fn backup_path_for(path: &Path, version: u32) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config".to_string());
    path.with_file_name(format!("{name}.v{version}.bak"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::store;

    /// v11 → v12：旧页边距键映射为阅读/编辑两套四向并移除旧键。
    #[test]
    fn v11_migrates_padding_to_margins() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("reader.json");
        std::fs::write(
            &path,
            br#"{"schemaVersion":11,"theme":"light","typography":{"pagePadding":64,"pagePaddingY":20}}"#,
        )
        .expect("写入失败");
        let _ = migrate_file(&path);
        let raw = std::fs::read_to_string(&path).expect("读取失败");
        let value: Value = serde_json::from_str(&raw).expect("解析失败");
        assert_eq!(value["schemaVersion"], defaults::SCHEMA_VERSION);
        assert_eq!(value["margins"]["reading"]["left"], 64);
        assert_eq!(value["margins"]["reading"]["top"], 20);
        assert_eq!(value["margins"]["editing"]["right"], 64);
        assert!(value["typography"].get("pagePadding").is_none());
    }

    /// v1 → 当前版本：写回版本号、生成 `.v1.bak` 备份、内容保持可解析。
    #[test]
    fn migrates_v1_to_current_with_backup() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = store::app_settings_path(dir.path());
        std::fs::write(&path, r#"{"schemaVersion":1,"maxTabs":30}"#).expect("写 v1");
        let status = migrate_file(&path);
        let MigrationStatus::Migrated { from, backup } = status else {
            panic!("应为 Migrated，实际 {status:?}");
        };
        assert_eq!(from, 1);
        let raw = std::fs::read_to_string(&path).expect("读回");
        assert!(raw.contains(&format!("\"schemaVersion\": {}", defaults::SCHEMA_VERSION)));
        let backup_raw = std::fs::read_to_string(&backup).expect("读备份");
        assert!(backup_raw.contains("\"schemaVersion\":1"));
    }

    /// 当前版本 → UpToDate 且不写文件（修改时间不变）。
    #[test]
    fn current_version_is_up_to_date_and_untouched() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = store::app_settings_path(dir.path());
        let content = format!("{{\"schemaVersion\":{}}}", defaults::SCHEMA_VERSION);
        std::fs::write(&path, &content).expect("写文件");
        let status = migrate_file(&path);
        assert_eq!(
            status,
            MigrationStatus::UpToDate {
                version: defaults::SCHEMA_VERSION
            }
        );
        assert_eq!(std::fs::read_to_string(&path).expect("读回"), content);
    }

    /// 未来版本 → 保持原样（不迁移、不备份）。
    #[test]
    fn future_version_is_kept_untouched() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = store::app_settings_path(dir.path());
        let content = r#"{"schemaVersion":99,"future":true}"#;
        std::fs::write(&path, content).expect("写文件");
        let status = migrate_file(&path);
        assert_eq!(status, MigrationStatus::FutureVersion { version: 99 });
        assert_eq!(std::fs::read_to_string(&path).expect("读回"), content);
    }

    /// 文件缺失 / 损坏：分别返回 Missing / Unreadable，损坏文件保持原样。
    #[test]
    fn missing_and_unreadable_are_reported() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = store::app_settings_path(dir.path());
        assert_eq!(migrate_file(&path), MigrationStatus::Missing);
        std::fs::write(&path, b"{ broken").expect("写损坏");
        assert_eq!(migrate_file(&path), MigrationStatus::Unreadable);
        assert_eq!(std::fs::read_to_string(&path).expect("读回"), "{ broken");
    }

    /// migrate_all：三类文件各自独立返回结果。
    /// v2 → v3：旧主题 id `eye` 迁移为 `paper-cream`（其他字段不动）。
    #[test]
    fn v2_theme_id_migration_maps_eye_to_paper_cream() {
        let mut value: Value =
            serde_json::json!({"schemaVersion": 2, "theme": "eye", "typography": {"fontSize": 20}});
        apply_chain(&mut value, 2).expect("迁移");
        assert_eq!(
            value.get("theme").and_then(Value::as_str),
            Some("paper-cream")
        );
        assert_eq!(
            value.get("schemaVersion").and_then(Value::as_u64),
            Some(u64::from(defaults::SCHEMA_VERSION))
        );
        assert_eq!(
            value
                .pointer("/typography/fontSize")
                .and_then(Value::as_u64),
            Some(20)
        );

        // 非 reader 配置（无 theme 字段）不受影响
        let mut app: Value = serde_json::json!({"schemaVersion": 2, "maxTabs": 18});
        apply_chain(&mut app, 2).expect("迁移");
        assert_eq!(app.get("maxTabs").and_then(Value::as_u64), Some(18));
    }

    #[test]
    fn migrate_all_covers_three_files() {
        let dir = tempfile::tempdir().expect("临时目录");
        std::fs::write(
            store::reader_settings_path(dir.path()),
            r#"{"schemaVersion":1}"#,
        )
        .expect("写 reader");
        let results = migrate_all(dir.path());
        assert_eq!(results.len(), 3);
        assert!(matches!(results[0].1, MigrationStatus::Missing));
        assert!(matches!(
            results[1].1,
            MigrationStatus::Migrated { from: 1, .. }
        ));
        assert!(matches!(results[2].1, MigrationStatus::Missing));
    }
}
