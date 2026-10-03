//! 配置导出 / 导入（JSON 包）。
//!
//! 需求对应：§一.4「可导出 JSON、可导入且强校验」——导入非法值必须拒绝并给出
//! 具体原因（字段路径 + 中文描述），缺失字段按默认值补齐，未知字段直接拒绝。
//!
//! 流程：
//! - 导出：读取三类配置 → 组装 [`SettingsBundle`] → 原子写入目标文件；
//! - 导入：解析顶层结构 → 版本检查（旧版包先走迁移链）→ 逐段严格校验
//!   （注册表驱动的类型/范围/枚举检查 + 未知字段检测）→ 反序列化为模型 →
//!   备份现有配置（`*.import-bak`）→ 依次原子写入 → 任一步失败即回滚已写文件。
//!
//! 安全边界：导入文件限制 [`MAX_BUNDLE_BYTES`]，超过直接拒绝（配置包不应很大）。

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::settings::defaults;
use crate::settings::migrate;
use crate::settings::model::AppSettings;
use crate::settings::reader::ReaderSettings;
use crate::settings::registry;
use crate::settings::shortcuts::ShortcutSettings;
use crate::settings::store;
use crate::storage::atomic;
use crate::time_util;

/// 导出包格式版本（结构变更时递增；导入严格比对）。
pub const BUNDLE_VERSION: u32 = 1;

/// 导入文件大小上限（字节）；配置包不含正文数据，8MB 足够。
pub const MAX_BUNDLE_BYTES: u64 = 8 * 1024 * 1024;

/// 导出包（JSON 顶层结构）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBundle {
    /// 导出包格式版本（本模块 [`BUNDLE_VERSION`]）
    pub bundle_version: u32,
    /// 配置 schema 版本（写入时为当前 [`defaults::SCHEMA_VERSION`]）
    pub schema_version: u32,
    /// 导出时间（RFC 3339，UTC）
    pub exported_at: String,
    /// 主配置原文（含 `schemaVersion` 字段）
    pub app: Value,
    /// 阅读排版配置原文
    pub reader: Value,
    /// 快捷键覆盖表原文（仅存与默认不同的项）
    pub shortcuts: Value,
}

/// 组装当前数据目录的导出包（不落盘，供命令与测试使用）。
pub fn build_bundle(dir: &Path) -> Result<SettingsBundle, String> {
    let app = serde_json::to_value(store::load_app_settings(dir))
        .map_err(|err| format!("序列化主配置失败：{err}"))?;
    let reader = serde_json::to_value(store::load_reader_settings(dir))
        .map_err(|err| format!("序列化排版配置失败：{err}"))?;
    let shortcuts = serde_json::to_value(store::load_shortcuts(dir))
        .map_err(|err| format!("序列化快捷键配置失败：{err}"))?;
    Ok(SettingsBundle {
        bundle_version: BUNDLE_VERSION,
        schema_version: defaults::SCHEMA_VERSION,
        exported_at: time_util::now_rfc3339(),
        app,
        reader,
        shortcuts,
    })
}

/// 渲染导出包文本（pretty JSON + 结尾换行，便于人工阅读与 diff）。
pub fn render_bundle(bundle: &SettingsBundle) -> Result<String, String> {
    let text =
        serde_json::to_string_pretty(bundle).map_err(|err| format!("生成导出内容失败：{err}"))?;
    Ok(format!("{text}\n"))
}

/// 导出到指定文件路径（原子写），返回写入字节数。
pub fn export_to_file(dir: &Path, target: &Path) -> Result<u64, String> {
    let bundle = build_bundle(dir)?;
    let text = render_bundle(&bundle)?;
    atomic::write_atomic_str(target, &text).map_err(|err| format!("写入导出文件失败：{err}"))?;
    Ok(text.len() as u64)
}

/// 从指定文件导入配置（读取 + 大小限制 + 解析 + [`import_value`]）。
pub fn import_from_file(dir: &Path, source: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(source).map_err(|err| format!("读取导入文件失败：{err}"))?;
    if meta.len() > MAX_BUNDLE_BYTES {
        return Err(format!(
            "导入文件过大（{:.1} MB，上限 {} MB）",
            meta.len() as f64 / (1024.0 * 1024.0),
            MAX_BUNDLE_BYTES / (1024 * 1024)
        ));
    }
    let raw = std::fs::read_to_string(source).map_err(|err| format!("读取导入文件失败：{err}"))?;
    let value: Value =
        serde_json::from_str(&raw).map_err(|err| format!("导入文件不是合法 JSON：{err}"))?;
    import_value(dir, &value)
}

