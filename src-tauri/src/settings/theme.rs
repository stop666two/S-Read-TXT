//! 主题系统 v2（P0-5）：清单校验、加载解析、导入导出与单主题驻留缓存。
//!
//! 主题来源（BCP 47 无关；id 为 ASCII 稳定标识）：
//! - 内置 6 套：`resources/themes/*.json`，编译期 `include_str!` 嵌入（无磁盘依赖）；
//! - 用户主题：数据目录 `data/themes/<id>.json`（导入生成；不得与内置 id 同名）。
//!
//! 单主题驻留：解析结果只保留最近一次（[`cached_theme`]），切换即替换，不累积。
//! `system` 为伪主题 id：解析为当前系统明暗对应的 `light` / `dark`。
//!
//! 校验规则（强校验，导入/加载一致）：
//! - `id`：小写字母/数字/连字符，1–32 字符，不得为 `system`，文件内 id 必须与文件名一致；
//! - `name` / `nameEn`：非空且 ≤32 字符；
//! - `base`：`light` / `dark`；
//! - `tokens`：13 个必需键齐全、无未知键、颜色支持 `#RGB` / `#RRGGBB` / `#RRGGBBAA` /
//!   `rgb()` / `rgba()` / `hsl()` / `hsla()`（与查找高亮色等共用同一校验）。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 主题清单文件格式版本（独立于设置 schema）
pub const THEME_SCHEMA_VERSION: u32 = 1;
/// 伪主题 id：跟随系统明暗
pub const SYSTEM_THEME_ID: &str = "system";
/// 用户主题目录名（位于数据目录内）
pub const USER_THEMES_DIR: &str = "themes";

/// 必需令牌键（与 `src/styles/base.css` / UnoCSS 颜色令牌一一对应）
pub const REQUIRED_TOKENS: &[&str] = &[
    "base",
    "surface",
    "chrome",
    "ink",
    "muted",
    "line",
    "hover",
    "accent",
    "tabActive",
    "selection",
    "danger",
    "success",
    "warning",
];

/// 内置主题表：id → 嵌入 JSON（顺序即界面默认展示顺序）
const BUILTIN_THEMES: &[(&str, &str)] = &[
    ("light", include_str!("../../resources/themes/light.json")),
    ("dark", include_str!("../../resources/themes/dark.json")),
    (
        "eye-green",
        include_str!("../../resources/themes/eye-green.json"),
    ),
    (
        "paper-cream",
        include_str!("../../resources/themes/paper-cream.json"),
    ),
    (
        "high-contrast",
        include_str!("../../resources/themes/high-contrast.json"),
    ),
    (
        "minimal-gray",
        include_str!("../../resources/themes/minimal-gray.json"),
    ),
];

/// 主题错误（IPC 层映射为稳定错误码）
#[derive(Debug, Error)]
pub enum ThemeError {
    /// 主题（内置或用户）不存在
    #[error("主题不存在：{0}")]
    NotFound(String),
    /// 清单校验失败（附具体原因）
    #[error("主题文件无效：{0}")]
    Invalid(String),
    /// 文件读写失败
    #[error("主题文件读写失败：{0}")]
    Io(String),
}

/// 主题清单（JSON 文件结构；`builtin` 由加载器按来源重写，非文件权威字段）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeManifest {
    /// 文件格式版本
    pub schema_version: u32,
    /// 稳定 id
    pub id: String,
    /// 中文名
    pub name: String,
    /// 英文名
    pub name_en: String,
    /// 明暗基底（`light` / `dark`；决定 color-scheme）
    pub base: String,
    /// 是否内置（加载时重写）
    pub builtin: bool,
    /// 颜色令牌表
    pub tokens: BTreeMap<String, String>,
}

impl Default for ThemeManifest {
    fn default() -> Self {
        Self {
            schema_version: THEME_SCHEMA_VERSION,
            id: String::new(),
            name: String::new(),
            name_en: String::new(),
            base: String::new(),
            builtin: false,
            tokens: BTreeMap::new(),
        }
    }
}

