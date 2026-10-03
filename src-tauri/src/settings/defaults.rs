//! 配置默认值与取值范围（与 `docs/configuration.md` 保持同步，改动须同改文档）。

use std::collections::BTreeMap;

use crate::settings::editor::{
    LineCaseMode, LineDedupeMode, LineIndentStyle, LineScopeKind, LineSortMode, RectModifier,
};
use crate::settings::model::{Language, LogLevel};
use crate::settings::reader::{BackgroundFill, TextAlign};

/// 配置 schema 版本（settings/reader/shortcuts 共用一个版本号；结构变更时递增并提供迁移）。
/// v2：新增 `hardLimitMB` / `startup` / `statusBar` / 排版扩展等字段（字段补齐式迁移，见 `settings::migrate`）。
/// v3：主题升级为 id 体系（旧值 `eye` 迁移为 `paper-cream`；新增主题动画字段）。
pub const SCHEMA_VERSION: u32 = 6;

// ---------- settings.json ----------

/// 默认日志级别
pub const DEFAULT_LOG_LEVEL: LogLevel = LogLevel::Info;
/// 默认界面语言（BCP 47：简体中文；允许值见注册表 `app.locale`）
pub const DEFAULT_LOCALE: Language = Language::ZhCn;
/// 默认可打开文件大小上限（MB）
pub const DEFAULT_MAX_FILE_SIZE_MB: u32 = 100;
/// 文件大小上限允许范围（MB，闭区间）
pub const MAX_FILE_SIZE_MB_RANGE: (u32, u32) = (1, 2048);
/// 默认只读阈值（MB）：超过此大小以只读模式打开（可浏览、不可编辑）
pub const DEFAULT_READ_ONLY_THRESHOLD_MB: u32 = 100;
/// 只读阈值允许范围（MB，闭区间）
pub const READ_ONLY_THRESHOLD_MB_RANGE: (u32, u32) = (1, 2048);
/// 默认硬上限（MB）：超过此大小拒绝打开（比只读阈值更宽的绝对上限）
pub const DEFAULT_HARD_LIMIT_MB: u32 = 2048;
/// 硬上限允许范围（MB，闭区间）
pub const HARD_LIMIT_MB_RANGE: (u32, u32) = (100, 16384);
/// 默认标签数量上限
pub const DEFAULT_MAX_TABS: u32 = 20;
/// 标签数量上限允许范围（闭区间）
pub const MAX_TABS_RANGE: (u32, u32) = (1, 200);
/// 默认历史保留条数
pub const DEFAULT_HISTORY_MAX_ENTRIES: u32 = 10_000;
/// 历史保留条数允许范围（闭区间）
pub const HISTORY_MAX_ENTRIES_RANGE: (u32, u32) = (100, 1_000_000);
/// 默认历史保留天数
pub const DEFAULT_HISTORY_RETENTION_DAYS: u32 = 365;
/// 历史保留天数允许范围（闭区间）
pub const HISTORY_RETENTION_DAYS_RANGE: (u32, u32) = (1, 36500);
/// 默认首次保存是否生成 .bak 备份
pub const DEFAULT_SAVE_BACKUP_ENABLED: bool = true;
/// 默认是否显示首启引导
pub const DEFAULT_SHOW_ONBOARDING: bool = true;

// ---------- settings.json / editor.clipboard（P1-5） ----------

/// 默认剪贴板历史上限（条；0 = 禁用）
pub const DEFAULT_CLIPBOARD_HISTORY_LIMIT: u32 = 200;
/// 剪贴板历史上限允许范围（闭区间；0 = 禁用）
pub const CLIPBOARD_HISTORY_LIMIT_RANGE: (u32, u32) = (0, 5000);
/// 默认是否持久化剪贴板历史到 `data/clipboard-history.json`
pub const DEFAULT_CLIPBOARD_PERSIST: bool = true;

// ---------- reader.json ----------

/// 默认主题 id（跟随系统）
pub const DEFAULT_THEME_ID: &str = "system";
/// 默认启用主题切换过渡动画
pub const DEFAULT_THEME_ANIM_ENABLED: bool = true;
/// 默认主题过渡时长（ms）
pub const DEFAULT_THEME_ANIM_MS: u32 = 200;
/// 主题过渡时长允许范围（ms，闭区间）
pub const THEME_ANIM_MS_RANGE: (u32, u32) = (0, 1000);

// ---------- 背景图（T-04～T-08） ----------

