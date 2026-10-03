//! 快捷键独立导入/导出（P0-9「快捷键扩展框架」）。
//!
//! 格式（用户可读、可手工编辑）：
//! ```json
//! {
//!   "bundleVersion": 1,
//!   "schemaVersion": 2,
//!   "exportedAt": "2026-10-03T12:00:00.000Z",
//!   "bindings": { "openFile": "Ctrl+O", "closeTab": "Ctrl+W" }
//! }
//! ```
//!
//! 语义：
//! - 导出：写「生效绑定」全表（默认 + 覆盖），便于用户查看与修改；
//! - 导入：以导入表**整体替换**覆盖项（导入后未列出的动作回到默认）；
//! - 校验：未知动作 / 空组合键 / 过长 / 非对象 / 超大小一律拒绝并给出具体原因；
//! - 落盘：导入前备份既有 `shortcuts.json`（`*.import-bak`），写入失败自动回滚。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::settings::defaults;
use crate::settings::shortcuts::ShortcutSettings;
use crate::settings::store;
use crate::storage::atomic;

/// 快捷键导出文件格式版本（结构变更时递增并保持旧版可导入）。
pub const BUNDLE_VERSION: u32 = 1;
/// 导入文件大小上限（256KB；快捷键表远小于此，超限视为异常文件）。
pub const MAX_FILE_BYTES: u64 = 256 * 1024;
/// 单个组合键字符串长度上限（字符数；与前端录制器约定一致）。
pub const MAX_COMBO_CHARS: usize = 64;

/// 渲染导出 JSON（生效绑定全表）。
///
/// 参数：`dir` 数据目录。返回：pretty JSON 文本（含结尾换行由落盘方补齐）。
pub fn render_bundle(dir: &Path) -> Result<String, String> {
    let effective = store::effective_bindings(&store::load_shortcuts(dir).bindings);
    let value = json!({
        "bundleVersion": BUNDLE_VERSION,
        "schemaVersion": defaults::SCHEMA_VERSION,
        "exportedAt": crate::time_util::now_rfc3339(),
        "bindings": effective,
    });
    serde_json::to_string_pretty(&value).map_err(|err| format!("序列化快捷键失败：{err}"))
}

/// 导出到文件（原子写）。
///
/// 参数：`dir` 数据目录；`path` 目标文件路径（由用户选择）。
/// 返回：写入字节数（含结尾换行）。
pub fn export_to_file(dir: &Path, path: &str) -> Result<u64, String> {
    let text = render_bundle(dir)?;
    let payload = format!("{text}\n");
    atomic::write_atomic_str(Path::new(path), &payload)
        .map_err(|err| format!("写入快捷键文件失败：{err}"))?;
    Ok(payload.len() as u64)
}

/// 从文件导入（强校验 + 备份 + 失败回滚）。
///
/// 参数：`dir` 数据目录；`path` 导入文件路径。
/// 返回：`Ok(())` 导入成功；`Err(原因)` 未做任何修改。
pub fn import_from_file(dir: &Path, path: &str) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|err| format!("读取快捷键文件失败：{err}"))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "快捷键文件过大（{} 字节，上限 {MAX_FILE_BYTES} 字节）",
            meta.len()
        ));
    }
    let text =
        std::fs::read_to_string(path).map_err(|err| format!("读取快捷键文件失败：{err}"))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|err| format!("快捷键文件不是合法 JSON：{err}"))?;
    let bindings = parse_bindings(&value)?;

    let target = store::shortcuts_path(dir);
    let backup = backup_existing(&target)?;
    let result = store::save_shortcuts(
        dir,
        &ShortcutSettings {
            schema_version: defaults::SCHEMA_VERSION,
            bindings: store::to_overrides(&bindings),
        },
    );
    if let Err(err) = result {
        restore_backup(&target, backup.as_deref());
        return Err(format!("保存快捷键失败：{err}"));
    }
    Ok(())
}

/// 解析并校验导入文件的 `bindings` 表。
///
/// 返回：动作 id → 组合键（仅已知动作；未知/空/过长逐项报错）。
fn parse_bindings(value: &Value) -> Result<BTreeMap<String, String>, String> {
    let object = value.as_object().ok_or("快捷键文件必须是 JSON 对象")?;
    let bindings = object
        .get("bindings")
        .ok_or("快捷键文件缺少 bindings 字段")?
        .as_object()
        .ok_or("bindings 字段必须是对象（动作 id → 组合键）")?;
    let mut out = BTreeMap::new();
    for (action, combo) in bindings {
        if !defaults::is_known_action(action) {
            return Err(format!("未知快捷键动作：{action}"));
        }
        let combo = combo
            .as_str()
            .ok_or_else(|| format!("{action}：组合键必须是字符串"))?
            .trim();
        if combo.is_empty() {
            return Err(format!("{action}：组合键不能为空"));
        }
        if combo.chars().count() > MAX_COMBO_CHARS {
            return Err(format!(
                "{action}：组合键过长（上限 {MAX_COMBO_CHARS} 字符）"
            ));
        }
        out.insert(action.clone(), combo.to_string());
    }
    if out.is_empty() {
        return Err("快捷键文件未包含任何绑定".to_string());
    }
    Ok(out)
}