/// 清单摘要（列表用；不含令牌，内存极低）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSummary {
    /// 稳定 id
    pub id: String,
    /// 中文名
    pub name: String,
    /// 英文名
    pub name_en: String,
    /// 明暗基底
    pub base: String,
    /// 是否内置
    pub builtin: bool,
}

/// 解析结果（单套驻留；前端据 tokens 写 CSS 变量）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedTheme {
    /// 实际生效的主题 id（`system` 已解析为 light/dark）
    pub id: String,
    /// 明暗基底
    pub base: String,
    /// 颜色令牌
    pub tokens: BTreeMap<String, String>,
}

/// 单主题驻留缓存（只保留最近一次解析结果）
static ACTIVE: Mutex<Option<ResolvedTheme>> = Mutex::new(None);

/// 读取单主题驻留缓存（测试与诊断用）
pub fn cached_theme() -> Option<ResolvedTheme> {
    ACTIVE.lock().ok().and_then(|guard| guard.clone())
}

/// 写入单主题驻留缓存（覆盖式）
fn cache_store(resolved: &ResolvedTheme) {
    if let Ok(mut guard) = ACTIVE.lock() {
        *guard = Some(resolved.clone());
    }
}

/// id 是否合法（小写字母/数字/连字符，1–32 字符，首字符为字母/数字，保留 `system`）
pub fn is_valid_theme_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 32 || id == SYSTEM_THEME_ID {
        return false;
    }
    let mut chars = id.chars();
    let first = chars.next().expect("非空");
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// 颜色是否合法（与查找高亮色等共用同一校验：hex / rgb() / rgba() / hsl() / hsla()）。
pub fn is_valid_color(value: &str) -> bool {
    crate::settings::store::is_valid_color(value)
}

/// 校验清单（未知键/缺键/非法值一律拒绝并给出具体原因）
pub fn validate_manifest(manifest: &ThemeManifest) -> Result<(), String> {
    if manifest.schema_version != THEME_SCHEMA_VERSION {
        return Err(format!(
            "schemaVersion 应为 {THEME_SCHEMA_VERSION}（实际 {}）",
            manifest.schema_version
        ));
    }
    if !is_valid_theme_id(&manifest.id) {
        return Err(format!(
            "id 非法：{:?}（要求小写字母/数字/连字符、1–32 字符、不得为 system）",
            manifest.id
        ));
    }
    for (label, text) in [("name", &manifest.name), ("nameEn", &manifest.name_en)] {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.chars().count() > 32 {
            return Err(format!("{label} 须为非空且不超过 32 字符"));
        }
    }
    if manifest.base != "light" && manifest.base != "dark" {
        return Err(format!("base 应为 light/dark（实际 {:?}）", manifest.base));
    }
    for key in REQUIRED_TOKENS {
        let Some(value) = manifest.tokens.get(*key) else {
            return Err(format!("缺少令牌：{key}"));
        };
        if !is_valid_color(value) {
            return Err(format!("令牌 {key} 颜色非法：{value:?}"));
        }
    }
    for key in manifest.tokens.keys() {
        if !REQUIRED_TOKENS.contains(&key.as_str()) {
            return Err(format!("未知令牌：{key}"));
        }
    }
    Ok(())
}

/// 取内置主题清单（解析失败返回 `None`；单元测试保证全部成功）
fn builtin_manifest(id: &str) -> Option<ThemeManifest> {
    let (_, raw) = BUILTIN_THEMES.iter().find(|(bid, _)| *bid == id)?;
    let mut manifest: ThemeManifest = serde_json::from_str(raw).ok()?;
    manifest.builtin = true;
    Some(manifest)
}

/// 用户主题文件路径
pub fn user_theme_path(data_dir: &Path, id: &str) -> PathBuf {
    data_dir.join(USER_THEMES_DIR).join(format!("{id}.json"))
}

