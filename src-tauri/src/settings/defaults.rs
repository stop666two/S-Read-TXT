//! 配置默认值与取值范围（与 `docs/configuration.md` 保持同步，改动须同改文档）。

use std::collections::BTreeMap;

use crate::settings::model::LogLevel;
use crate::settings::reader::Theme;

/// 配置 schema 版本（settings/reader/shortcuts 共用一个版本号；结构变更时递增并提供迁移）。
pub const SCHEMA_VERSION: u32 = 1;

// ---------- settings.json ----------

/// 默认日志级别
pub const DEFAULT_LOG_LEVEL: LogLevel = LogLevel::Info;
/// 默认可打开文件大小上限（MB）
pub const DEFAULT_MAX_FILE_SIZE_MB: u32 = 100;
/// 文件大小上限允许范围（MB，闭区间）
pub const MAX_FILE_SIZE_MB_RANGE: (u32, u32) = (1, 2048);
/// 默认标签数量上限
pub const DEFAULT_MAX_TABS: u32 = 20;
/// 标签数量上限允许范围（闭区间）
pub const MAX_TABS_RANGE: (u32, u32) = (1, 100);
/// 默认历史保留条数
pub const DEFAULT_HISTORY_MAX_ENTRIES: u32 = 10_000;
/// 历史保留条数允许范围（闭区间）
pub const HISTORY_MAX_ENTRIES_RANGE: (u32, u32) = (1, 100_000);
/// 默认历史保留天数
pub const DEFAULT_HISTORY_RETENTION_DAYS: u32 = 365;
/// 历史保留天数允许范围（闭区间）
pub const HISTORY_RETENTION_DAYS_RANGE: (u32, u32) = (1, 3650);
/// 默认首次保存是否生成 .bak 备份
pub const DEFAULT_SAVE_BACKUP_ENABLED: bool = true;
/// 默认是否显示首启引导
pub const DEFAULT_SHOW_ONBOARDING: bool = true;

// ---------- reader.json ----------

/// 默认主题（跟随系统）
pub const DEFAULT_THEME: Theme = Theme::System;
/// 默认正文字体
pub const DEFAULT_FONT_FAMILY: &str = "Microsoft YaHei";
/// 默认正文字号（px）
pub const DEFAULT_FONT_SIZE: u32 = 16;
/// 字号允许范围（px，闭区间）
pub const FONT_SIZE_RANGE: (u32, u32) = (12, 32);
/// 默认行高倍数
pub const DEFAULT_LINE_HEIGHT: f32 = 1.8;
/// 行高倍数允许范围（闭区间）
pub const LINE_HEIGHT_RANGE: (f32, f32) = (1.2, 2.6);
/// 默认正文限宽（px）
pub const DEFAULT_CONTENT_WIDTH: u32 = 720;
/// 正文限宽允许范围（px，闭区间）
pub const CONTENT_WIDTH_RANGE: (u32, u32) = (480, 1200);
/// 默认阅读区页边距（px）
pub const DEFAULT_PAGE_PADDING: u32 = 48;
/// 页边距允许范围（px，闭区间）
pub const PAGE_PADDING_RANGE: (u32, u32) = (24, 96);

// ---------- shortcuts.json ----------

/// 默认快捷键表：动作 id（稳定 ASCII 标识）→ 组合键字符串。
/// 注：`Ctrl+1`~`9` 跳转标签为固定键，不在此表中（不参与自定义）。
pub const DEFAULT_BINDINGS: &[(&str, &str)] = &[
    ("openFile", "Ctrl+O"),
    ("save", "Ctrl+S"),
    ("saveAs", "Ctrl+Shift+S"),
    ("toggleEdit", "Ctrl+E"),
    ("closeTab", "Ctrl+W"),
    ("nextTab", "Ctrl+Tab"),
    ("prevTab", "Ctrl+Shift+Tab"),
    ("pageDown", "PgDn"),
    ("pageUp", "PgUp"),
    ("firstLine", "Home"),
    ("lastLine", "End"),
    ("fullscreen", "F11"),
    ("find", "Ctrl+F"),
    ("replace", "Ctrl+H"),
    ("historyPanel", "Ctrl+Shift+H"),
];

/// 构造默认绑定表（`BTreeMap`：序列化顺序稳定，便于文件 diff 与人工核对）。
pub fn default_bindings() -> BTreeMap<String, String> {
    DEFAULT_BINDINGS
        .iter()
        .map(|(action, combo)| ((*action).to_string(), (*combo).to_string()))
        .collect()
}

/// 判断动作 id 是否为已知动作（未知项在加载时丢弃并记录日志）。
pub fn is_known_action(action: &str) -> bool {
    DEFAULT_BINDINGS.iter().any(|(known, _)| *known == action)
}