/// 默认不启用背景图
pub const DEFAULT_BACKGROUND_ENABLED: bool = false;
/// 默认背景图不透明度（%）
pub const DEFAULT_BACKGROUND_OPACITY: u32 = 40;
/// 背景图不透明度允许范围（%，闭区间）
pub const BACKGROUND_OPACITY_RANGE: (u32, u32) = (0, 100);
/// 默认背景图填充模式
pub const DEFAULT_BACKGROUND_FILL: BackgroundFill = BackgroundFill::Cover;
/// 默认背景图模糊半径（px）
pub const DEFAULT_BACKGROUND_BLUR: u32 = 0;
/// 背景图模糊半径允许范围（px，闭区间）
pub const BACKGROUND_BLUR_RANGE: (u32, u32) = (0, 40);
/// 默认背景图亮度调整（%）
pub const DEFAULT_BACKGROUND_DIM: i32 = 0;
/// 背景图亮度调整允许范围（%，闭区间；负=暗化，正=亮化）
pub const BACKGROUND_DIM_RANGE: (i32, i32) = (-50, 50);
/// 背景图存储文件名最大长度（字符；注册表校验用）
pub const BACKGROUND_FILE_MAX_CHARS: u32 = 200;
/// 默认正文字体
pub const DEFAULT_FONT_FAMILY: &str = "Microsoft YaHei";
/// 默认正文字号（px）
pub const DEFAULT_FONT_SIZE: u32 = 16;
/// 字号允许范围（px，闭区间）
pub const FONT_SIZE_RANGE: (u32, u32) = (8, 72);
/// 默认行高倍数
pub const DEFAULT_LINE_HEIGHT: f32 = 1.8;
/// 行高倍数允许范围（闭区间）
pub const LINE_HEIGHT_RANGE: (f32, f32) = (1.0, 3.2);
/// 默认正文限宽（px）
pub const DEFAULT_CONTENT_WIDTH: u32 = 720;
/// 正文限宽允许范围（px，闭区间）
pub const CONTENT_WIDTH_RANGE: (u32, u32) = (320, 2400);
/// 默认阅读区左右页边距（px）
pub const DEFAULT_PAGE_PADDING: u32 = 48;
/// 左右页边距允许范围（px，闭区间）
pub const PAGE_PADDING_RANGE: (u32, u32) = (0, 240);
/// 默认阅读区上下留白（px）
pub const DEFAULT_PAGE_PADDING_Y: u32 = 48;
/// 阅读区上下留白允许范围（px，闭区间）
pub const PAGE_PADDING_Y_RANGE: (u32, u32) = (0, 240);
/// 默认段间距（px；0 表示无额外间距）
pub const DEFAULT_PARAGRAPH_SPACING: u32 = 0;
/// 段间距允许范围（px，闭区间）
pub const PARAGRAPH_SPACING_RANGE: (u32, u32) = (0, 64);
/// 默认首行缩进（字符数；0 表示不缩进）
pub const DEFAULT_FIRST_LINE_INDENT: u32 = 0;
/// 首行缩进允许范围（字符数，闭区间）
pub const FIRST_LINE_INDENT_RANGE: (u32, u32) = (0, 8);
/// 默认文字对齐
pub const DEFAULT_TEXT_ALIGN: TextAlign = TextAlign::Left;
/// 默认翻页平滑滚动
pub const DEFAULT_SMOOTH_SCROLL: bool = true;

// ---------- 状态栏与启动行为 ----------

/// 状态栏默认显示文件名与进度
pub const DEFAULT_STATUS_BAR_SHOW_FILE: bool = true;
/// 状态栏默认显示阅读百分比
pub const DEFAULT_STATUS_BAR_SHOW_PERCENT: bool = true;
/// 状态栏默认显示文件大小
pub const DEFAULT_STATUS_BAR_SHOW_SIZE: bool = true;
/// 状态栏默认显示编码切换
pub const DEFAULT_STATUS_BAR_SHOW_ENCODING: bool = true;
/// 启动默认恢复上次会话（标签）
pub const DEFAULT_STARTUP_RESTORE_SESSION: bool = true;
/// 启动默认恢复窗口位置与大小
pub const DEFAULT_STARTUP_RESTORE_WINDOW: bool = true;

// ---------- settings.json / 编辑器行操作默认值 ----------

