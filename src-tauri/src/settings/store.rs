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
use crate::settings::display::DisplaySettings;
use crate::settings::editor::EditorSettings;
use crate::settings::model::AppSettings;
use crate::settings::reader::ReaderSettings;
use crate::settings::shortcuts::ShortcutSettings;
use crate::settings::status::StatusSettings;
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
    normalize_editor(&mut settings.editor);
    normalize_find(settings);
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
    normalize_status(&mut settings.status);
    normalize_display(&mut settings.display);
    normalize_file(&mut settings.file);
}

/// 文件与快照设置归一：编码回退、换行归一、数值钳制、扩展名清洗。
fn normalize_file(file: &mut crate::settings::file::FileSettings) {
    const KNOWN_ENCODINGS: [&str; 8] = [
        "UTF-8",
        "GB18030",
        "UTF-16LE",
        "UTF-16BE",
        "Big5",
        "Shift_JIS",
        "EUC-KR",
        "windows-1252",
    ];
    let encoding = file.new_encoding.trim();
    file.new_encoding = if KNOWN_ENCODINGS.contains(&encoding) {
        encoding.to_string()
    } else {
        log::warn!("未知的新建文件编码，已回退默认：{encoding}");
        defaults::DEFAULT_FILE_NEW_ENCODING.to_string()
    };
    file.new_eol = file.new_eol.normalized();
    let (min_interval, max_interval) = defaults::FILE_AUTOSAVE_INTERVAL_RANGE;
    file.autosave_interval_sec = file.autosave_interval_sec.clamp(min_interval, max_interval);
    let (min_keep, max_keep) = defaults::FILE_SNAPSHOT_KEEP_RANGE;
    file.snapshot_keep = file.snapshot_keep.clamp(min_keep, max_keep);
    let (min_mb, max_mb) = defaults::FILE_SNAPSHOT_MAX_MB_RANGE;
    file.snapshot_max_mb = file.snapshot_max_mb.clamp(min_mb, max_mb);
    let (min_recent, max_recent) = defaults::FILE_RECENT_LIMIT_RANGE;
    file.recent_limit = file.recent_limit.clamp(min_recent, max_recent);
    let mut seen = std::collections::BTreeSet::new();
    let mut cleaned: Vec<String> = Vec::new();
    for raw in std::mem::take(&mut file.associations) {
        let mut ext = raw.trim().to_ascii_lowercase();
        if !ext.starts_with('.') {
            ext.insert(0, '.');
        }
        let body_ok = ext.len() > 1
            && ext.len() <= defaults::FILE_ASSOCIATION_MAX_CHARS as usize
            && ext[1..]
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '+' || ch == '-' || ch == '.');
        if !body_ok || !seen.insert(ext.clone()) {
            continue;
        }
        cleaned.push(ext);
        if cleaned.len() >= defaults::FILE_ASSOCIATIONS_MAX_ITEMS as usize {
            break;
        }
    }
    if cleaned.is_empty() {
        cleaned = defaults::DEFAULT_FILE_ASSOCIATIONS
            .iter()
            .map(|ext| (*ext).to_string())
            .collect();
    }
    file.associations = cleaned;
}

/// 显示选项归一：标尺位置钳制、不可见标记白名单/去重。
fn normalize_display(settings: &mut DisplaySettings) {
    let (min_pos, max_pos) = defaults::DISPLAY_RULER_POSITION_RANGE;
    settings.ruler_position = settings.ruler_position.clamp(min_pos, max_pos);
    let mut marks: Vec<String> = Vec::new();
    for raw in std::mem::take(&mut settings.invisible) {
        let id = raw.trim().to_string();
        if id.is_empty() {
            continue;
        }
        if !defaults::DISPLAY_INVISIBLE_IDS.contains(&id.as_str()) {
            log::warn!("不可见字符标记包含未知取值，已忽略：{id}");
            continue;
        }
        if marks.contains(&id) {
            continue;
        }
        marks.push(id);
    }
    settings.invisible = marks;
    settings.folding = settings.folding.normalized();
    let mut patterns: Vec<String> = Vec::new();
    for raw in std::mem::take(&mut settings.outline_patterns) {
        let pattern = raw.trim().to_string();
        if pattern.is_empty() {
            continue;
        }
        if pattern.chars().count() > defaults::OUTLINE_PATTERN_MAX_CHARS as usize {
            log::warn!("大纲正则过长，已忽略：{pattern}");
            continue;
        }
        if regex::Regex::new(&pattern).is_err() {
            log::warn!("大纲正则语法非法，已忽略：{pattern}");
            continue;
        }
        if patterns.contains(&pattern) {
            continue;
        }
        if patterns.len() >= defaults::OUTLINE_PATTERNS_MAX_ITEMS as usize {
            log::warn!("大纲正则超过数量上限，多余项已忽略");
            break;
        }
        patterns.push(pattern);
    }
    settings.outline_patterns = if patterns.is_empty() {
        defaults::DEFAULT_OUTLINE_PATTERNS
            .iter()
            .map(|pattern| (*pattern).to_string())
            .collect()
    } else {
        patterns
    };
}

