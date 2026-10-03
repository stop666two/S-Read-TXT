//! 配置文件读写（`settings.json` / `reader.json` / `shortcuts.json`）。
//!
//! 策略：
//! - 读：文件缺失 → 默认值；内容损坏 → 备份为 `<文件名>.corrupt-<纳秒>` 后回退默认值
//!   （实现：`storage::json_io::load_json_or_default`）；
//! - 归一：载入与保存前对齐 `schemaVersion`、未知枚举回退默认、数值裁剪到文档范围；
//! - 写：经 `json_io` 原子落盘（pretty JSON、UTF-8 无 BOM）；
//! - 快捷键：文件仅存覆盖项；[`effective_bindings`] = 默认 + 覆盖；[`to_overrides`] = 与默认不同的项。

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::settings::defaults;
use crate::settings::model::AppSettings;
use crate::settings::reader::ReaderSettings;
use crate::settings::shortcuts::ShortcutSettings;
use crate::settings::{SettingsSaveRequest, SettingsSnapshot};
use crate::storage::json_io;

/// 主配置文件名
pub const FILE_APP_SETTINGS: &str = "settings.json";
/// 阅读排版配置文件名
pub const FILE_READER_SETTINGS: &str = "reader.json";
/// 快捷键配置文件名
pub const FILE_SHORTCUTS: &str = "shortcuts.json";

/// 主配置文件路径（数据目录内）
pub fn app_settings_path(dir: &Path) -> PathBuf {
    dir.join(FILE_APP_SETTINGS)
}

/// 阅读排版配置文件路径（数据目录内）
pub fn reader_settings_path(dir: &Path) -> PathBuf {
    dir.join(FILE_READER_SETTINGS)
}

/// 快捷键配置文件路径（数据目录内）
pub fn shortcuts_path(dir: &Path) -> PathBuf {
    dir.join(FILE_SHORTCUTS)
}

/// 载入聚合快照（app + reader + 生效快捷键），供 `get_settings` 命令使用。
pub fn load_snapshot(dir: &Path) -> SettingsSnapshot {
    let raw_shortcuts = load_shortcuts(dir);
    SettingsSnapshot {
        app: load_app_settings(dir),
        reader: load_reader_settings(dir),
        shortcuts: ShortcutSettings {
            schema_version: defaults::SCHEMA_VERSION,
            bindings: effective_bindings(&raw_shortcuts.bindings),
        },
    }
}

/// 载入主配置（自愈：缺失/损坏回退默认值）。
pub fn load_app_settings(dir: &Path) -> AppSettings {
    json_io::load_json_or_default(&app_settings_path(dir), normalize_app)
}

/// 载入阅读排版配置（自愈：缺失/损坏回退默认值）。
pub fn load_reader_settings(dir: &Path) -> ReaderSettings {
    json_io::load_json_or_default(&reader_settings_path(dir), normalize_reader)
}

/// 载入快捷键覆盖表（自愈：缺失/损坏回退空覆盖；未知动作与空绑定被丢弃）。
pub fn load_shortcuts(dir: &Path) -> ShortcutSettings {
    json_io::load_json_or_default(&shortcuts_path(dir), normalize_shortcuts)
}

/// 保存主配置（保存前归一，确保写入合法值）。
pub fn save_app_settings(dir: &Path, settings: &AppSettings) -> io::Result<()> {
    let mut copy = settings.clone();
    normalize_app(&mut copy);
    json_io::write_json_atomic(&app_settings_path(dir), &copy)
}

/// 保存阅读排版配置（保存前归一）。
pub fn save_reader_settings(dir: &Path, settings: &ReaderSettings) -> io::Result<()> {
    let mut copy = settings.clone();
    normalize_reader(&mut copy);
    json_io::write_json_atomic(&reader_settings_path(dir), &copy)
}

/// 保存快捷键覆盖表（保存前归一；传入的应是覆盖项，生效表请先用 [`to_overrides`] 转换）。
pub fn save_shortcuts(dir: &Path, settings: &ShortcutSettings) -> io::Result<()> {
    let mut copy = settings.clone();
    normalize_shortcuts(&mut copy);
    json_io::write_json_atomic(&shortcuts_path(dir), &copy)
}