/// 导入解析后的配置包（校验 → 备份 → 写入 → 失败回滚）。
pub fn import_value(dir: &Path, value: &Value) -> Result<(), String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "导入文件的顶层必须是 JSON 对象".to_string())?;
    for key in obj.keys() {
        if !matches!(
            key.as_str(),
            "bundleVersion" | "schemaVersion" | "exportedAt" | "app" | "reader" | "shortcuts"
        ) {
            return Err(format!("未知顶层字段：{key}"));
        }
    }
    let bundle_version = obj
        .get("bundleVersion")
        .and_then(Value::as_u64)
        .ok_or_else(|| "缺少 bundleVersion 字段".to_string())? as u32;
    if bundle_version != BUNDLE_VERSION {
        return Err(format!(
            "不支持的导出格式版本：v{bundle_version}（当前支持 v{BUNDLE_VERSION}）"
        ));
    }
    let schema_version = obj
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .ok_or_else(|| "缺少 schemaVersion 字段".to_string())? as u32;
    if schema_version > defaults::SCHEMA_VERSION {
        return Err(format!(
            "导出文件来自更新的配置版本（v{schema_version}），当前应用支持至 v{}",
            defaults::SCHEMA_VERSION
        ));
    }
    let mut app_raw = obj
        .get("app")
        .cloned()
        .ok_or_else(|| "缺少 app 配置段".to_string())?;
    let mut reader_raw = obj
        .get("reader")
        .cloned()
        .ok_or_else(|| "缺少 reader 配置段".to_string())?;
    let mut shortcuts_raw = obj
        .get("shortcuts")
        .cloned()
        .ok_or_else(|| "缺少 shortcuts 配置段".to_string())?;
    if schema_version < defaults::SCHEMA_VERSION {
        migrate::apply_chain(&mut app_raw, schema_version)?;
        migrate::apply_chain(&mut reader_raw, schema_version)?;
        migrate::apply_chain(&mut shortcuts_raw, schema_version)?;
    }
    validate_section("app", &app_raw)?;
    validate_section("reader", &reader_raw)?;
    validate_section("shortcuts", &shortcuts_raw)?;
    let app: AppSettings =
        serde_json::from_value(app_raw).map_err(|err| format!("app 配置解析失败：{err}"))?;
    let reader: ReaderSettings =
        serde_json::from_value(reader_raw).map_err(|err| format!("reader 配置解析失败：{err}"))?;
    let shortcuts: ShortcutSettings = serde_json::from_value(shortcuts_raw)
        .map_err(|err| format!("shortcuts 配置解析失败：{err}"))?;

    let targets = [
        store::app_settings_path(dir),
        store::reader_settings_path(dir),
        store::shortcuts_path(dir),
    ];
    let mut guards: Vec<WriteGuard> = Vec::with_capacity(targets.len());
    for target in &targets {
        let backup = if target.is_file() {
            let backup = backup_path_for(target);
            std::fs::copy(target, &backup)
                .map_err(|err| format!("备份原配置失败（{}）：{err}", target.display()))?;
            Some(backup)
        } else {
            None
        };
        guards.push(WriteGuard {
            target: target.clone(),
            backup,
            wrote: false,
        });
    }
    write_step(&mut guards, 0, || store::save_app_settings(dir, &app))?;
    write_step(&mut guards, 1, || store::save_reader_settings(dir, &reader))?;
    write_step(&mut guards, 2, || store::save_shortcuts(dir, &shortcuts))?;
    log::info!(
        target: "sread::settings",
        "配置导入完成（原配置已备份为 *.import-bak）"
    );
    Ok(())
}

/// 单个配置文件的写入守卫（预备份 + 回滚信息）。
struct WriteGuard {
    /// 目标文件路径
    target: PathBuf,
    /// 导入前备份路径（目标原本不存在时为 None）
    backup: Option<PathBuf>,
    /// 是否已成功写入新内容
    wrote: bool,
}

/// 导入前的备份文件名：`<文件名>.import-bak`（覆盖式，保留最近一次导入前状态）。
fn backup_path_for(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config".to_string());
    target.with_file_name(format!("{name}.import-bak"))
}

/// 执行一步写入；失败时回滚此前已写文件并返回中文原因。
fn write_step<F>(guards: &mut [WriteGuard], index: usize, step: F) -> Result<(), String>
where
    F: FnOnce() -> std::io::Result<()>,
{
    match step() {
        Ok(()) => {
            guards[index].wrote = true;
            Ok(())
        }
        Err(err) => {
            rollback(&guards[..index]);
            Err(format!(
                "写入配置失败（{}）：{err}（已回滚）",
                guards[index].target.display()
            ))
        }
    }
}

/// 按逆序回滚已写文件：有备份则还原，无备份（原本不存在）则删除。
fn rollback(guards: &[WriteGuard]) {
    for guard in guards.iter().rev() {
        if !guard.wrote {
            continue;
        }
        match &guard.backup {
            Some(backup) => {
                if let Err(err) = std::fs::copy(backup, &guard.target) {
                    log::error!(
                        target: "sread::settings",
                        "回滚失败（{} ← {}）：{err}",
                        guard.target.display(),
                        backup.display()
                    );
                }
            }
            None => {
                let _ = std::fs::remove_file(&guard.target);
            }
        }
    }
}

