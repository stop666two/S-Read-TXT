//! `session.json` 模型（会话：窗口状态 + 打开标签锚点）。
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
/// 会话配置格式版本（独立于设置 schema：会话结构变更时递增并在会话模块内提供迁移）
pub const SESSION_SCHEMA_VERSION: u32 = 1;

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
}

impl Default for SessionTab {
    fn default() -> Self {
        Self {
            path: String::new(),
            encoding: None,
            scroll_row: 0,
            edit_mode: false,
        }
    }
}

/// 会话（`session.json`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SessionState {
    /// 会话格式版本（保存时写入当前 [`SESSION_SCHEMA_VERSION`]）
    pub schema_version: u32,
    /// 窗口状态
    pub window: WindowState,
    /// 活动标签下标（归一后保证落在 `tabs` 范围内）
    pub active_tab_index: u32,
    /// 上次打开的标签列表（惰性恢复）
    pub tabs: Vec<SessionTab>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            schema_version: SESSION_SCHEMA_VERSION,
            window: WindowState::default(),
            active_tab_index: 0,
            tabs: Vec::new(),
        }
    }
}