/// 保存聚合快照（`save_settings` 命令入口）：
/// 主配置/阅读配置直接落盘；快捷键按「覆盖项」落盘（与默认相同的项不写，
/// 因此恢复默认 = 提交默认表 → 覆盖表清空）。
pub fn save_snapshot(dir: &Path, request: &SettingsSaveRequest) -> io::Result<()> {
    save_app_settings(dir, &request.app)?;
    save_reader_settings(dir, &request.reader)?;
    save_shortcuts(
        dir,
        &ShortcutSettings {
            schema_version: defaults::SCHEMA_VERSION,
            bindings: to_overrides(&request.shortcuts),
        },
    )?;
    Ok(())
}

/// 合并生效绑定：默认表为底，覆盖表覆盖（未知动作再防御性过滤一次）。
pub fn effective_bindings(overrides: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut effective = defaults::default_bindings();
    for (action, combo) in overrides {
        if defaults::is_known_action(action) {
            effective.insert(action.clone(), combo.clone());
        }
    }
    effective
}

/// 由生效绑定表反推覆盖表（只保留与默认不同的项；恢复默认 → 空表）。
pub fn to_overrides(effective: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let defaults_map = defaults::default_bindings();
    effective
        .iter()
        .filter(|(action, combo)| {
            defaults_map
                .get(*action)
                .map(|d| d != *combo)
                .unwrap_or(false)
        })
        .map(|(action, combo)| (action.clone(), combo.clone()))
        .collect()
}

/// 主配置归一：版本对齐、枚举回退、数值裁剪。
/// 双阈值一致性：只读阈值 ≤ 硬上限（若用户配置倒挂，以下方放宽准则修正硬上限）。
fn normalize_app(settings: &mut AppSettings) {
    settings.schema_version = defaults::SCHEMA_VERSION;
    settings.log_level = settings.log_level.normalized();
    settings.locale = settings.locale.normalized();
    let (min_size, max_size) = defaults::MAX_FILE_SIZE_MB_RANGE;
    settings.max_file_size_mb = settings.max_file_size_mb.clamp(min_size, max_size);
    let (min_hard, max_hard) = defaults::HARD_LIMIT_MB_RANGE;
    settings.hard_limit_mb = settings.hard_limit_mb.clamp(min_hard, max_hard);
    // 一致性：硬上限不得低于只读阈值（否则 只读阈值 < size ≤ 硬上限 的区间不存在）
    if settings.hard_limit_mb < settings.max_file_size_mb {
        settings.hard_limit_mb = settings.max_file_size_mb;
    }
    let (min_tabs, max_tabs) = defaults::MAX_TABS_RANGE;
    settings.max_tabs = settings.max_tabs.clamp(min_tabs, max_tabs);
    let (min_entries, max_entries) = defaults::HISTORY_MAX_ENTRIES_RANGE;
    settings.history.max_entries = settings.history.max_entries.clamp(min_entries, max_entries);
    let (min_days, max_days) = defaults::HISTORY_RETENTION_DAYS_RANGE;
    settings.history.retention_days = settings.history.retention_days.clamp(min_days, max_days);
}

/// 阅读排版归一：主题回退、字体去空白、数值裁剪。
fn normalize_reader(settings: &mut ReaderSettings) {
    settings.schema_version = defaults::SCHEMA_VERSION;
    settings.theme = settings.theme.normalized();
    let font = settings.typography.font_family.trim();
    settings.typography.font_family = if font.is_empty() {
        defaults::DEFAULT_FONT_FAMILY.to_string()
    } else {
        font.to_string()
    };
    let (min_size, max_size) = defaults::FONT_SIZE_RANGE;
    settings.typography.font_size = settings.typography.font_size.clamp(min_size, max_size);
    let (min_lh, max_lh) = defaults::LINE_HEIGHT_RANGE;
    settings.typography.line_height = settings.typography.line_height.clamp(min_lh, max_lh);
    let (min_width, max_width) = defaults::CONTENT_WIDTH_RANGE;
    settings.typography.content_width = settings
        .typography
        .content_width
        .clamp(min_width, max_width);
    let (min_pad, max_pad) = defaults::PAGE_PADDING_RANGE;
    settings.typography.page_padding = settings.typography.page_padding.clamp(min_pad, max_pad);
    let (min_pad_y, max_pad_y) = defaults::PAGE_PADDING_Y_RANGE;
    settings.typography.page_padding_y = settings
        .typography
        .page_padding_y
        .clamp(min_pad_y, max_pad_y);
    let (min_para, max_para) = defaults::PARAGRAPH_SPACING_RANGE;
    settings.typography.paragraph_spacing = settings
        .typography
        .paragraph_spacing
        .clamp(min_para, max_para);
    let (min_indent, max_indent) = defaults::FIRST_LINE_INDENT_RANGE;
    settings.typography.first_line_indent = settings
        .typography
        .first_line_indent
        .clamp(min_indent, max_indent);
    settings.typography.text_align = settings.typography.text_align.normalized();
}

