//! `session.json` 模型（会话：多窗口几何 + 各窗口分栏布局与标签锚点）。
//! 字段以 `docs/configuration.md` §2.4 为准（保持同步）。
//!
//! 语义：退出时写入、启动时读取；标签**惰性恢复**（先出标签栏，
//! 仅激活标签建立索引）；未保存的编辑内容一律不持久化。

use serde::{Deserialize, Serialize};

/// 默认窗口宽度（px；与 `tauri.conf.json` 的 `width` 保持一致）
pub const DEFAULT_WINDOW_WIDTH: u32 = 1100;
/// 默认窗口高度（px；与 `tauri.conf.json` 的 `height` 保持一致）
pub const DEFAULT_WINDOW_HEIGHT: u32 = 760;
/// 窗口最小宽度（px；与 `tauri.conf.json` 的 `minWidth` 一致）
pub const MIN_WINDOW_WIDTH: u32 = 720;
/// 窗口最小高度（px；与 `tauri.conf.json` 的 `minHeight` 一致）
pub const MIN_WINDOW_HEIGHT: u32 = 480;
/// 窗口尺寸上限（px；防御手改配置导致窗口不可用的异常值）
pub const MAX_WINDOW_DIMENSION: u32 = 16384;
/// 会话载入的绝对安全上限（叶栏位数量持久化校验；用户可调上限见设置 `app.maxPanes`，范围 1–16）
pub const MAX_PANES_ABSOLUTE: usize = 16;
/// 会话配置格式版本（独立于设置 schema：会话结构变更时递增并在会话模块内提供迁移）
pub const SESSION_SCHEMA_VERSION: u32 = 4;

/// 单标签折叠锚点条数上限（防御脏数据；用户可调上限见设置 `app.startup.maxSessionFolds`，0 = 不恢复）
pub const MAX_SESSION_FOLDS_ABSOLUTE: usize = 100_000;

/// 窗口状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowState {
    /// 窗口左上角 X 坐标（`None` = 居中）
    pub x: Option<i32>,
    /// 窗口左上角 Y 坐标（`None` = 居中）
    pub y: Option<i32>,
    /// 窗口宽度（px）
    pub width: u32,
    /// 窗口高度（px）
    pub height: u32,
    /// 启动时是否最大化
    pub maximized: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            width: DEFAULT_WINDOW_WIDTH,
            height: DEFAULT_WINDOW_HEIGHT,
            maximized: false,
        }
    }
}

/// 单个折叠区域的锚点（起始显示行 + 行数；恢复时与重算区间逐项校验）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldSpan {
    /// 折叠头部行（0 基）
    pub start_row: u64,
    /// 折叠区域行数（含头部行）
    pub len: u64,
}

/// 单个标签的会话锚点（不持久化未保存的编辑内容）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionTab {
    /// 文件绝对路径（文件不存在时启动跳过并提示）
    pub path: String,
    /// 手动选择的编码（`None` = 自动检测）
    pub encoding: Option<String>,
    /// 滚动锚点（显示行号，从 0 开始）
    pub scroll_row: u64,
    /// 上次是否处于编辑态（仅恢复界面模式）
    pub edit_mode: bool,
    /// 标签颜色（调色板 id；`None` = 未设置）
    pub color: Option<String>,
    /// 编辑标签光标行（0 基；`None` = 未记录）
    pub caret_row: Option<u64>,
    /// 编辑标签光标行内 UTF-16 偏移（0 基；`None` = 未记录）
    pub caret_col: Option<u64>,
    /// 折叠区域锚点（恢复时与重算区间校验，不匹配丢弃）
    pub folds: Vec<FoldSpan>,
}

impl Default for SessionTab {
    fn default() -> Self {
        Self {
            path: String::new(),
            encoding: None,
            scroll_row: 0,
            edit_mode: false,
            color: None,
            caret_row: None,
            caret_col: None,
            folds: Vec::new(),
        }
    }
}