/// 校验单个配置段：注册表驱动的已知项检查 + 未知字段检测。
fn validate_section(section: &str, value: &Value) -> Result<(), String> {
    value
        .as_object()
        .ok_or_else(|| format!("{section} 配置段必须是对象"))?;
    let prefix = format!("{section}.");
    for spec in registry::SPECS
        .iter()
        .filter(|spec| spec.id.starts_with(&prefix))
    {
        let relative = &spec.id[prefix.len()..];
        if let Some(found) = registry::navigate(value, relative) {
            registry::validate_value(spec, found)?;
        }
    }
    let mut leaves = Vec::new();
    collect_leaves(value, section.to_string(), &mut leaves);
    for path in leaves {
        if path == format!("{section}.schemaVersion") {
            let raw = registry::navigate(value, "schemaVersion").and_then(Value::as_u64);
            if raw.is_none() {
                return Err(format!("{section}.schemaVersion 必须是非负整数"));
            }
            continue;
        }
        if registry::spec_by_id(&path).is_some() {
            continue;
        }
        if let Some(action) = path.strip_prefix("shortcuts.bindings.") {
            if section == "shortcuts" && defaults::is_known_action(action) {
                continue;
            }
        }
        return Err(format!("未知字段：{path}"));
    }
    Ok(())
}