/// 加载并校验主题清单（内置优先；用户主题校验 id 与文件名一致）
pub fn load_manifest(data_dir: &Path, id: &str) -> Result<ThemeManifest, ThemeError> {
    if let Some(manifest) = builtin_manifest(id) {
        validate_manifest(&manifest).map_err(ThemeError::Invalid)?;
        return Ok(manifest);
    }
    let path = user_theme_path(data_dir, id);
    let raw = fs::read_to_string(&path).map_err(|_| ThemeError::NotFound(id.to_string()))?;
    let mut manifest: ThemeManifest = serde_json::from_str(&raw)
        .map_err(|e| ThemeError::Invalid(format!("{id}：JSON 解析失败：{e}")))?;
    if manifest.id != id {
        return Err(ThemeError::Invalid(format!(
            "{id}：文件内 id（{}）与文件名不一致",
            manifest.id
        )));
    }
    manifest.builtin = false;
    validate_manifest(&manifest).map_err(|e| ThemeError::Invalid(format!("{id}：{e}")))?;
    Ok(manifest)
}

/// 清单 → 摘要
pub fn summary_of(manifest: &ThemeManifest) -> ThemeSummary {
    ThemeSummary {
        id: manifest.id.clone(),
        name: manifest.name.clone(),
        name_en: manifest.name_en.clone(),
        base: manifest.base.clone(),
        builtin: manifest.builtin,
    }
}

/// 列出全部主题（内置在前；用户主题按 id 排序；非法用户文件跳过并记录日志）
pub fn list_themes(data_dir: &Path) -> Vec<ThemeSummary> {
    let mut summaries: Vec<ThemeSummary> = BUILTIN_THEMES
        .iter()
        .filter_map(|(id, _)| builtin_manifest(id))
        .filter(|manifest| validate_manifest(manifest).is_ok())
        .map(|manifest| summary_of(&manifest))
        .collect();
    let dir = data_dir.join(USER_THEMES_DIR);
    if let Ok(entries) = fs::read_dir(&dir) {
        let mut user: Vec<ThemeSummary> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            match load_manifest(data_dir, stem) {
                Ok(manifest) => user.push(summary_of(&manifest)),
                Err(err) => log::warn!(target: "sread::theme", "跳过无效用户主题：{err}"),
            }
        }
        user.sort_by(|a, b| a.id.cmp(&b.id));
        summaries.extend(user);
    }
    summaries
}

/// 系统是否偏好浅色（读取 HKCU 个性化设置；读取失败回退浅色）
#[cfg(windows)]
fn os_prefers_light() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let subkey: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let value: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut data: u32 = 1;
    let mut size: u32 = std::mem::size_of::<u32>() as u32;
    let mut kind: u32 = 0;
    // SAFETY：指针均指向本函数栈上有效数据；size 初值为 u32 字节数。
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            &mut kind,
            (&mut data as *mut u32).cast(),
            &mut size,
        )
    };
    rc == 0 && data != 0
}

/// 非 Windows 平台的系统明暗（本项目仅发布 Windows；回退浅色便于测试）
#[cfg(not(windows))]
fn os_prefers_light() -> bool {
    true
}

/// 系统明暗对应的内置主题 id（`light` / `dark`）
pub fn system_base_id() -> &'static str {
    if os_prefers_light() {
        "light"
    } else {
        "dark"
    }
}

/// 解析主题（单驻留）：`setting_id` 为设置中的主题 id；
/// - `system` → 按系统明暗解析为 light/dark；
/// - 用户主题缺失/无效 → 回退系统主题并记录日志；
/// 成功后写入驻留缓存并返回解析结果。
pub fn resolve_theme(data_dir: &Path, setting_id: &str) -> Result<ResolvedTheme, ThemeError> {
    let requested = setting_id.trim();
    let requested = if requested.is_empty() {
        SYSTEM_THEME_ID
    } else {
        requested
    };
    let primary = if requested == SYSTEM_THEME_ID {
        system_base_id().to_string()
    } else {
        requested.to_string()
    };
    let (manifest, effective_id) = match load_manifest(data_dir, &primary) {
        Ok(manifest) => (manifest, primary),
        Err(err) => {
            if requested == SYSTEM_THEME_ID {
                return Err(err);
            }
            log::warn!(target: "sread::theme", "主题不可用，回退跟随系统：{err}");
            let fallback = system_base_id().to_string();
            (load_manifest(data_dir, &fallback)?, fallback)
        }
    };
    let resolved = ResolvedTheme {
        id: effective_id,
        base: manifest.base,
        tokens: manifest.tokens,
    };
    cache_store(&resolved);
    Ok(resolved)
}