/// 分栏方向（`row` = 左右并排；`column` = 上下堆叠）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PaneSplitDir {
    /// 子栏横向并排（左右）
    Row,
    /// 子栏纵向堆叠（上下）
    Column,
}

/// 分栏布局树：叶子引用栏位键；`Split` 的 `sizes` 与 `children` 等长且合计归一为 1。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PaneLayout {
    /// 叶：一个栏位
    Leaf {
        /// 栏位键（`窗口标签#栏号`）
        pane: String,
    },
    /// 分支：按 `dir` 排列的多个子布局
    Split {
        /// 排列方向
        dir: PaneSplitDir,
        /// 各子布局占比（与 `children` 等长；合计 1）
        sizes: Vec<f64>,
        /// 子布局（至少 2 个）
        children: Vec<PaneLayout>,
    },
}

/// 单个栏位的会话（标签锚点 + 活动标签）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PaneSession {
    /// 栏位键（`窗口标签#栏号`）
    pub pane: String,
    /// 活动标签下标（归一后保证落在 `tabs` 范围内）
    pub active_tab_index: u32,
    /// 上次打开的标签列表（惰性恢复）
    pub tabs: Vec<SessionTab>,
}

impl Default for PaneSession {
    fn default() -> Self {
        Self {
            pane: String::new(),
            active_tab_index: 0,
            tabs: Vec::new(),
        }
    }
}

/// 单个窗口的会话（几何 + 分栏布局 + 各栏标签锚点 + 聚焦栏）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowSession {
    /// 窗口 label（`main` / `main-2`…；保存时由后端按调用窗口覆写）
    pub label: String,
    /// 窗口状态
    pub window: WindowState,
    /// 各栏位的标签锚点（v3 起；顺序即栏位键顺序）
    pub panes: Vec<PaneSession>,
    /// 分栏布局树（v3 起；叶集合应与 `panes` 一致）
    pub layout: Option<PaneLayout>,
    /// 最后聚焦的栏位键（启动激活；`None` = 第一个栏位）
    pub focused_pane: Option<String>,
    /// v2 兼容字段：单栏活动下标（读取旧文件时迁移，写出时省略）
    #[serde(skip_serializing, rename = "activeTabIndex")]
    pub legacy_active_tab_index: u32,
    /// v2 兼容字段：单栏标签（读取旧文件时迁移，写出时省略）
    #[serde(skip_serializing, rename = "tabs")]
    pub legacy_tabs: Vec<SessionTab>,
}

impl Default for WindowSession {
    fn default() -> Self {
        Self {
            label: String::new(),
            window: WindowState::default(),
            panes: Vec::new(),
            layout: None,
            focused_pane: None,
            legacy_active_tab_index: 0,
            legacy_tabs: Vec::new(),
        }
    }
}

/// 会话（`session.json`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionState {
    /// 会话格式版本（保存时写入当前 [`SESSION_SCHEMA_VERSION`]）
    pub schema_version: u32,
    /// 各窗口会话（v2 起；顺序即窗口创建顺序）
    pub windows: Vec<WindowSession>,
    /// 最后聚焦的主窗口 label（启动时激活；`None` = 主窗口）
    pub focused_label: Option<String>,
    /// v1 兼容字段：单窗口状态（读取旧文件时迁移，写出时省略）
    #[serde(skip_serializing, rename = "window")]
    pub legacy_window: WindowState,
    /// v1 兼容字段：单窗口活动下标（读取旧文件时迁移，写出时省略）
    #[serde(skip_serializing, rename = "activeTabIndex")]
    pub legacy_active_tab_index: u32,
    /// v1 兼容字段：单窗口标签（读取旧文件时迁移，写出时省略）
    #[serde(skip_serializing, rename = "tabs")]
    pub legacy_tabs: Vec<SessionTab>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            schema_version: SESSION_SCHEMA_VERSION,
            windows: Vec::new(),
            focused_label: None,
            legacy_window: WindowState::default(),
            legacy_active_tab_index: 0,
            legacy_tabs: Vec::new(),
        }
    }
}