/// 快捷键归一：版本对齐；丢弃未知动作与空绑定（记日志）。
fn normalize_shortcuts(settings: &mut ShortcutSettings) {
    settings.schema_version = defaults::SCHEMA_VERSION;
    let mut cleaned = BTreeMap::new();
    for (action, combo) in std::mem::take(&mut settings.bindings) {
        let action = action.trim().to_string();
        let combo = combo.trim().to_string();
        if !defaults::is_known_action(&action) {
            log::warn!("快捷键配置包含未知动作，已忽略：{action}");
            continue;
        }
        if combo.is_empty() {
            log::warn!("快捷键配置包含空绑定，已忽略：{action}");
            continue;
        }
        cleaned.insert(action, combo);
    }
    settings.bindings = cleaned;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("创建临时目录失败")
    }

    /// 主配置往返一致（保存会写入当前 schemaVersion）。
    #[test]
    fn app_settings_roundtrip() {
        let dir = data_dir();
        let mut settings = AppSettings::default();
        settings.max_file_size_mb = 256;
        settings.hard_limit_mb = 8192;
        settings.max_tabs = 30;
        settings.history.max_entries = 500;
        settings.log_level = crate::settings::model::LogLevel::Debug;
        settings.save_backup_enabled = false;
        save_app_settings(dir.path(), &settings).expect("保存失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(loaded, settings);
        let raw = std::fs::read_to_string(app_settings_path(dir.path())).expect("读取失败");
        assert!(
            raw.contains(&format!("\"schemaVersion\": {}", defaults::SCHEMA_VERSION)),
            "保存应写入当前 schemaVersion：{raw}"
        );
    }

    /// 主配置缺失 → 默认值。
    #[test]
    fn app_settings_missing_returns_defaults() {
        let dir = data_dir();
        assert_eq!(load_app_settings(dir.path()), AppSettings::default());
    }

    /// 主配置损坏 → 备份文件生成 + 回退默认值。
    #[test]
    fn app_settings_corrupt_backs_up_and_defaults() {
        let dir = data_dir();
        std::fs::write(app_settings_path(dir.path()), b"{ broken").expect("写损坏文件失败");
        assert_eq!(load_app_settings(dir.path()), AppSettings::default());
        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .expect("列目录失败")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("settings.json.corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1, "应生成一个备份：{backups:?}");
    }

    /// 界面语言：未知值归一为 zh-CN；已知值往返保留。
    #[test]
    fn app_settings_normalizes_locale() {
        let dir = data_dir();
        let mut settings = AppSettings::default();
        settings.locale = crate::settings::model::Language::Unknown;
        save_app_settings(dir.path(), &settings).expect("保存失败");
        assert_eq!(
            load_app_settings(dir.path()).locale,
            crate::settings::model::Language::ZhCn
        );

        settings.locale = crate::settings::model::Language::En;
        save_app_settings(dir.path(), &settings).expect("保存失败");
        let raw = std::fs::read_to_string(app_settings_path(dir.path())).expect("读取失败");
        assert!(raw.contains("\"locale\": \"en\""), "en 应落盘：{raw}");
        assert_eq!(
            load_app_settings(dir.path()).locale,
            crate::settings::model::Language::En
        );
    }

    /// 主配置数值裁剪 + 未知日志级别归一。
    #[test]
    fn app_settings_clamps_and_normalizes_unknown_values() {
        let dir = data_dir();
        std::fs::write(
            app_settings_path(dir.path()),
            br#"{"schemaVersion":99,"logLevel":"trace","maxFileSizeMB":0,"maxTabs":9999,"history":{"maxEntries":0,"retentionDays":99999},"saveBackupEnabled":false,"showOnboarding":false}"#,
        )
        .expect("写配置失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(loaded.schema_version, defaults::SCHEMA_VERSION);
        assert_eq!(loaded.log_level, crate::settings::model::LogLevel::Info);
        assert_eq!(loaded.max_file_size_mb, 1);
        assert_eq!(loaded.max_tabs, 200);
        assert_eq!(loaded.history.max_entries, 100);
        assert_eq!(loaded.history.retention_days, 36500);
        assert!(!loaded.save_backup_enabled);
        assert!(!loaded.show_onboarding);
        // 缺失 hardLimitMB → 默认 2048；一致性修正后仍 ≥ 只读阈值
        assert_eq!(loaded.hard_limit_mb, 2048);
    }

    /// 双阈值一致性：硬上限低于只读阈值时修正为只读阈值；越界钳制。
    #[test]
    fn hard_limit_inverts_are_corrected() {
        let dir = data_dir();
        std::fs::write(
            app_settings_path(dir.path()),
            br#"{"maxFileSizeMB":512,"hardLimitMB":100}"#,
        )
        .expect("写配置失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(loaded.max_file_size_mb, 512);
        assert_eq!(loaded.hard_limit_mb, 512, "硬上限应被修正为不低于只读阈值");

        std::fs::write(
            app_settings_path(dir.path()),
            br#"{"maxFileSizeMB":100,"hardLimitMB":99999999}"#,
        )
        .expect("写配置失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(loaded.hard_limit_mb, 16384, "硬上限应钳制到范围上限");
    }

    /// 阅读配置：未知主题归一、空字体回退、数值裁剪。
    #[test]
    fn reader_settings_clamps_and_falls_back() {
        let dir = data_dir();
        std::fs::write(
            reader_settings_path(dir.path()),
            br#"{"theme":"neon","typography":{"fontFamily":"   ","fontSize":100,"lineHeight":0.5,"contentWidth":10,"pagePadding":1000,"pagePaddingY":999,"paragraphSpacing":999,"firstLineIndent":99,"textAlign":"diagonal"},"statusBar":{"showSize":true,"showEncoding":false}}"#,
        )
        .expect("写配置失败");
        let loaded = load_reader_settings(dir.path());
        assert_eq!(loaded.theme, crate::settings::reader::Theme::System);
        assert_eq!(loaded.typography.font_family, defaults::DEFAULT_FONT_FAMILY);
        assert_eq!(loaded.typography.font_size, 72);
        assert!((loaded.typography.line_height - 1.0).abs() < f32::EPSILON);
        assert_eq!(loaded.typography.content_width, 320);
        assert_eq!(loaded.typography.page_padding, 240);
        assert_eq!(loaded.typography.page_padding_y, 240);
        assert_eq!(loaded.typography.paragraph_spacing, 64);
        assert_eq!(loaded.typography.first_line_indent, 8);
        assert_eq!(
            loaded.typography.text_align,
            crate::settings::reader::TextAlign::Left
        );
        assert!(loaded.status_bar.show_size);
        assert!(!loaded.status_bar.show_encoding, "显式 false 应保留");
        assert!(loaded.status_bar.show_file_name, "缺失字段取默认 true");
        assert!(loaded.status_bar.show_percent, "缺失字段取默认 true");
    }

    /// 旧版 reader.json（无 pagePaddingY 字段）加载 → 取默认 48（向后兼容）。
    #[test]
    fn reader_settings_missing_padding_y_defaults() {
        let dir = data_dir();
        std::fs::write(
            reader_settings_path(dir.path()),
            br#"{"theme":"light","typography":{"fontFamily":"Arial","fontSize":16,"lineHeight":1.8,"contentWidth":720,"pagePadding":48}}"#,
        )
        .expect("写配置失败");
        let loaded = load_reader_settings(dir.path());
        assert_eq!(loaded.typography.font_size, 16);
        assert_eq!(
            loaded.typography.page_padding_y,
            defaults::DEFAULT_PAGE_PADDING_Y
        );
        assert_eq!(
            loaded.typography.paragraph_spacing,
            defaults::DEFAULT_PARAGRAPH_SPACING
        );
        assert_eq!(
            loaded.typography.first_line_indent,
            defaults::DEFAULT_FIRST_LINE_INDENT
        );
        assert!(loaded.typography.smooth_scroll);
        assert_eq!(
            loaded.status_bar,
            crate::settings::reader::StatusBarSettings::default()
        );
    }

    /// 阅读配置往返一致。
    #[test]
    fn reader_settings_roundtrip() {
        let dir = data_dir();
        let mut settings = ReaderSettings::default();
        settings.theme = crate::settings::reader::Theme::Eye;
        settings.typography.font_size = 20;
        settings.typography.line_height = 2.0;
        save_reader_settings(dir.path(), &settings).expect("保存失败");
        assert_eq!(load_reader_settings(dir.path()), settings);
    }

    /// 快捷键：生效表 = 默认 + 覆盖。
    #[test]
    fn shortcuts_effective_merges_defaults_and_overrides() {
        let overrides: BTreeMap<String, String> = [("openFile".to_string(), "Alt+O".to_string())]
            .into_iter()
            .collect();
        let effective = effective_bindings(&overrides);
        assert_eq!(effective.get("openFile").map(String::as_str), Some("Alt+O"));
        assert_eq!(effective.get("save").map(String::as_str), Some("Ctrl+S"));
        assert_eq!(effective.len(), defaults::DEFAULT_BINDINGS.len());
    }

    /// 快捷键：`to_overrides` 仅保留与默认不同的项（恢复默认 → 空表）。
    #[test]
    fn shortcuts_to_overrides_filters_defaults() {
        let mut effective = defaults::default_bindings();
        effective.insert("closeTab".to_string(), "Ctrl+Q".to_string());
        let overrides = to_overrides(&effective);
        assert_eq!(overrides.len(), 1);
        assert_eq!(
            overrides.get("closeTab").map(String::as_str),
            Some("Ctrl+Q")
        );
        assert!(to_overrides(&defaults::default_bindings()).is_empty());
    }

    /// 快捷键：未知动作与空绑定在加载时丢弃。
    #[test]
    fn shortcuts_drop_unknown_and_empty_on_load() {
        let dir = data_dir();
        std::fs::write(
            shortcuts_path(dir.path()),
            br#"{"schemaVersion":1,"bindings":{"openFile":"Alt+O","notAnAction":"Ctrl+X","save":"  "}}"#,
        )
        .expect("写配置失败");
        let loaded = load_shortcuts(dir.path());
        assert_eq!(loaded.bindings.len(), 1);
        assert_eq!(
            loaded.bindings.get("openFile").map(String::as_str),
            Some("Alt+O")
        );
    }

    /// 快捷键：保存覆盖表并回读（文件只含覆盖项）。
    #[test]
    fn shortcuts_roundtrip_stores_only_overrides() {
        let dir = data_dir();
        let mut effective = defaults::default_bindings();
        effective.insert("fullscreen".to_string(), "Alt+Enter".to_string());
        save_shortcuts(
            dir.path(),
            &ShortcutSettings {
                schema_version: 1,
                bindings: to_overrides(&effective),
            },
        )
        .expect("保存失败");
        let raw = std::fs::read_to_string(shortcuts_path(dir.path())).expect("读取失败");
        assert!(raw.contains("Alt+Enter"));
        assert!(!raw.contains("Ctrl+O"), "默认项不应落盘：{raw}");
        let loaded = load_shortcuts(dir.path());
        assert_eq!(loaded.bindings.len(), 1);
    }

    /// 聚合保存：快捷键只落盘覆盖项；回读快照与提交的生效配置一致。
    #[test]
    fn save_snapshot_roundtrip_stores_only_overrides() {
        let dir = data_dir();
        let mut effective = defaults::default_bindings();
        effective.insert("openFile".to_string(), "Alt+O".to_string());
        let mut app = AppSettings::default();
        app.max_tabs = 25;
        let mut reader = ReaderSettings::default();
        reader.theme = crate::settings::reader::Theme::Dark;
        let request = crate::settings::SettingsSaveRequest {
            app: app.clone(),
            reader: reader.clone(),
            shortcuts: effective.clone(),
        };
        save_snapshot(dir.path(), &request).expect("保存失败");
        let raw = std::fs::read_to_string(shortcuts_path(dir.path())).expect("读取失败");
        assert!(raw.contains("Alt+O"), "覆盖项应落盘：{raw}");
        assert!(!raw.contains("Ctrl+W"), "默认项不应落盘：{raw}");
        let snapshot = load_snapshot(dir.path());
        assert_eq!(snapshot.app.max_tabs, 25);
        assert_eq!(snapshot.reader.theme, crate::settings::reader::Theme::Dark);
        assert_eq!(snapshot.shortcuts.bindings, effective);
    }
}