/// 保存（新建或覆盖）用户主题：强校验 + 内置 id 保护 + 同名用户主题覆盖。
/// 与 `import_theme` 共用同一校验链；供设置界面「主题编辑器」直接调用。
pub fn save_manifest(data_dir: &Path, mut manifest: ThemeManifest) -> Result<ThemeSummary, ThemeError> {
    manifest.builtin = false;
    validate_manifest(&manifest).map_err(ThemeError::Invalid)?;
    if builtin_manifest(&manifest.id).is_some() {
        return Err(ThemeError::Invalid(format!(
            "内置主题 id 不可覆盖：{}",
            manifest.id
        )));
    }
    let target = user_theme_path(data_dir, &manifest.id);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| ThemeError::Io(e.to_string()))?;
    }
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| ThemeError::Io(e.to_string()))?;
    crate::storage::atomic::write_atomic_str(&target, &format!("{text}\n"))
        .map_err(|e| ThemeError::Io(e.to_string()))?;
    Ok(summary_of(&manifest))
}

/// 导入用户主题（校验后写入 `data/themes/`；与内置同名拒绝；同名用户主题覆盖）
pub fn import_theme(data_dir: &Path, source: &Path) -> Result<ThemeSummary, ThemeError> {
    let raw = fs::read_to_string(source).map_err(|e| ThemeError::Io(e.to_string()))?;
    let manifest: ThemeManifest = serde_json::from_str(&raw)
        .map_err(|e| ThemeError::Invalid(format!("JSON 解析失败：{e}")))?;
    save_manifest(data_dir, manifest)
}

/// 导出主题清单到指定路径（内置与用户主题均可导出）
pub fn export_theme(data_dir: &Path, id: &str, target: &Path) -> Result<(), ThemeError> {
    let manifest = load_manifest(data_dir, id)?;
    let text =
        serde_json::to_string_pretty(&manifest).map_err(|e| ThemeError::Io(e.to_string()))?;
    crate::storage::atomic::write_atomic_str(target, &format!("{text}\n"))
        .map_err(|e| ThemeError::Io(e.to_string()))
}

