//! 配置默认值与取值范围（与 `docs/configuration.md` 保持同步，改动须同改文档）。

use std::collections::BTreeMap;

use crate::settings::display::FoldingMode;
use crate::settings::editor::{
    LineCaseMode, LineDedupeMode, LineIndentStyle, LineScopeKind, LineSortMode, RectModifier,
    TimestampFormat,
};
use crate::settings::file::NewEol;
use crate::settings::model::{FindScope, Language, LogLevel};
use crate::settings::reader::{BackgroundFill, PageMode, TextAlign};
use crate::settings::status::CountMode;

/// 配置 schema 版本（settings/reader/shortcuts 共用一个版本号；结构变更时递增并提供迁移）。
/// v2：新增 `hardLimitMB` / `startup` / `statusBar` / 排版扩展等字段（字段补齐式迁移，见 `settings::migrate`）。
/// v3：主题升级为 id 体系（旧值 `eye` 迁移为 `paper-cream`；新增主题动画字段）。
/// v4–v6：编辑器行操作/多光标/剪贴板设置节（字段补齐式）。
/// v7：新增查找与正则设置节（find/regex）。
/// v8：新增辅助编辑设置节（editor.insert / autoPairs / cleanup，P1-7）。
/// v9：新增查找多文件开关与并发数（find.multifile*，P1-8）。
/// v10：新增状态栏显示与交互设置节（status，P2-1）。
/// v11：新增显示选项设置节（display，P2-2）。
/// v12：页边距改四向「阅读/编辑两套」（typography.pagePadding → margins，S4）。
/// v13：新增阅读模式设置节（reading，P2-4）。
/// v14：新增显示折叠/大纲/面包屑字段。
pub const SCHEMA_VERSION: u32 = 15;

// ---------- settings.json ----------

/// 默认日志级别
pub const DEFAULT_LOG_LEVEL: LogLevel = LogLevel::Info;
/// 默认界面语言（BCP 47：简体中文；允许值见注册表 `app.locale`）
pub const DEFAULT_LOCALE: Language = Language::ZhCn;
/// 默认可打开文件大小上限（MB）
pub const DEFAULT_MAX_FILE_SIZE_MB: u32 = 100;
/// 文件大小上限允许范围（MB，闭区间）
pub const MAX_FILE_SIZE_MB_RANGE: (u32, u32) = (1, 65536);
/// 默认只读阈值（MB）：超过此大小以只读模式打开（可浏览、不可编辑）
pub const DEFAULT_READ_ONLY_THRESHOLD_MB: u32 = 100;
/// 只读阈值允许范围（MB，闭区间）
pub const READ_ONLY_THRESHOLD_MB_RANGE: (u32, u32) = (1, 65536);
/// 默认硬上限（MB）：超过此大小拒绝打开（比只读阈值更宽的绝对上限）
pub const DEFAULT_HARD_LIMIT_MB: u32 = 2048;
/// 硬上限允许范围（MB，闭区间）
pub const HARD_LIMIT_MB_RANGE: (u32, u32) = (100, 1_048_576);
/// 默认标签数量上限
pub const DEFAULT_MAX_TABS: u32 = 20;
/// 标签数量上限允许范围（闭区间）
pub const MAX_TABS_RANGE: (u32, u32) = (1, 2000);
/// 默认历史保留条数
pub const DEFAULT_HISTORY_MAX_ENTRIES: u32 = 10_000;
/// 历史保留条数允许范围（闭区间）
pub const HISTORY_MAX_ENTRIES_RANGE: (u32, u32) = (100, 10_000_000);
/// 默认历史保留天数
pub const DEFAULT_HISTORY_RETENTION_DAYS: u32 = 365;
/// 历史保留天数允许范围（闭区间）
pub const HISTORY_RETENTION_DAYS_RANGE: (u32, u32) = (1, 365000);
/// 默认首次保存是否生成 .bak 备份
pub const DEFAULT_SAVE_BACKUP_ENABLED: bool = true;
/// 默认是否显示首启引导
pub const DEFAULT_SHOW_ONBOARDING: bool = true;