/// 状态栏归一：显示项白名单/去重/回退默认、计数模式、宽度钳制、提示文案截断。
fn normalize_status(settings: &mut StatusSettings) {
    let mut items: Vec<String> = Vec::new();
    for raw in std::mem::take(&mut settings.items) {
        let id = raw.trim().to_string();
        if id.is_empty() {
            continue;
        }
        if !defaults::STATUS_ITEM_IDS.contains(&id.as_str()) {
            log::warn!("状态栏显示项包含未知取值，已忽略：{id}");
            continue;
        }
        if items.contains(&id) {
            continue;
        }
        items.push(id);
        if items.len() >= defaults::STATUS_ITEMS_MAX as usize {
            break;
        }
    }
    settings.items = if items.is_empty() {
        StatusSettings::default().items
    } else {
        items
    };
    settings.count_mode = settings.count_mode.normalized();
    let (min_width, max_width) = defaults::STATUS_TAB_WIDTH_RANGE;
    settings.tab_width = settings.tab_width.clamp(min_width, max_width);
    settings.empty_selection_text = settings
        .empty_selection_text
        .trim()
        .chars()
        .take(defaults::STATUS_EMPTY_SELECTION_MAX_CHARS)
        .collect();
    if settings.empty_selection_text.is_empty() {
        settings.empty_selection_text = defaults::DEFAULT_STATUS_EMPTY_SELECTION.to_string();
    }
}

/// 编辑器默认值归一：枚举回退、缩进宽度钳制、分隔符非法回退默认。
fn normalize_editor(editor: &mut EditorSettings) {
    let lines = &mut editor.lines;
    lines.default_scope = lines.default_scope.normalized();
    lines.sort_mode = lines.sort_mode.normalized();
    lines.dedupe_mode = lines.dedupe_mode.normalized();
    lines.indent_style = lines.indent_style.normalized();
    lines.case_default = lines.case_default.normalized();
    let (min_width, max_width) = defaults::LINE_INDENT_WIDTH_RANGE;
    lines.indent_width = lines.indent_width.clamp(min_width, max_width);
    let delimiter_ok = !lines.column_delimiter.is_empty()
        && lines.column_delimiter.chars().count() <= defaults::LINE_COLUMN_DELIMITER_MAX_CHARS;
    if !delimiter_ok {
        lines.column_delimiter = defaults::DEFAULT_LINE_COLUMN_DELIMITER.to_string();
    }
    let multi = &mut editor.multi_cursor;
    multi.rect_modifier = multi.rect_modifier.normalized();
    let (min_count, max_count) = defaults::MULTI_CURSOR_MAX_COUNT_RANGE;
    multi.max_count = multi.max_count.clamp(min_count, max_count);
    let clipboard = &mut editor.clipboard;
    let (min_limit, max_limit) = defaults::CLIPBOARD_HISTORY_LIMIT_RANGE;
    clipboard.history_limit = clipboard.history_limit.clamp(min_limit, max_limit);
    editor.insert.timestamp_format = editor.insert.timestamp_format.normalized();
}