/// 删除用户主题（内置主题拒绝删除）
pub fn remove_theme(data_dir: &Path, id: &str) -> Result<(), ThemeError> {
    if builtin_manifest(id).is_some() {
        return Err(ThemeError::Invalid(format!("内置主题不可删除：{id}")));
    }
    let path = user_theme_path(data_dir, id);
    if !path.is_file() {
        return Err(ThemeError::NotFound(id.to_string()));
    }
    fs::remove_file(&path).map_err(|e| ThemeError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("创建临时目录失败")
    }

    /// 内置 6 套全部解析且通过强校验。
    #[test]
    fn builtin_manifests_all_valid() {
        assert_eq!(BUILTIN_THEMES.len(), 6);
        for (id, _) in BUILTIN_THEMES {
            let manifest = builtin_manifest(id).unwrap_or_else(|| panic!("{id} 解析失败"));
            assert!(manifest.builtin);
            validate_manifest(&manifest).unwrap_or_else(|e| panic!("{id} 校验失败：{e}"));
            assert_eq!(manifest.id, *id);
        }
    }

    /// 颜色校验：hex 与 CSS 函数式写法（rgb/rgba/hsl/hsla）均可用；注入字符被拒。
    #[test]
    fn color_validation_supports_css_functions() {
        assert!(is_valid_color("#abc"));
        assert!(is_valid_color("#AABBCC"));
        assert!(is_valid_color("#AABBCCDD"));
        assert!(is_valid_color("rgb(1,2,3)"));
        assert!(is_valid_color("rgba(1, 2, 3, 0.5)"));
        assert!(is_valid_color("hsl(200, 50%, 40%)"));
        assert!(is_valid_color("hsla(200deg,50%,40%,0.3)"));
        assert!(!is_valid_color("red"));
        assert!(!is_valid_color("rgb(1,2)"));
        assert!(!is_valid_color("#12345"));
        assert!(!is_valid_color("rgb(1,2,3); background:url(x)"));
        assert!(!is_valid_color("var(--x)"));
    }

    /// 保存用户主题：写入/覆盖/清单可见；内置 id 与非法值拒绝。
    #[test]
    fn save_manifest_writes_and_guards() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut manifest = builtin_manifest("light").expect("内置");
        manifest.id = "custom-test".into();
        manifest.name = "测试主题".into();
        manifest.name_en = "Test theme".into();
        let summary = save_manifest(dir.path(), manifest.clone()).expect("保存");
        assert_eq!(summary.id, "custom-test");
        assert!(user_theme_path(dir.path(), "custom-test").is_file());
        let loaded = load_manifest(dir.path(), "custom-test").expect("载入");
        assert_eq!(loaded.tokens.get("accent"), manifest.tokens.get("accent"));
        // 覆盖同 id 用户主题允许
        let mut again = manifest.clone();
        again.name = "改名字".into();
        save_manifest(dir.path(), again).expect("覆盖保存");
        assert_eq!(
            load_manifest(dir.path(), "custom-test").expect("载入").name,
            "改名字"
        );
        // 内置 id 拒绝
        let mut builtin_id = manifest.clone();
        builtin_id.id = "light".into();
        assert!(save_manifest(dir.path(), builtin_id).is_err());
        // 非法颜色拒绝
        let mut bad = manifest;
        bad.tokens.insert("ink".into(), "red".into());
        assert!(save_manifest(dir.path(), bad).is_err());
    }

    /// 清单校验：缺令牌 / 非法颜色 / 未知令牌 / 非法 id / 非法 base 均拒绝。
    #[test]
    fn validate_rejects_bad_manifests() {
        let base = builtin_manifest("light").expect("内置");
        assert!(validate_manifest(&base).is_ok());

        let mut missing = base.clone();
        missing.tokens.remove("ink");
        assert!(validate_manifest(&missing)
            .expect_err("缺令牌")
            .contains("ink"));

        let mut bad_color = base.clone();
        bad_color.tokens.insert("ink".into(), "red".into());
        assert!(validate_manifest(&bad_color)
            .expect_err("颜色")
            .contains("ink"));

        let mut extra = base.clone();
        extra.tokens.insert("glow".into(), "#FFFFFF".into());
        assert!(validate_manifest(&extra)
            .expect_err("未知")
            .contains("glow"));

        let mut bad_id = base.clone();
        bad_id.id = "System".into();
        assert!(validate_manifest(&bad_id).is_err());

        let mut reserved = base.clone();
        reserved.id = "system".into();
        assert!(validate_manifest(&reserved).is_err());

        let mut bad_base = base;
        bad_base.base = "sepia".into();
        assert!(validate_manifest(&bad_base).is_err());
    }

    /// 列表：内置 6 套在前；导入用户主题后追加；非法用户文件被跳过。
    #[test]
    fn list_includes_builtins_and_valid_users_only() {
        let dir = data_dir();
        let list = list_themes(dir.path());
        assert_eq!(list.len(), 6);
        assert!(list.iter().all(|s| s.builtin));

        let manifest = builtin_manifest("light").expect("内置");
        let mut user = manifest;
        user.id = "my-theme".into();
        user.name = "自定义".into();
        user.name_en = "Custom".into();
        let text = serde_json::to_string_pretty(&user).expect("序列化");
        let themes_dir = dir.path().join(USER_THEMES_DIR);
        fs::create_dir_all(&themes_dir).expect("建目录");
        fs::write(themes_dir.join("my-theme.json"), text).expect("写用户主题");
        fs::write(themes_dir.join("broken.json"), "{ not json").expect("写坏文件");

        let list = list_themes(dir.path());
        assert_eq!(list.len(), 7);
        assert_eq!(list[6].id, "my-theme");
        assert!(!list[6].builtin);
    }

    /// 导入/导出往返；内置 id 导入拒绝；文件名与内部 id 不一致拒绝。
    #[test]
    fn import_export_roundtrip_and_guards() {
        let dir = data_dir();
        let source_dir = data_dir();
        let manifest = builtin_manifest("dark").expect("内置");
        let mut user = manifest;
        user.id = "night-owl".into();
        user.name = "夜猫".into();
        user.name_en = "Night Owl".into();
        let source = source_dir.path().join("night-owl.json");
        fs::write(
            &source,
            serde_json::to_string_pretty(&user).expect("序列化"),
        )
        .expect("写源文件");

        let summary = import_theme(dir.path(), &source).expect("导入");
        assert_eq!(summary.id, "night-owl");
        assert!(!summary.builtin);

        let exported = source_dir.path().join("exported.json");
        export_theme(dir.path(), "night-owl", &exported).expect("导出");
        let reloaded: ThemeManifest =
            serde_json::from_str(&fs::read_to_string(&exported).expect("读导出"))
                .expect("解析导出");
        assert_eq!(reloaded.id, "night-owl");
        assert!(validate_manifest(&reloaded).is_ok());

        // 内置 id 不可导入覆盖
        let builtin_copy = source_dir.path().join("light.json");
        fs::write(
            &builtin_copy,
            serde_json::to_string_pretty(&builtin_manifest("light").expect("内置"))
                .expect("序列化"),
        )
        .expect("写内置副本");
        assert!(import_theme(dir.path(), &builtin_copy).is_err());

        // 文件内 id 与文件名不一致
        let mismatch_dir = dir.path().join(USER_THEMES_DIR);
        fs::write(
            mismatch_dir.join("wrong-name.json"),
            serde_json::to_string_pretty(&reloaded).expect("序列化"),
        )
        .expect("写错名文件");
        assert!(load_manifest(dir.path(), "wrong-name").is_err());
    }

    /// 删除：用户主题可删，内置拒绝，缺失报不存。
    #[test]
    fn remove_rules() {
        let dir = data_dir();
        assert!(remove_theme(dir.path(), "light").is_err());
        assert!(matches!(
            remove_theme(dir.path(), "ghost"),
            Err(ThemeError::NotFound(_))
        ));
        let source_dir = data_dir();
        let mut user = builtin_manifest("light").expect("内置");
        user.id = "temp-theme".into();
        let source = source_dir.path().join("temp.json");
        fs::write(
            &source,
            serde_json::to_string_pretty(&user).expect("序列化"),
        )
        .expect("写源");
        import_theme(dir.path(), &source).expect("导入");
        assert!(user_theme_path(dir.path(), "temp-theme").is_file());
        remove_theme(dir.path(), "temp-theme").expect("删除");
        assert!(!user_theme_path(dir.path(), "temp-theme").is_file());
    }

    /// 解析：具体 id 直取；system 解析为 light/dark；未知 id 回退系统；缓存单驻留。
    #[test]
    fn resolve_and_single_resident_cache() {
        let dir = data_dir();
        let dark = resolve_theme(dir.path(), "dark").expect("解析 dark");
        assert_eq!(dark.id, "dark");
        assert_eq!(dark.base, "dark");
        assert_eq!(dark.tokens.get("base").map(String::as_str), Some("#1E1E1E"));

        let system = resolve_theme(dir.path(), SYSTEM_THEME_ID).expect("解析 system");
        assert!(system.id == "light" || system.id == "dark");

        let fallback = resolve_theme(dir.path(), "no-such-theme").expect("回退");
        assert!(fallback.id == "light" || fallback.id == "dark");

        let cached = cached_theme().expect("缓存");
        assert_eq!(cached.id, fallback.id, "缓存应只保留最近一次解析结果");
    }
}