/// 递归收集所有叶子路径（非对象值，含 null；用于未知字段检测）。
fn collect_leaves(value: &Value, path: String, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                collect_leaves(child, format!("{path}.{key}"), out);
            }
        }
        _ => out.push(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// 构造含自定义值的样本配置目录。
    fn seed_custom(dir: &Path) {
        let mut app = AppSettings::default();
        app.max_tabs = 30;
        app.history.max_entries = 500;
        store::save_app_settings(dir, &app).expect("保存 app");
        let mut reader = ReaderSettings::default();
        reader.typography.font_size = 20;
        store::save_reader_settings(dir, &reader).expect("保存 reader");
        let mut shortcuts = ShortcutSettings {
            schema_version: defaults::SCHEMA_VERSION,
            bindings: BTreeMap::new(),
        };
        shortcuts
            .bindings
            .insert("openFile".to_string(), "Ctrl+Shift+O".to_string());
        store::save_shortcuts(dir, &shortcuts).expect("保存 shortcuts");
    }

    /// 导出 → 导入往返：值一致（含快捷键覆盖）。
    #[test]
    fn export_import_roundtrip() {
        let source = tempfile::tempdir().expect("临时目录");
        seed_custom(source.path());
        let export_path = source.path().join("bundle.json");
        let bytes = export_to_file(source.path(), &export_path).expect("导出");
        assert!(bytes > 0);

        let target = tempfile::tempdir().expect("临时目录");
        import_from_file(target.path(), &export_path).expect("导入");
        let snapshot = store::load_snapshot(target.path());
        assert_eq!(snapshot.app.max_tabs, 30);
        assert_eq!(snapshot.app.history.max_entries, 500);
        assert_eq!(snapshot.reader.typography.font_size, 20);
        assert_eq!(
            snapshot
                .shortcuts
                .bindings
                .get("openFile")
                .map(String::as_str),
            Some("Ctrl+Shift+O")
        );
    }

    /// 越界值：拒绝并给出字段路径。
    #[test]
    fn import_rejects_out_of_range_with_field_path() {
        let dir = tempfile::tempdir().expect("临时目录");
        let raw = serde_json::to_value(build_bundle(dir.path()).expect("组装")).expect("值");
        let mut value = raw;
        value["app"]["maxTabs"] = serde_json::json!(9999);
        let err = import_value(dir.path(), &value).expect_err("应拒绝");
        assert!(err.contains("app.maxTabs") && err.contains("范围"), "{err}");
    }

    /// 未知字段：拒绝并指出路径。
    #[test]
    fn import_rejects_unknown_field() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut value = serde_json::to_value(build_bundle(dir.path()).expect("组装")).expect("值");
        value["app"]["bogus"] = serde_json::json!(1);
        let err = import_value(dir.path(), &value).expect_err("应拒绝");
        assert!(err.contains("bogus"), "{err}");
    }

    /// 类型错误：拒绝并指出路径。
    #[test]
    fn import_rejects_wrong_type() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut value = serde_json::to_value(build_bundle(dir.path()).expect("组装")).expect("值");
        value["reader"]["typography"]["fontSize"] = serde_json::json!("big");
        let err = import_value(dir.path(), &value).expect_err("应拒绝");
        assert!(err.contains("reader.typography.fontSize"), "{err}");
    }

    /// 缺段拒绝；未来版本拒绝。
    #[test]
    fn import_rejects_missing_section_and_future_version() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut value = serde_json::to_value(build_bundle(dir.path()).expect("组装")).expect("值");
        let removed = value.as_object_mut().expect("对象").remove("reader");
        assert!(removed.is_some());
        let err = import_value(dir.path(), &value).expect_err("缺段应拒绝");
        assert!(err.contains("reader"), "{err}");

        let mut value = serde_json::to_value(build_bundle(dir.path()).expect("组装")).expect("值");
        value["schemaVersion"] = serde_json::json!(99);
        let err = import_value(dir.path(), &value).expect_err("未来版本应拒绝");
        assert!(err.contains("99"), "{err}");
    }

    /// v1 包导入：迁移到当前版本，缺字段补默认值。
    #[test]
    fn import_v1_bundle_migrates() {
        let dir = tempfile::tempdir().expect("临时目录");
        let value = serde_json::json!({
            "bundleVersion": 1,
            "schemaVersion": 1,
            "exportedAt": "2026-10-03T00:00:00.000Z",
            "app": {"schemaVersion": 1, "maxTabs": 30},
            "reader": {"schemaVersion": 1},
            "shortcuts": {"schemaVersion": 1, "bindings": {}}
        });
        import_value(dir.path(), &value).expect("导入 v1");
        let snapshot = store::load_snapshot(dir.path());
        assert_eq!(snapshot.app.max_tabs, 30);
        assert_eq!(snapshot.app.schema_version, defaults::SCHEMA_VERSION);
        assert_eq!(snapshot.app.hard_limit_mb, defaults::DEFAULT_HARD_LIMIT_MB);
    }

    /// 写入中途失败：已写文件回滚为导入前内容。
    #[test]
    fn import_rolls_back_when_a_write_fails() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut app = AppSettings::default();
        app.max_tabs = 25;
        store::save_app_settings(dir.path(), &app).expect("写初值");
        let before = std::fs::read_to_string(store::app_settings_path(dir.path())).expect("读初值");
        // 先生成合法导入包（reader.json 尚不存在）；再制造写入障碍，
        // 避免 build_bundle → load_json_or_default 把障碍目录备份改名移走。
        let mut value = serde_json::to_value(build_bundle(dir.path()).expect("组装")).expect("值");
        value["app"]["maxTabs"] = serde_json::json!(40);
        // reader.json 为非空目录 → 写入必然失败，且位于第二个写入位（app 已写成功）
        let blocking = store::reader_settings_path(dir.path());
        std::fs::create_dir(&blocking).expect("造目录");
        std::fs::write(blocking.join("keep.txt"), b"x").expect("放入占位文件");
        let err = import_value(dir.path(), &value).expect_err("应失败");
        assert!(err.contains("已回滚"), "{err}");
        let after = std::fs::read_to_string(store::app_settings_path(dir.path())).expect("读回");
        assert_eq!(before, after, "app 配置应回滚为导入前内容");
    }

    /// 超过大小上限的文件：拒绝并给出上限说明（不解析内容）。
    #[test]
    fn import_rejects_oversized_file() {
        let dir = tempfile::tempdir().expect("临时目录");
        let big = dir.path().join("big.json");
        let file = std::fs::File::create(&big).expect("建文件");
        file.set_len(MAX_BUNDLE_BYTES + 1).expect("扩展大小");
        drop(file);
        let err = import_from_file(dir.path(), &big).expect_err("应拒绝");
        assert!(err.contains("过大"), "{err}");
    }

    /// 导出格式版本不符：拒绝并指出不支持的版本。
    #[test]
    fn import_rejects_wrong_bundle_version() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut value = serde_json::to_value(build_bundle(dir.path()).expect("组装")).expect("值");
        value["bundleVersion"] = serde_json::json!(2);
        let err = import_value(dir.path(), &value).expect_err("应拒绝");
        assert!(
            err.contains("不支持的导出格式版本") && err.contains("v2"),
            "{err}"
        );
    }

    /// 顶层未知字段：拒绝并指出字段名。
    #[test]
    fn import_rejects_unknown_top_level_key() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut value = serde_json::to_value(build_bundle(dir.path()).expect("组装")).expect("值");
        value["extra"] = serde_json::json!(1);
        let err = import_value(dir.path(), &value).expect_err("应拒绝");
        assert!(err.contains("extra"), "{err}");
    }

    /// 顶层非对象：拒绝并给出说明。
    #[test]
    fn import_rejects_non_object_top_level() {
        let dir = tempfile::tempdir().expect("临时目录");
        let err = import_value(dir.path(), &serde_json::json!([1, 2])).expect_err("应拒绝");
        assert!(err.contains("JSON 对象"), "{err}");
    }
}