/// 备份既有 shortcuts.json（不存在返回 `None`）。
fn backup_existing(target: &Path) -> Result<Option<PathBuf>, String> {
    if !target.is_file() {
        return Ok(None);
    }
    let backup = target.with_file_name(format!(
        "{}.import-bak",
        target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "shortcuts.json".to_string())
    ));
    std::fs::copy(target, &backup).map_err(|err| format!("备份快捷键文件失败：{err}"))?;
    Ok(Some(backup))
}

/// 回滚：有备份则恢复，无备份则删除新写入的文件（尽力而为，失败记日志）。
fn restore_backup(target: &Path, backup: Option<&Path>) {
    match backup {
        Some(path) => {
            if let Err(err) = std::fs::copy(path, target) {
                log::error!("回滚快捷键文件失败：{err}");
            }
        }
        None => {
            let _ = std::fs::remove_file(target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("创建临时目录失败")
    }

    /// 导出：包含格式字段与全部 15 个生效绑定。
    #[test]
    fn export_renders_effective_bindings() {
        let dir = data_dir();
        let text = render_bundle(dir.path()).expect("导出失败");
        let value: Value = serde_json::from_str(&text).expect("导出应为合法 JSON");
        assert_eq!(value["bundleVersion"], 1);
        assert_eq!(value["schemaVersion"], defaults::SCHEMA_VERSION);
        let bindings = value["bindings"].as_object().expect("bindings 对象");
        assert_eq!(bindings.len(), 15);
        assert_eq!(bindings["openFile"], "Ctrl+O");
    }

    /// 导入：整体替换覆盖项（未列出的动作回到默认）。
    #[test]
    fn import_replaces_overrides_and_applies() {
        let dir = data_dir();
        // 先制造一个既有覆盖：closeTab → Alt+W
        let mut overrides = BTreeMap::new();
        overrides.insert("closeTab".to_string(), "Alt+W".to_string());
        store::save_shortcuts(
            dir.path(),
            &ShortcutSettings {
                schema_version: defaults::SCHEMA_VERSION,
                bindings: overrides,
            },
        )
        .expect("预置覆盖失败");

        let import_path = dir.path().join("import.json");
        std::fs::write(
            &import_path,
            r#"{ "bindings": { "openFile": "Ctrl+Shift+O" } }"#,
        )
        .expect("写导入文件失败");
        import_from_file(dir.path(), &import_path.to_string_lossy()).expect("导入失败");

        let saved = store::load_shortcuts(dir.path());
        assert_eq!(saved.bindings.len(), 1, "覆盖项应被整体替换：{:?}", saved.bindings);
        assert_eq!(saved.bindings["openFile"], "Ctrl+Shift+O");
        let effective = store::effective_bindings(&saved.bindings);
        assert_eq!(effective["closeTab"], "Ctrl+W", "未列出动作应回默认");
    }

    /// 导入：未知动作拒绝且不改动原文件。
    #[test]
    fn import_rejects_unknown_action() {
        let dir = data_dir();
        let before = store::load_shortcuts(dir.path());
        let import_path = dir.path().join("bad.json");
        std::fs::write(&import_path, r#"{ "bindings": { "bogus": "Ctrl+B" } }"#).expect("写文件失败");
        let err = import_from_file(dir.path(), &import_path.to_string_lossy()).expect_err("应拒绝");
        assert!(err.contains("bogus"), "错误应包含动作名：{err}");
        assert_eq!(store::load_shortcuts(dir.path()).bindings, before.bindings);
    }

    /// 导入：空组合键 / 缺少 bindings / 非法 JSON / 超大文件逐项拒绝。
    #[test]
    fn import_rejects_invalid_files() {
        let dir = data_dir();
        let cases: [(&str, &str); 3] = [
            (r#"{ "bindings": { "openFile": "   " } }"#, "不能为空"),
            (r#"{ "bundleVersion": 1 }"#, "缺少 bindings"),
            ("not-json", "合法 JSON"),
        ];
        for (body, expected) in cases {
            let path = dir.path().join("case.json");
            std::fs::write(&path, body).expect("写文件失败");
            let err = import_from_file(dir.path(), &path.to_string_lossy()).expect_err("应拒绝");
            assert!(err.contains(expected), "预期错误含「{expected}」，实际：{err}");
        }

        let big = dir.path().join("big.json");
        let file = std::fs::File::create(&big).expect("创建失败");
        file.set_len(MAX_FILE_BYTES + 1).expect("膨胀失败");
        let err = import_from_file(dir.path(), &big.to_string_lossy()).expect_err("应拒绝");
        assert!(err.contains("过大"), "预期错误含「过大」：{err}");
    }
}