// ---------- settings.json / editor.clipboard ----------

/// 默认剪贴板历史上限（条；0 = 禁用）
pub const DEFAULT_CLIPBOARD_HISTORY_LIMIT: u32 = 200;
/// 剪贴板历史上限允许范围（闭区间；0 = 禁用）
pub const CLIPBOARD_HISTORY_LIMIT_RANGE: (u32, u32) = (0, 200_000);
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
pub const THEME_ANIM_MS_RANGE: (u32, u32) = (0, 10_000);

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
pub const BACKGROUND_BLUR_RANGE: (u32, u32) = (0, 500);
/// 默认背景图亮度调整（%）
pub const DEFAULT_BACKGROUND_DIM: i32 = 0;
/// 背景图亮度调整允许范围（%，闭区间；负=暗化，正=亮化）
pub const BACKGROUND_DIM_RANGE: (i32, i32) = (-100, 200);
/// 背景图存储文件名最大长度（字符；注册表校验用）
pub const BACKGROUND_FILE_MAX_CHARS: u32 = 200;
/// 默认正文字体
pub const DEFAULT_FONT_FAMILY: &str = "Microsoft YaHei";
/// 默认正文字号（px）
pub const DEFAULT_FONT_SIZE: u32 = 16;
/// 字号允许范围（px，闭区间）
pub const FONT_SIZE_RANGE: (u32, u32) = (6, 512);
/// 默认行高倍数
pub const DEFAULT_LINE_HEIGHT: f32 = 1.8;
/// 行高倍数允许范围（闭区间）
pub const LINE_HEIGHT_RANGE: (f32, f32) = (0.5, 5.0);
/// 默认正文限宽（px）
pub const DEFAULT_CONTENT_WIDTH: u32 = 720;
/// 正文限宽允许范围（px，闭区间）
pub const CONTENT_WIDTH_RANGE: (u32, u32) = (160, 20000);
/// 默认页边距（px；阅读与编辑两套默认一致）
pub const DEFAULT_MARGIN: u32 = 48;
/// 页边距允许范围（px，闭区间）
pub const MARGIN_RANGE: (u32, u32) = (0, 2000);

// ---------- reader.json · reading（阅读模式；P2-4） ----------

/// 默认分栏数（1 = 单栏）
pub const DEFAULT_READING_COLUMNS: u32 = 1;
/// 分栏数允许范围（闭区间）
pub const READING_COLUMNS_RANGE: (u32, u32) = (1, 2);
/// 默认自动滚动速度（px/s）
pub const DEFAULT_AUTO_SCROLL_SPEED: u32 = 30;
/// 自动滚动速度允许范围（px/s，闭区间）
pub const AUTO_SCROLL_SPEED_RANGE: (u32, u32) = (5, 300);
/// 默认专注模式
pub const DEFAULT_FOCUS_MODE: bool = false;
/// 默认打字机模式
pub const DEFAULT_TYPEWRITER: bool = false;
/// 默认护眼提醒间隔（分钟；0 = 关闭）
pub const DEFAULT_EYE_CARE_INTERVAL_MIN: u32 = 0;
/// 护眼提醒间隔允许范围（分钟，闭区间；0 单独放行表示关闭）
pub const EYE_CARE_INTERVAL_RANGE: (u32, u32) = (5, 240);
/// 默认番茄钟时长（分钟）
pub const DEFAULT_POMODORO_MIN: u32 = 25;
/// 番茄钟时长允许范围（分钟，闭区间）
pub const POMODORO_MIN_RANGE: (u32, u32) = (5, 120);
/// 默认开启阅读时长统计
pub const DEFAULT_READING_STATS: bool = true;
/// 默认开启阅读进度记忆
pub const DEFAULT_PROGRESS_MEMORY: bool = true;
/// 默认翻页方式
pub const DEFAULT_PAGE_MODE: PageMode = PageMode::Scroll;
/// 默认翻页动画时长（ms）
pub const DEFAULT_PAGE_ANIM_MS: u32 = 320;
/// 翻页动画时长允许范围（ms，闭区间；0 = 无动画）
pub const PAGE_ANIM_MS_RANGE: (u32, u32) = (0, 2000);
/// 默认段间距（px；0 表示无额外间距）
pub const DEFAULT_PARAGRAPH_SPACING: u32 = 0;
/// 段间距允许范围（px，闭区间）
pub const PARAGRAPH_SPACING_RANGE: (u32, u32) = (0, 2000);
/// 默认首行缩进（字符数；0 表示不缩进）
pub const DEFAULT_FIRST_LINE_INDENT: u32 = 0;
/// 首行缩进允许范围（字符数，闭区间）
pub const FIRST_LINE_INDENT_RANGE: (u32, u32) = (0, 200);
/// 默认文字对齐
pub const DEFAULT_TEXT_ALIGN: TextAlign = TextAlign::Left;
/// 默认翻页平滑滚动
pub const DEFAULT_SMOOTH_SCROLL: bool = true;