/// 查找/正则归一：范围钳制、枚举回退、颜色与正则库合法性校验。
fn normalize_find(settings: &mut AppSettings) {
    let find = &mut settings.find;
    find.default_scope = find.default_scope.normalized();
    let (min_history, max_history) = defaults::FIND_HISTORY_LIMIT_RANGE;
    find.history_limit = find.history_limit.clamp(min_history, max_history);
    let (min_mf, max_mf) = defaults::FIND_MULTIFILE_CONCURRENCY_RANGE;
    find.multifile_concurrency = find.multifile_concurrency.clamp(min_mf, max_mf);
    find.highlight_color = normalize_color(&find.highlight_color);
    let regex = &mut settings.regex;
    let (min_timeout, max_timeout) = defaults::REGEX_TIMEOUT_MS_RANGE;
    regex.timeout_ms = regex.timeout_ms.clamp(min_timeout, max_timeout);
    regex.library = normalize_regex_library(std::mem::take(&mut regex.library));
}

/// 颜色归一：允许 空 / `#RGB` / `#RRGGBB` / `#RRGGBBAA` / `rgb()` / `rgba()`；非法回退空串（跟随主题）。
fn normalize_color(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || is_valid_color(trimmed) {
        return trimmed.to_string();
    }
    log::warn!("查找高亮颜色非法，已回退主题内置色：{trimmed}");
    String::new()
}

/// 颜色格式校验（供归一、注册表导入校验与主题令牌共用）。
/// 支持 CSS 常用写法：`#RGB` / `#RRGGBB` / `#RRGGBBAA` / `rgb()` / `rgba()` / `hsl()` / `hsla()`；
/// 并拒绝可能破坏样式表的字符（引号/分号/大括号等，作为写入 CSS 变量前的防线）。
pub(crate) fn is_valid_color(value: &str) -> bool {
    if value
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || "#(),.%+- ".contains(c)))
    {
        return false;
    }
    let lowered = value.to_ascii_lowercase();
    if let Some(hex) = lowered.strip_prefix('#') {
        return matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    for (prefix, components, percent_ok) in [
        ("rgb(", 3usize, false),
        ("rgba(", 4usize, false),
        ("hsl(", 3usize, true),
        ("hsla(", 4usize, true),
    ] {
        if let Some(inner) = lowered
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(')'))
        {
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() != components {
                return false;
            }
            return parts.iter().enumerate().all(|(index, part)| {
                let part = part.trim();
                if index == 3 {
                    return part
                        .parse::<f64>()
                        .map(|alpha| (0.0..=1.0).contains(&alpha))
                        .unwrap_or(false);
                }
                if percent_ok {
                    let cleaned = part
                        .strip_suffix('%')
                        .or_else(|| part.strip_suffix("deg"))
                        .unwrap_or(part);
                    return cleaned.trim().parse::<f64>().is_ok();
                }
                part.parse::<u8>().is_ok()
            });
        }
    }
    false
}

/// 正则库归一：去空白/去重（保序）/剔除超长项与非法正则；数量超限截断。
fn normalize_regex_library(items: Vec<String>) -> Vec<String> {
    let max_chars = defaults::REGEX_LIBRARY_MAX_CHARS as usize;
    let max_items = defaults::REGEX_LIBRARY_MAX_ITEMS as usize;
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().count() > max_chars {
            log::warn!("正则库条目过长（> {max_chars} 字符），已忽略");
            continue;
        }
        if regex::Regex::new(trimmed).is_err() {
            log::warn!("正则库条目语法非法，已忽略：{trimmed}");
            continue;
        }
        if out.iter().any(|existing| existing == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
        if out.len() >= max_items {
            break;
        }
    }
    out
}