/// 行操作默认作用范围（全文）
pub const DEFAULT_LINE_SCOPE: LineScopeKind = LineScopeKind::All;
/// 默认排序方式（字典序）
pub const DEFAULT_LINE_SORT_MODE: LineSortMode = LineSortMode::Lex;
/// 默认去重规则（保留首次）
pub const DEFAULT_LINE_DEDUPE_MODE: LineDedupeMode = LineDedupeMode::KeepFirst;
/// 去重默认忽略大小写
pub const DEFAULT_LINE_DEDUPE_IGNORE_CASE: bool = false;
/// 去重默认模糊匹配（NFKC + 忽略空白）
pub const DEFAULT_LINE_DEDUPE_FUZZY: bool = false;
/// 默认缩进宽度（空格数）
pub const DEFAULT_LINE_INDENT_WIDTH: u32 = 4;
/// 缩进宽度允许范围（闭区间）
pub const LINE_INDENT_WIDTH_RANGE: (u32, u32) = (1, 16);
/// 默认缩进字符（空格）
pub const DEFAULT_LINE_INDENT_STYLE: LineIndentStyle = LineIndentStyle::Spaces;
/// 默认大小写转换模式（小写）
pub const DEFAULT_LINE_CASE_MODE: LineCaseMode = LineCaseMode::Lower;
/// 默认列编辑分隔符（制表符）
pub const DEFAULT_LINE_COLUMN_DELIMITER: &str = "\t";
/// 列分隔符最大字符数（超长回退默认）
pub const LINE_COLUMN_DELIMITER_MAX_CHARS: usize = 16;
/// 行操作默认开启操作预览
pub const DEFAULT_LINE_PREVIEW: bool = true;
/// 行操作默认跳过空行
pub const DEFAULT_LINE_SKIP_EMPTY: bool = false;
/// 默认启用多光标与矩形选择
pub const DEFAULT_MULTI_CURSOR_ENABLED: bool = true;
/// 默认矩形选择修饰键（Alt）
pub const DEFAULT_MULTI_CURSOR_RECT_MODIFIER: RectModifier = RectModifier::Alt;
/// 默认多光标数量上限
pub const DEFAULT_MULTI_CURSOR_MAX_COUNT: u32 = 1000;
/// 多光标数量上限允许范围（闭区间）
pub const MULTI_CURSOR_MAX_COUNT_RANGE: (u32, u32) = (2, 10_000);

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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{default_bindings, is_known_action, DEFAULT_BINDINGS};

    /// 默认表不变量：数量固定、动作唯一、组合键唯一且格式规范
    /// （快捷键引擎与录制校验依赖这些前提）。
    #[test]
    fn default_bindings_are_unique_and_wellformed() {
        assert_eq!(
            DEFAULT_BINDINGS.len(),
            15,
            "默认动作数量变化须同步引擎与文档"
        );
        let mut actions: BTreeSet<&str> = BTreeSet::new();
        let mut combos: BTreeSet<String> = BTreeSet::new();
        for (action, combo) in DEFAULT_BINDINGS {
            assert!(actions.insert(action), "动作 id 重复：{action}");
            assert!(is_known_action(action));
            assert!(!combo.trim().is_empty(), "空绑定：{action}");
            assert!(
                combos.insert(combo.to_lowercase()),
                "组合键重复（大小写不敏感）：{combo}"
            );
            let parts: Vec<&str> = combo.split('+').collect();
            assert!(
                parts.iter().all(|part| !part.is_empty()),
                "格式错误：{combo}"
            );
            let base = parts.last().expect("至少一个按键");
            assert!(is_canonical_base_key(base), "主键名不合法：{combo}");
            for modifier in &parts[..parts.len() - 1] {
                assert!(
                    matches!(*modifier, "Ctrl" | "Shift" | "Alt"),
                    "未知修饰键 {modifier}（{combo}）"
                );
            }
        }
        assert_eq!(default_bindings().len(), DEFAULT_BINDINGS.len());
    }

    /// 主键名是否与前端 `keys.ts` 的规范名一致（单字符大写字母/数字、命名键、F1~F24）。
    fn is_canonical_base_key(base: &str) -> bool {
        const NAMED: [&str; 14] = [
            "Tab",
            "PgUp",
            "PgDn",
            "Home",
            "End",
            "Insert",
            "Delete",
            "Backspace",
            "Enter",
            "Space",
            "Esc",
            "Up",
            "Down",
            "Left",
        ];
        if NAMED.contains(&base) {
            return true;
        }
        if base.len() == 1 {
            return base
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
        }
        base.strip_prefix('F')
            .and_then(|num| num.parse::<u8>().ok())
            .is_some_and(|num| (1..=24).contains(&num))
    }
}