// ---------- 启动行为 ----------

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
pub const LINE_INDENT_WIDTH_RANGE: (u32, u32) = (1, 128);
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
pub const MULTI_CURSOR_MAX_COUNT_RANGE: (u32, u32) = (2, 100_000);

/// 默认时间戳插入格式（本地日期时间，`YYYY-MM-DD HH:mm:ss`）
pub const DEFAULT_TIMESTAMP_FORMAT: TimestampFormat = TimestampFormat::LocalDateTime;
/// 默认启用括号匹配/自动缩进总开关
pub const DEFAULT_AUTO_PAIRS_ENABLED: bool = true;
/// 默认启用自动补对（选区包裹、右符号跳过、空对退格）
pub const DEFAULT_AUTO_PAIRS_AUTO_CLOSE: bool = true;
/// 默认启用回车自动继承缩进
pub const DEFAULT_AUTO_PAIRS_AUTO_INDENT: bool = true;
/// 默认启用括号配对高亮
pub const DEFAULT_AUTO_PAIRS_HIGHLIGHT_MATCH: bool = true;
/// 默认「一键清理」包含：删除行尾空白
pub const DEFAULT_CLEANUP_TRAILING_WHITESPACE: bool = true;
/// 默认「一键清理」包含：合并重复空行
pub const DEFAULT_CLEANUP_COLLAPSE_BLANK_LINES: bool = true;
/// 默认「一键清理」包含：统一末尾换行
pub const DEFAULT_CLEANUP_TRAILING_NEWLINE: bool = true;

// ---------- settings.json / 查找与正则 ----------