/// 阅读排版归一：主题回退、字体去空白、数值裁剪。
fn normalize_reader(settings: &mut ReaderSettings) {
    settings.schema_version = defaults::SCHEMA_VERSION;
    settings.theme_id = settings.theme_id.trim().to_string();
    if settings.theme_id.is_empty() {
        settings.theme_id = defaults::DEFAULT_THEME_ID.to_string();
    }
    let (min_anim, max_anim) = defaults::THEME_ANIM_MS_RANGE;
    settings.theme_anim_ms = settings.theme_anim_ms.clamp(min_anim, max_anim);
    let background = &mut settings.background;
    background.fill = background.fill.normalized();
    background.file = background
        .file
        .take()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let (min_opacity, max_opacity) = defaults::BACKGROUND_OPACITY_RANGE;
    background.opacity = background.opacity.clamp(min_opacity, max_opacity);
    let (min_blur, max_blur) = defaults::BACKGROUND_BLUR_RANGE;
    background.blur = background.blur.clamp(min_blur, max_blur);
    let (min_dim, max_dim) = defaults::BACKGROUND_DIM_RANGE;
    background.dim = background.dim.clamp(min_dim, max_dim);
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
    let (min_margin, max_margin) = defaults::MARGIN_RANGE;
    for margin in [&mut settings.margins.reading, &mut settings.margins.editing] {
        margin.top = margin.top.clamp(min_margin, max_margin);
        margin.right = margin.right.clamp(min_margin, max_margin);
        margin.bottom = margin.bottom.clamp(min_margin, max_margin);
        margin.left = margin.left.clamp(min_margin, max_margin);
    }
    settings.reading.page_mode = settings.reading.page_mode.normalized();
    let (min_cols, max_cols) = defaults::READING_COLUMNS_RANGE;
    settings.reading.columns = settings.reading.columns.clamp(min_cols, max_cols);
    let (min_speed, max_speed) = defaults::AUTO_SCROLL_SPEED_RANGE;
    settings.reading.auto_scroll_speed = settings
        .reading
        .auto_scroll_speed
        .clamp(min_speed, max_speed);
    if settings.reading.eye_care_interval_min != 0 {
        let (min_eye, max_eye) = defaults::EYE_CARE_INTERVAL_RANGE;
        settings.reading.eye_care_interval_min = settings
            .reading
            .eye_care_interval_min
            .clamp(min_eye, max_eye);
    }
    let (min_pomo, max_pomo) = defaults::POMODORO_MIN_RANGE;
    settings.reading.pomodoro_min = settings.reading.pomodoro_min.clamp(min_pomo, max_pomo);
    let (min_panim, max_panim) = defaults::PAGE_ANIM_MS_RANGE;
    settings.reading.page_anim_ms = settings.reading.page_anim_ms.clamp(min_panim, max_panim);
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

    /// file 节归一：编码回退、换行归一、钳制、扩展名清洗。
    #[test]
    fn file_settings_normalize_on_load() {
        let dir = data_dir();
        let mut settings = AppSettings::default();
        settings.file.new_encoding = " utf-8 ".to_string();
        settings.file.new_eol = crate::settings::file::NewEol::Unknown;
        settings.file.autosave_interval_sec = 1;
        settings.file.snapshot_keep = 0;
        settings.file.snapshot_max_mb = 5;
        settings.file.recent_limit = 999;
        settings.file.associations = vec!["TXT".into(), "md".into(), ".md".into(), "bad/x".into()];
        save_app_settings(dir.path(), &settings).expect("保存失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(loaded.file.new_encoding, "UTF-8");
        assert_eq!(loaded.file.new_eol, crate::settings::file::NewEol::Lf);
        assert_eq!(loaded.file.autosave_interval_sec, 5);
        assert_eq!(loaded.file.snapshot_keep, 1);
        assert_eq!(loaded.file.snapshot_max_mb, 10);
        assert_eq!(loaded.file.recent_limit, 200);
        assert_eq!(
            loaded.file.associations,
            vec![".txt".to_string(), ".md".to_string()]
        );
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

    /// 界面语言反序列化：常见变体归一（zh / zh-Hans / en-US / EN）；未知值归入默认。
    #[test]
    fn locale_deserialization_accepts_variants() {
        use crate::settings::model::Language;
        for (raw, expected) in [
            ("zh-CN", Language::ZhCn),
            ("zh", Language::ZhCn),
            ("zh-Hans", Language::ZhCn),
            ("en-US", Language::En),
            ("EN", Language::En),
        ] {
            let parsed: Language = serde_json::from_value(serde_json::json!(raw)).expect("解析");
            assert_eq!(parsed, expected, "{raw}");
        }
        let unknown: Language = serde_json::from_value(serde_json::json!("fr")).expect("解析");
        assert_eq!(unknown, Language::Unknown);
        assert_eq!(unknown.normalized(), Language::ZhCn);
    }

    /// 查找/正则设置：颜色非法回退空、正则库去重与非法/超长项剔除、超时与历史上限钳制。
    #[test]
    fn find_and_regex_normalize_on_load() {
        let dir = data_dir();
        let mut settings = AppSettings::default();
        settings.find.highlight_color = "not-a-color".to_string();
        settings.find.history_limit = 999_999;
        settings.find.default_scope = crate::settings::model::FindScope::Unknown;
        settings.regex.timeout_ms = 5;
        settings.regex.library = vec![
            "  a+  ".to_string(),
            "a+".to_string(),
            "[".to_string(),
            "x".repeat(600),
            "".to_string(),
        ];
        save_app_settings(dir.path(), &settings).expect("保存失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(loaded.find.highlight_color, "");
        assert_eq!(loaded.find.history_limit, 100_000);
        assert_eq!(
            loaded.find.default_scope,
            crate::settings::model::FindScope::Document
        );
        assert_eq!(loaded.regex.timeout_ms, 10);
        assert_eq!(loaded.regex.library, vec!["a+".to_string()]);
    }

    /// 主配置数值裁剪 + 未知日志级别归一。
    #[test]
    fn app_settings_clamps_and_normalizes_unknown_values() {
        let dir = data_dir();
        std::fs::write(
            app_settings_path(dir.path()),
            br#"{"schemaVersion":99,"logLevel":"trace","maxFileSizeMB":0,"maxTabs":9999,"history":{"maxEntries":0,"retentionDays":999999},"saveBackupEnabled":false,"showOnboarding":false}"#,
        )
        .expect("写配置失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(loaded.schema_version, defaults::SCHEMA_VERSION);
        assert_eq!(loaded.log_level, crate::settings::model::LogLevel::Info);
        assert_eq!(loaded.max_file_size_mb, 1);
        assert_eq!(loaded.max_tabs, 2000);
        assert_eq!(loaded.history.max_entries, 100);
        assert_eq!(loaded.history.retention_days, 365000);
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
        assert_eq!(loaded.hard_limit_mb, 1_048_576, "硬上限应钳制到范围上限");
    }

    /// 阅读配置：未知主题 id 保留（可能为用户主题）、空值回退默认、空字体回退、数值裁剪。
    #[test]
    fn reader_settings_clamps_and_falls_back() {
        let dir = data_dir();
        std::fs::write(
            reader_settings_path(dir.path()),
            br#"{"theme":"neon","themeAnimMs":99999,"typography":{"fontFamily":"   ","fontSize":1000,"lineHeight":0.1,"contentWidth":10,"paragraphSpacing":99999,"firstLineIndent":999,"textAlign":"diagonal"},"margins":{"reading":{"top":0,"right":99999,"bottom":99999,"left":99999},"editing":{"top":1,"right":2,"bottom":3,"left":4}}}"#,
        )
        .expect("写配置失败");
        let loaded = load_reader_settings(dir.path());
        assert_eq!(loaded.theme_id, "neon", "未知主题 id 保留");
        assert_eq!(loaded.theme_anim_ms, 10_000);
        assert_eq!(loaded.typography.font_family, defaults::DEFAULT_FONT_FAMILY);
        assert_eq!(loaded.typography.font_size, 512);
        assert!((loaded.typography.line_height - 0.5).abs() < f32::EPSILON);
        assert_eq!(loaded.typography.content_width, 160);
        assert_eq!(loaded.margins.reading.top, 0);
        assert_eq!(loaded.margins.reading.right, 2000);
        assert_eq!(loaded.margins.reading.bottom, 2000);
        assert_eq!(loaded.margins.reading.left, 2000);
        assert_eq!(loaded.margins.editing.top, 1);
        assert_eq!(loaded.margins.editing.left, 4);
        assert_eq!(loaded.typography.paragraph_spacing, 2000);
        assert_eq!(loaded.typography.first_line_indent, 200);
        assert_eq!(
            loaded.typography.text_align,
            crate::settings::reader::TextAlign::Left
        );
    }

    /// 剪贴板历史设置归一：上限钳制（0 = 禁用保留），持久化开关原样。
    #[test]
    fn editor_clipboard_normalizes_on_load() {
        let dir = data_dir();
        let mut app = AppSettings::default();
        app.editor.clipboard.history_limit = 999_999;
        app.editor.clipboard.persist = false;
        save_app_settings(dir.path(), &app).expect("保存失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(loaded.editor.clipboard.history_limit, 200_000);
        assert!(!loaded.editor.clipboard.persist);
        app.editor.clipboard.history_limit = 0;
        save_app_settings(dir.path(), &app).expect("保存失败");
        assert_eq!(
            load_app_settings(dir.path()).editor.clipboard.history_limit,
            0
        );
    }

    /// 时间戳格式未知取值归一（P1-7）。
    #[test]
    fn editor_timestamp_format_normalizes_on_load() {
        let dir = data_dir();
        std::fs::write(
            app_settings_path(dir.path()),
            br#"{"schemaVersion": 8, "editor": {"insert": {"timestampFormat": "weird"}}}"#,
        )
        .expect("写测试文件失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(
            loaded.editor.insert.timestamp_format,
            crate::settings::editor::TimestampFormat::LocalDateTime
        );
    }

    /// 状态栏归一：未知/重复显示项剔除、空列表回退默认、计数模式与宽度钳制、提示截断。
    #[test]
    fn status_normalizes_on_load() {
        let dir = data_dir();
        let mut settings = AppSettings::default();
        settings.status.items = vec![
            "lineCol".into(),
            "bogus".into(),
            "lineCol".into(),
            "".into(),
            "counts".into(),
        ];
        settings.status.count_mode = crate::settings::status::CountMode::Unknown;
        settings.status.tab_width = 999;
        settings.status.empty_selection_text =
            "  这是一个超过十六个字符限制的选择提示文案  ".into();
        save_app_settings(dir.path(), &settings).expect("保存失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(
            loaded.status.items,
            vec!["lineCol".to_string(), "counts".to_string()]
        );
        assert_eq!(
            loaded.status.count_mode,
            crate::settings::status::CountMode::Grapheme
        );
        assert_eq!(loaded.status.tab_width, 128);
        assert!(loaded.status.empty_selection_text.chars().count() <= 16);
        assert!(!loaded.status.empty_selection_text.starts_with(' '));
        // 空列表回退默认组合
        let mut empty = AppSettings::default();
        empty.status.items = vec![];
        save_app_settings(dir.path(), &empty).expect("保存失败");
        let loaded = load_app_settings(dir.path());
        assert!(loaded.status.items.contains(&"lineCol".to_string()));
        assert!(loaded.status.items.len() >= 3);
    }

    /// 编辑器多光标设置归一：未知修饰键回退 Alt、上限钳制到 2。
    #[test]
    fn editor_multi_cursor_normalizes_on_load() {
        use crate::settings::editor::RectModifier;
        let dir = data_dir();
        let mut app = AppSettings::default();
        app.editor.multi_cursor.rect_modifier = RectModifier::Unknown;
        app.editor.multi_cursor.max_count = 1;
        save_app_settings(dir.path(), &app).expect("保存失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(loaded.editor.multi_cursor.rect_modifier, RectModifier::Alt);
        assert_eq!(loaded.editor.multi_cursor.max_count, 2);
        assert!(loaded.editor.multi_cursor.enabled);
    }

    /// 缺省 margins 时回退默认四向（向后兼容旧文件）。
    #[test]
    fn reader_settings_missing_margins_defaults() {
        let dir = data_dir();
        std::fs::write(
            reader_settings_path(dir.path()),
            br#"{"theme":"light","typography":{"fontFamily":"Arial","fontSize":16,"lineHeight":1.8,"contentWidth":720}}"#,
        )
        .expect("写配置失败");
        let loaded = load_reader_settings(dir.path());
        assert_eq!(loaded.typography.font_size, 16);
        assert_eq!(loaded.margins.reading.top, defaults::DEFAULT_MARGIN);
        assert_eq!(loaded.margins.reading.left, defaults::DEFAULT_MARGIN);
        assert_eq!(loaded.margins.editing.bottom, defaults::DEFAULT_MARGIN);
        assert_eq!(loaded.margins.editing.right, defaults::DEFAULT_MARGIN);
        assert_eq!(
            loaded.typography.paragraph_spacing,
            defaults::DEFAULT_PARAGRAPH_SPACING
        );
        assert_eq!(
            loaded.typography.first_line_indent,
            defaults::DEFAULT_FIRST_LINE_INDENT
        );
        assert!(loaded.typography.smooth_scroll);
    }

    /// 阅读模式设置归一：翻页方式回退、数值钳制、护眼 0 放行。
    #[test]
    fn reading_settings_normalize_on_load() {
        let dir = data_dir();
        std::fs::write(
            reader_settings_path(dir.path()),
            r#"{"schemaVersion":13,"reading":{"columns":9,"autoScrollSpeed":1,"focusMode":true,"typewriter":false,"eyeCareIntervalMin":3,"pomodoroMin":999,"readingStats":false,"progressMemory":false,"pageMode":"weird","pageAnimMs":9999}}"#,
        )
        .expect("写阅读配置失败");
        let loaded = load_reader_settings(dir.path());
        assert_eq!(loaded.reading.columns, 2);
        assert_eq!(loaded.reading.auto_scroll_speed, 5);
        assert!(loaded.reading.focus_mode);
        assert_eq!(loaded.reading.eye_care_interval_min, 5);
        assert_eq!(loaded.reading.pomodoro_min, 120);
        assert!(!loaded.reading.reading_stats);
        assert!(!loaded.reading.progress_memory);
        assert_eq!(
            loaded.reading.page_mode,
            crate::settings::reader::PageMode::Scroll
        );
        assert_eq!(loaded.reading.page_anim_ms, 2000);
    }

    /// 显示选项归一：标尺位置钳制 + 不可见标记白名单去重。
    /// 显示「折叠/大纲」字段归一（P2-6）：未知折叠回退默认、非法/超长/重复正则剔除、空列表回退内置默认。
    #[test]
    fn display_outline_and_folding_normalize() {
        use crate::settings::display::{DisplaySettings, FoldingMode};

        let mut settings = DisplaySettings::default();
        settings.folding = FoldingMode::Unknown;
        settings.outline_patterns = vec![
            "  ^第..$  ".to_string(),
            "(".to_string(),
            "".to_string(),
            "^第..$".to_string(),
        ];
        normalize_display(&mut settings);
        assert_eq!(settings.folding, FoldingMode::Off);
        assert_eq!(settings.outline_patterns, vec!["^第..$".to_string()]);

        let mut empty = DisplaySettings::default();
        empty.outline_patterns = vec!["   ".to_string(), "(".to_string()];
        normalize_display(&mut empty);
        assert_eq!(
            empty.outline_patterns.len(),
            crate::settings::defaults::DEFAULT_OUTLINE_PATTERNS.len()
        );
    }

    #[test]
    fn display_normalizes_on_load() {
        let dir = data_dir();
        let raw = r#"{ "schemaVersion": 11, "display": { "rulerPosition": 500000, "invisible": ["space", "space", "bogus"] } }"#;
        std::fs::write(app_settings_path(dir.path()), raw).expect("写入失败");
        let loaded = load_app_settings(dir.path());
        assert_eq!(loaded.display.ruler_position, 100_000);
        assert_eq!(loaded.display.invisible, vec!["space".to_string()]);
        assert!(loaded.display.word_wrap);
    }

    /// 空主题 id 回退默认（system）。
    #[test]
    fn reader_settings_empty_theme_falls_back() {
        let dir = data_dir();
        std::fs::write(reader_settings_path(dir.path()), br#"{"theme":"   "}"#)
            .expect("写配置失败");
        assert_eq!(
            load_reader_settings(dir.path()).theme_id,
            defaults::DEFAULT_THEME_ID
        );
    }

    /// 阅读配置往返一致。
    #[test]
    fn reader_settings_roundtrip() {
        let dir = data_dir();
        let mut settings = ReaderSettings::default();
        settings.theme_id = "paper-cream".to_string();
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
        reader.theme_id = "dark".to_string();
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
        assert_eq!(snapshot.reader.theme_id, "dark");
        assert_eq!(snapshot.shortcuts.bindings, effective);
    }
}
