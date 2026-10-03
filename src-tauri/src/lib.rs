//! S-Read-TXT 核心库：**纯业务逻辑**（无 GUI/框架依赖，便于单元与集成测试）。
//!
//! 结构说明：lib + bin 拆分——
//! - 本库只包含与运行环境解耦的模块（存储/配置/日志/历史/会话/读取引擎）；
//! - Tauri 应用胶水（IPC 命令、窗口与插件装配）位于 `main.rs`（bin），
//!   这样测试程序无需链接 WebView2（GNU 工具链下 webview2 库以 DLL 形式
//!   存在且可能被系统目录的旧副本遮蔽，测试保持纯逻辑可规避该类环境问题）。
//!
//! 模块总览（详见各模块文档与设计文档 §3–§7）：
//! - `storage`：便携数据目录、原子写、JSON 容错读写
//! - `settings`：settings/reader/shortcuts 三类配置
//! - `logging`：JSON 行日志 + 轮转 + 请求链路上下文
//! - `history`：历史记录（JSONL）
//! - `session`：会话（窗口 + 标签锚点）
//! - `elevation`：管理员权限（UAC）按需申请（便携数据目录不可写且非管理员时）
//! - `textfile`：读取引擎（mmap/编码/稀疏索引/文本窗口）
//! - `time_util`：RFC 3339 时间工具

pub mod app_state;
pub mod background;
pub mod clipboard_history;
pub mod elevation;
pub mod find_history;
pub mod fonts;
pub mod history;
pub mod ipc_error;
pub mod logging;
pub mod resources;
pub mod session;
pub mod settings;
pub mod storage;
pub mod textfile;
pub mod time_util;