/// 默认区分大小写（F-01）
pub const DEFAULT_FIND_CASE_SENSITIVE: bool = false;
/// 默认全词匹配（F-02；仅字面模式生效）
pub const DEFAULT_FIND_WHOLE_WORD: bool = false;
/// 默认循环查找（F-05）
pub const DEFAULT_FIND_WRAP_AROUND: bool = true;
/// 默认高亮全部匹配（F-06）
pub const DEFAULT_FIND_HIGHLIGHT_ALL: bool = true;
/// 默认显示匹配计数（F-07）
pub const DEFAULT_FIND_MATCH_COUNT: bool = true;
/// 默认替换预览（F-08）
pub const DEFAULT_FIND_REPLACE_PREVIEW: bool = true;
/// 默认查找范围（F-09）
pub const DEFAULT_FIND_SCOPE: FindScope = FindScope::Document;
/// 默认查找历史条数（F-10）
pub const DEFAULT_FIND_HISTORY_LIMIT: u32 = 50;
/// 查找历史条数范围（闭区间）
pub const FIND_HISTORY_LIMIT_RANGE: (u32, u32) = (0, 100_000);
/// 默认是否启用多文件（工作区）搜索
pub const DEFAULT_FIND_MULTIFILE_ENABLED: bool = true;
/// 默认多文件搜索并发数（同时扫描的只读标签数）
pub const DEFAULT_FIND_MULTIFILE_CONCURRENCY: u32 = 4;
/// 多文件搜索并发数允许范围（闭区间）
pub const FIND_MULTIFILE_CONCURRENCY_RANGE: (u32, u32) = (1, 128);
/// 默认高亮颜色（空串 = 跟随主题内置色；F-11）
pub const DEFAULT_FIND_HIGHLIGHT_COLOR: &str = "";
/// 默认正则超时（毫秒；F-03）
pub const DEFAULT_REGEX_TIMEOUT_MS: u32 = 500;
/// 正则超时范围（毫秒，闭区间）
pub const REGEX_TIMEOUT_MS_RANGE: (u32, u32) = (10, 600_000);
/// 正则库条目上限（F-14）
pub const REGEX_LIBRARY_MAX_ITEMS: u32 = 200;
/// 正则库单条最大字符数（F-14）
pub const REGEX_LIBRARY_MAX_CHARS: u32 = 512;

// ---------- settings.json：状态栏 ----------

/// 状态栏默认显示项与顺序（id 白名单见 [`STATUS_ITEM_IDS`]）
pub const DEFAULT_STATUS_ITEMS: &[&str] = &[
    "lineCol", "counts", "progress", "size", "encoding", "eol", "modified",
];
/// 状态栏显示项可选 id（顺序即设置界面选项顺序）
pub const STATUS_ITEM_IDS: &[&str] = &[
    "lineCol", "counts", "words", "progress", "size", "encoding", "eol", "modified", "readTime",
];
/// 状态栏显示项数量上限
pub const STATUS_ITEMS_MAX: u32 = 16;
/// 默认计数模式（字素簇）
pub const DEFAULT_STATUS_COUNT_MODE: CountMode = CountMode::Grapheme;
/// 默认 Tab 显示宽度
pub const DEFAULT_STATUS_TAB_WIDTH: u32 = 4;
/// Tab 显示宽度范围（闭区间）
pub const STATUS_TAB_WIDTH_RANGE: (u32, u32) = (1, 128);
/// 默认未选择提示
pub const DEFAULT_STATUS_EMPTY_SELECTION: &str = "未选择";
/// 未选择提示最大字符数
pub const STATUS_EMPTY_SELECTION_MAX_CHARS: usize = 16;

// ---------- 显示选项（display，P2-2） ----------

/// 显示行号默认关
pub const DEFAULT_DISPLAY_LINE_NUMBERS: bool = false;
/// 相对行号默认关
pub const DEFAULT_DISPLAY_RELATIVE_LINE_NUMBERS: bool = false;
/// 高亮当前行默认开
pub const DEFAULT_DISPLAY_HIGHLIGHT_CURRENT_LINE: bool = true;
/// 自动换行默认开
pub const DEFAULT_DISPLAY_WORD_WRAP: bool = true;
/// 标尺默认关
pub const DEFAULT_DISPLAY_RULER: bool = false;
/// 标尺默认位置（px，相对正文列左缘）
pub const DEFAULT_DISPLAY_RULER_POSITION: u32 = 80;
/// 标尺位置允许范围（px，闭区间）
pub const DISPLAY_RULER_POSITION_RANGE: (u32, u32) = (0, 100_000);
/// 缩进参考线默认关
pub const DEFAULT_DISPLAY_INDENT_GUIDES: bool = false;
/// 不可见字符标记白名单（id 列表）
pub const DISPLAY_INVISIBLE_IDS: &[&str] = &["space", "tab", "newline", "trailingSpace"];
/// 滚动条标记默认开
pub const DEFAULT_DISPLAY_SCROLLBAR_MARKERS: bool = true;
/// 默认折叠方式（关闭）
pub const DEFAULT_DISPLAY_FOLDING: FoldingMode = FoldingMode::Off;
/// 默认是否启用大纲面板
pub const DEFAULT_DISPLAY_OUTLINE: bool = true;
/// 默认是否显示面包屑（V-10）
pub const DEFAULT_DISPLAY_BREADCRUMB: bool = true;
/// 大纲正则条目数量上限
pub const OUTLINE_PATTERNS_MAX_ITEMS: u32 = 32;
/// 大纲正则单条字符数上限
pub const OUTLINE_PATTERN_MAX_CHARS: u32 = 200;
/// 内置章节正则（「按标题」折叠与大纲默认规则；可被 `display.outlinePatterns` 覆盖）
pub const DEFAULT_OUTLINE_PATTERNS: &[&str] = &[
    r"^\s*第[0-9零一二三四五六七八九十百千万]+[章节回卷部篇]",
    r"^\s*(?:Chapter|CHAPTER|Part|PART)\s+(?:[0-9]+|[IVXLCDM]+)\b",
];

// ---------- settings.json / file（文件与快照；P3-1） ----------

/// 新建文件默认编码（下拉取值见文档；非法回退此项）
pub const DEFAULT_FILE_NEW_ENCODING: &str = "UTF-8";
/// 新建文件默认换行
pub const DEFAULT_FILE_NEW_EOL: NewEol = NewEol::Lf;
/// 默认自动保存快照间隔（秒）
pub const DEFAULT_FILE_AUTOSAVE_INTERVAL_SEC: u32 = 30;
/// 自动保存间隔允许范围（秒，闭区间）
pub const FILE_AUTOSAVE_INTERVAL_RANGE: (u32, u32) = (5, 600);
/// 默认是否将自动保存写回用户文件（D-06：默认关闭）
pub const DEFAULT_FILE_AUTOSAVE_WRITE_BACK: bool = false;
/// 默认快照保留份数
pub const DEFAULT_FILE_SNAPSHOT_KEEP: u32 = 50;
/// 快照保留份数允许范围（闭区间）
pub const FILE_SNAPSHOT_KEEP_RANGE: (u32, u32) = (1, 1000);
/// 默认快照总容量上限（MB）
pub const DEFAULT_FILE_SNAPSHOT_MAX_MB: u32 = 200;
/// 快照总容量上限允许范围（MB，闭区间）
pub const FILE_SNAPSHOT_MAX_MB_RANGE: (u32, u32) = (10, 4096);
/// 默认版本历史开关
pub const DEFAULT_FILE_VERSION_HISTORY: bool = true;
/// 默认文件关联扩展名
pub const DEFAULT_FILE_ASSOCIATIONS: &[&str] = &[".txt"];
/// 文件关联扩展名条数上限
pub const FILE_ASSOCIATIONS_MAX_ITEMS: u32 = 32;
/// 单条扩展名长度上限（字符）
pub const FILE_ASSOCIATION_MAX_CHARS: u32 = 16;
/// 默认「最近打开」显示条数
pub const DEFAULT_FILE_RECENT_LIMIT: u32 = 20;
/// 「最近打开」显示条数允许范围（闭区间；0 = 不显示）
pub const FILE_RECENT_LIMIT_RANGE: (u32, u32) = (0, 200);

// ---------- shortcuts.json ----------

/// 默认快捷键表：动作 id（稳定 ASCII 标识）→ 组合键字符串。
/// 注：`Ctrl+1`~`9` 跳转标签为固定键，不在此表中（不参与自定义）。
pub const DEFAULT_BINDINGS: &[(&str, &str)] = &[
    ("openFile", "Ctrl+O"),
    ("newWindow", "Ctrl+Shift+N"),
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
            16,
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
