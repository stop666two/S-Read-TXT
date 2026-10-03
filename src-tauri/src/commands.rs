//! Tauri IPC 命令层：薄封装（参数校验 + 日志链路 + 统一错误载荷）。
//!
//! 业务逻辑都在 `s_read_txt` 库内；本模块只负责：
//! - 从运行环境解析数据目录并加载配置；
//! - 访问受 `Mutex` 托管的 [`AppState`]；
//! - 把库错误映射为 [`IpcError`]（稳定错误码 + 中文消息）并输出链路日志。
//!
//! 参数命名：Tauri v2 默认把 Rust 下划线参数转换为 camelCase 暴露给前端
//! （如 `tab_id` → `tabId`）。

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{Emitter, State};

use s_read_txt::app_state::{AppState, RowsPayload, TabInfo};
use s_read_txt::background::{self, BackgroundEntry};
use s_read_txt::fonts::{self, FontEntry};
use s_read_txt::history::entry::HistoryEntry;
use s_read_txt::history::store as history_store;
use s_read_txt::ipc_error::{
    IpcError, CODE_BACKGROUND_INVALID, CODE_CONFIG_SAVE, CODE_HISTORY_SAVE, CODE_INVALID_ENCODING,
    CODE_INVALID_POSITION, CODE_INVALID_SCOPE, CODE_IO, CODE_MIGRATE_FAILED, CODE_SESSION_SAVE,
    CODE_SETTINGS_EXPORT, CODE_SETTINGS_IMPORT, CODE_SETTINGS_RESET, CODE_TAB_NOT_FOUND,
    CODE_THEME_INVALID,
};
use s_read_txt::logging;
use s_read_txt::logging::context::{with_context, LogContext};
use s_read_txt::resources;
use s_read_txt::session::model::SessionState;
use s_read_txt::session::store as session_store;
use s_read_txt::settings::registry::{self, SettingSpec};
use s_read_txt::settings::reset::{self as settings_reset, ResetScope};
use s_read_txt::settings::store as settings_store;
use s_read_txt::settings::theme::{self, ResolvedTheme, ThemeSummary};
use s_read_txt::settings::{bundle, shortcut_io, SettingsSaveRequest, SettingsSnapshot};
use s_read_txt::storage::data_dir;
use s_read_txt::storage::migrate_dir::{self as migrate_dir, MigrationReport};
use s_read_txt::storage::paths::{self, DataDirOrigin};
use s_read_txt::textfile::editing::edit_doc::{EditApplied, EditOp};
use s_read_txt::textfile::editing::search::{
    FindHit, ReplaceAllOutcome, ReplaceNextOutcome, ReplacePreview, SearchMode, PREVIEW_LIST_CAP,
};
use s_read_txt::textfile::encoding::FileEncoding;
use s_read_txt::time_util;

/// 应用信息（IPC 返回体；字段序列化为 camelCase 供前端直接消费）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// 应用版本号（取自 Cargo.toml，与 tauri.conf.json 保持同步）
    version: String,
    /// 数据目录绝对路径（便携解析：程序目录/data；`SRT_DATA_DIR` 可覆盖）
    data_dir: String,
    /// 数据目录来源（portable / envOverride）
    data_dir_origin: DataDirOrigin,
}

/// 数据目录状态（IPC 返回体）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataDirStatus {
    /// 数据目录绝对路径
    dir: String,
    /// 当前是否可写（真实写探针结果，非权限位推断）
    writable: bool,
    /// 不可写原因（可写时为 None；供前端提示与日志回溯）
    message: Option<String>,
    /// 数据目录来源（portable / envOverride / runtimeOverride / persisted）
    origin: DataDirOrigin,
    /// 持久化指针目标（程序目录 `config.json`；未设置时为 None）
    persisted: Option<String>,
}

/// 获取应用状态锁（中毒视为内部错误）。
fn lock_state<'a>(
    state: &'a State<'_, Mutex<AppState>>,
) -> Result<MutexGuard<'a, AppState>, IpcError> {
    state
        .lock()
        .map_err(|_| IpcError::internal("应用状态锁已被污染（前序线程 panic）"))
}

/// 解析可选编码标签：`None` = 保持当前/自动；未知标签报错。
fn parse_encoding_opt(encoding: Option<String>) -> Result<Option<FileEncoding>, IpcError> {
    match encoding {
        Some(label) => FileEncoding::from_label(&label)
            .map(Some)
            .ok_or_else(|| IpcError::new(CODE_INVALID_ENCODING, format!("未知编码：{label}"))),
        None => Ok(None),
    }
}

/// 解析查找模式标签：`literal`（默认）/ `regex`；未知标签报错。
fn parse_search_mode(mode: Option<String>) -> Result<SearchMode, IpcError> {
    match mode.as_deref() {
        None | Some("literal") => Ok(SearchMode::Literal),
        Some("regex") => Ok(SearchMode::Regex),
        Some(other) => Err(IpcError::new(
            CODE_INVALID_POSITION,
            format!("未知查找模式：{other}"),
        )),
    }
}

/// 命令：返回应用版本与数据目录（关于页数据源；亦用于 IPC 冒烟自检）。
#[tauri::command]
pub fn get_app_info() -> AppInfo {
    with_context(LogContext::request(), || {
        let (dir, origin) = paths::resolve_data_dir();
        AppInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            data_dir: dir.to_string_lossy().into_owned(),
            data_dir_origin: origin,
        }
    })
}

/// 读取迁移指针目标（状态展示用；不可读时为 None）。
fn persisted_dir_text() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    paths::read_pointer(&exe).map(|dir| dir.to_string_lossy().into_owned())
}

/// 命令：探测数据目录可写性（启动自检与「目录不可写」引导流程的数据源）。
///
/// 返回：Ok(DataDirStatus)——即使不可写也返回 Ok，由前端依据 `writable`
///       字段决定进入正常流程还是引导选择目录流程。
#[tauri::command]
pub fn data_dir_status() -> DataDirStatus {
    with_context(LogContext::request(), || {
        let (dir, origin) = paths::resolve_data_dir();
        let dir_text = dir.to_string_lossy().into_owned();
        match data_dir::probe_writable(&dir) {
            Ok(()) => DataDirStatus {
                dir: dir_text,
                writable: true,
                message: None,
                origin,
                persisted: persisted_dir_text(),
            },
            Err(err) => {
                // 重要事件：不可写会导致数据无法持久化，进入日志便于排查
                log::warn!(
                    target: "sread::storage",
                    "数据目录不可写：{}（{err}）",
                    dir_text
                );
                DataDirStatus {
                    dir: dir_text,
                    writable: false,
                    message: Some(err.to_string()),
                    origin,
                    persisted: persisted_dir_text(),
                }
            }
        }
    })
}

/// 命令：设置会话级数据目录（「数据目录不可写」引导流程使用）。
///
/// 行为：校验目录可创建且可写 → 设为运行时覆盖（此后设置/历史/会话/日志全部改路）
///       → 尝试切换日志输出目录（失败不阻塞切换本身）→ 返回新状态。
/// 说明：仅本次运行有效；重启后重新探测程序目录（符合既定的「会话级」决策）。
#[tauri::command]
pub fn set_data_dir(dir: String) -> DataDirStatus {
    with_context(LogContext::request(), || {
        let path = PathBuf::from(&dir);
        match data_dir::probe_writable(&path) {
            Ok(()) => {
                paths::set_runtime_override(path.clone());
                if let Err(err) = logging::retarget(&path) {
                    // 日志重定向失败不阻塞数据目录切换（应用继续运行）
                    log::warn!(target: "sread::storage", "日志目录切换失败：{err}");
                }
                log::info!(
                    target: "sread::storage",
                    "数据目录已切换（会话级）：{}",
                    path.display()
                );
                DataDirStatus {
                    dir: dir.clone(),
                    writable: true,
                    message: None,
                    origin: DataDirOrigin::RuntimeOverride,
                    persisted: persisted_dir_text(),
                }
            }
            Err(err) => DataDirStatus {
                dir,
                writable: false,
                message: Some(err.to_string()),
                origin: DataDirOrigin::RuntimeOverride,
                persisted: persisted_dir_text(),
            },
        }
    })
}

/// 命令：迁移数据目录到指定位置（P0-10；复制校验 → 写指针 → 清理原目录）。
///
/// 说明：成功后需重启应用方可完全生效（指针在启动时读取）；
///       原目录被占用时延迟清理（下次启动自动重试，见 `migrate_dir::cleanup_pending`）。
#[tauri::command]
pub fn migrate_data_dir(target: String) -> Result<MigrationReport, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let exe = std::env::current_exe()
            .map_err(|err| IpcError::new(CODE_IO, format!("无法定位程序路径：{err}")))?;
        let pointer = paths::pointer_path(&exe);
        let report = migrate_dir::migrate_data_dir(&dir, Path::new(&target), &pointer)
            .map_err(|err| IpcError::new(CODE_MIGRATE_FAILED, format!("迁移失败：{err}")))?;
        log::info!(
            target: "sread::storage",
            "数据目录已迁移：{} → {}（复制 {} 文件/{} 字节，跳过 {}，原目录已清理：{}）",
            dir.display(),
            target,
            report.copied_files,
            report.copied_bytes,
            report.skipped,
            report.old_removed
        );
        Ok(report)
    })
}

/// 命令：重启应用（数据目录迁移完成后立即生效；先拉起新进程再退出当前进程）。
///
/// 说明：不去等待新进程（旧进程退出即释放旧目录句柄）；直接退出可能跳过
///       WebView2 优雅清理（缓存可重建，无数据风险）。
#[tauri::command]
pub fn restart_app() -> Result<(), IpcError> {
    with_context(LogContext::request(), || {
        let exe = std::env::current_exe()
            .map_err(|err| IpcError::new(CODE_IO, format!("无法定位程序路径：{err}")))?;
        std::process::Command::new(exe)
            .spawn()
            .map_err(|err| IpcError::new(CODE_IO, format!("重启失败：{err}")))?;
        log::info!(target: "sread::main", "用户请求重启应用（数据目录迁移生效）");
        std::process::exit(0);
    })
}

/// 命令：读取全部配置（聚合快照；快捷键字段为「生效绑定」= 默认 + 覆盖）。
#[tauri::command]
pub fn get_settings() -> SettingsSnapshot {
    with_context(LogContext::request(), || {
        log::debug!(target: "sread::ipc", "读取配置快照");
        let (dir, _origin) = paths::resolve_data_dir();
        settings_store::load_snapshot(&dir)
    })
}

/// 命令：保存全部配置并返回保存后的快照（前端以返回值刷新状态）。
/// 成功后广播设置变更事件（主窗口热刷新；设置窗口本地状态以返回值为准，无需自行广播）。
#[tauri::command]
pub fn save_settings(
    app: tauri::AppHandle,
    request: SettingsSaveRequest,
) -> Result<SettingsSnapshot, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        settings_store::save_snapshot(&dir, &request)
            .map_err(|err| IpcError::new(CODE_CONFIG_SAVE, format!("保存配置失败：{err}")))?;
        log::info!(target: "sread::ipc", "配置已保存");
        let _ = app.emit(
            EVENT_SETTINGS_CHANGED,
            serde_json::json!({ "kind": "save" }),
        );
        Ok(settings_store::load_snapshot(&dir))
    })
}

/// 设置变更广播事件名（主窗口与设置窗口监听后热刷新）。
const EVENT_SETTINGS_CHANGED: &str = "srt://settings-changed";

/// 命令：导出全部配置为 JSON 包（写入用户选择的文件路径；返回写入字节数）。
#[tauri::command]
pub fn export_settings(path: String) -> Result<u64, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let bytes = bundle::export_to_file(&dir, Path::new(&path))
            .map_err(|message| IpcError::new(CODE_SETTINGS_EXPORT, message))?;
        log::info!(target: "sread::ipc", "配置已导出：{path}（{bytes} 字节）");
        Ok(bytes)
    })
}

/// 命令：从 JSON 包导入配置（强校验 + 备份 + 失败回滚）；成功后广播设置变更事件。
#[tauri::command]
pub fn import_settings(app: tauri::AppHandle, path: String) -> Result<SettingsSnapshot, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        bundle::import_from_file(&dir, Path::new(&path))
            .map_err(|message| IpcError::new(CODE_SETTINGS_IMPORT, message))?;
        log::info!(target: "sread::ipc", "配置已导入：{path}");
        let _ = app.emit(
            EVENT_SETTINGS_CHANGED,
            serde_json::json!({ "kind": "import" }),
        );
        Ok(settings_store::load_snapshot(&dir))
    })
}

/// 命令：导出快捷键到 JSON 文件（生效绑定全表；写入用户选择路径，返回字节数）。
#[tauri::command]
pub fn export_shortcuts(path: String) -> Result<u64, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let bytes = shortcut_io::export_to_file(&dir, &path)
            .map_err(|message| IpcError::new(CODE_SETTINGS_EXPORT, message))?;
        log::info!(target: "sread::ipc", "快捷键已导出：{path}（{bytes} 字节）");
        Ok(bytes)
    })
}

/// 命令：从 JSON 文件导入快捷键（强校验 + 备份 + 失败回滚）；成功后广播设置变更事件。
#[tauri::command]
pub fn import_shortcuts(app: tauri::AppHandle, path: String) -> Result<SettingsSnapshot, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        shortcut_io::import_from_file(&dir, &path)
            .map_err(|message| IpcError::new(CODE_SETTINGS_IMPORT, message))?;
        log::info!(target: "sread::ipc", "快捷键已导入：{path}");
        let _ = app.emit(
            EVENT_SETTINGS_CHANGED,
            serde_json::json!({ "kind": "import" }),
        );
        Ok(settings_store::load_snapshot(&dir))
    })
}

/// 命令：重置设置（全部 / 分组 / 单项）；成功后广播设置变更事件。
#[tauri::command]
pub fn reset_settings(
    app: tauri::AppHandle,
    scope: ResetScope,
) -> Result<SettingsSnapshot, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        settings_reset::reset_scope(&dir, &scope)
            .map_err(|message| IpcError::new(CODE_SETTINGS_RESET, message))?;
        log::info!(target: "sread::ipc", "设置已重置：{scope:?}");
        let _ = app.emit(
            EVENT_SETTINGS_CHANGED,
            serde_json::json!({ "kind": "reset" }),
        );
        Ok(settings_store::load_snapshot(&dir))
    })
}

/// 命令：设置项注册表（设置界面动态生成与搜索索引的唯一元数据源）。
#[tauri::command]
pub fn get_settings_registry() -> Vec<SettingSpec> {
    with_context(LogContext::request(), || registry::SPECS.to_vec())
}

/// 命令：数据目录磁盘占用分项统计（P0-8；目录缺失视为全 0）。
#[tauri::command]
pub fn get_disk_usage() -> Result<resources::DiskUsageReport, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        resources::disk_usage_result(&dir)
            .map_err(|err| IpcError::new(CODE_IO, format!("统计磁盘占用失败：{err}")))
    })
}

/// 命令：清理指定范围缓存（logs/webview/backups；逐文件容错，被占用计入 skipped）。
#[tauri::command]
pub fn clear_cache(scope: String) -> Result<resources::ClearResult, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let result = resources::clear_scope_by_key(&dir, &scope)
            .map_err(|err| IpcError::new(CODE_INVALID_SCOPE, err.to_string()))?;
        log::info!(
            target: "sread::ipc",
            "缓存清理：scope={scope} 释放={} 字节 跳过={}",
            result.cleared_bytes,
            result.skipped
        );
        Ok(result)
    })
}

/// 命令：默认快捷键表（动作 id → 组合键；设置界面「恢复默认」的唯一真源）。
#[tauri::command]
pub fn get_default_shortcuts() -> std::collections::BTreeMap<String, String> {
    with_context(LogContext::request(), || {
        s_read_txt::settings::defaults::default_bindings()
    })
}

/// 背景图读取返回体（base64 + MIME；前端拼 data URL 渲染图层）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundImageData {
    /// 图片字节的 Base64 编码
    data_base64: String,
    /// MIME 类型（image/png | image/jpeg | image/webp）
    mime: String,
}

/// 命令：设置背景图（校验扩展名/大小 → 复制到数据目录 `backgrounds/` → 单文件驻留）。
#[tauri::command]
pub fn set_background_file(path: String) -> Result<BackgroundEntry, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        background::set_background(&dir, Path::new(&path)).map_err(background_ipc_error)
    })
}

/// 命令：删除背景图文件（幂等；非法文件名仍拒绝）。
#[tauri::command]
pub fn clear_background_file(file_name: String) -> Result<(), IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        background::remove_background(&dir, &file_name).map_err(background_ipc_error)
    })
}

/// 命令：读取背景图（文件缺失/非法名返回错误；前端降级为无背景）。
#[tauri::command]
pub fn read_background_image(file_name: String) -> Result<BackgroundImageData, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let (bytes, mime) =
            background::read_background(&dir, &file_name).map_err(background_ipc_error)?;
        Ok(BackgroundImageData {
            data_base64: fonts::base64_encode(&bytes),
            mime: mime.to_string(),
        })
    })
}

/// 背景图错误 → IPC 载荷（统一错误码 `BACKGROUND_INVALID`；消息为中文原因）。
fn background_ipc_error(err: background::BackgroundError) -> IpcError {
    IpcError::new(CODE_BACKGROUND_INVALID, err.to_string())
}

/// 命令：读取历史记录（去重 + 修剪 + 时间倒序；必要时自愈压缩文件）。
#[tauri::command]
pub fn get_history() -> Vec<HistoryEntry> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let settings = settings_store::load_app_settings(&dir);
        history_store::load(&dir, &settings.history)
    })
}

/// 命令：删除单条历史（以文件路径为键；返回更新后的列表）。
#[tauri::command]
pub fn remove_history(file_path: String) -> Result<Vec<HistoryEntry>, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let settings = settings_store::load_app_settings(&dir);
        let mut entries = history_store::load(&dir, &settings.history);
        entries.retain(|entry| entry.path != file_path);
        history_store::write_all(&dir, &entries)
            .map_err(|err| IpcError::new(CODE_HISTORY_SAVE, format!("保存历史失败：{err}")))?;
        log::info!(
            target: "sread::ipc",
            "历史条目已删除（剩余 {} 条）",
            entries.len()
        );
        Ok(entries)
    })
}

/// 命令：清空全部历史。
#[tauri::command]
pub fn clear_history() -> Result<(), IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        history_store::write_all(&dir, &[])
            .map_err(|err| IpcError::new(CODE_HISTORY_SAVE, format!("清空历史失败：{err}")))?;
        log::info!(target: "sread::ipc", "历史已清空");
        Ok(())
    })
}

/// 命令：读取会话（窗口状态 + 标签锚点；自愈载入，损坏回退默认）。
#[tauri::command]
pub fn get_session() -> SessionState {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        session_store::load(&dir)
    })
}

/// 命令：保存会话（退出/周期性调用；返回保存后的会话以便前端确认）。
#[tauri::command]
pub fn save_session(session: SessionState) -> Result<SessionState, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        session_store::save(&dir, &session)
            .map_err(|err| IpcError::new(CODE_SESSION_SAVE, format!("保存会话失败：{err}")))?;
        log::debug!(
            target: "sread::ipc",
            "会话已保存（{} 个标签）",
            session.tabs.len()
        );
        Ok(session_store::load(&dir))
    })
}

/// 命令：打开文件（重复打开自动复用已有标签；首次打开成功时记录历史）。
#[tauri::command]
pub fn open_file(path: String, state: State<'_, Mutex<AppState>>) -> Result<TabInfo, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let settings = settings_store::load_app_settings(&dir);
        let (info, reused) = lock_state(&state)?.open_file(Path::new(&path), &settings)?;
        if !reused {
            let entry = HistoryEntry {
                path: info.path.clone(),
                name: info.name.clone(),
                size: info.byte_len,
                encoding: info.encoding.clone(),
                opened_at: time_util::now_rfc3339(),
                last_row: 0,
                last_percent: 0.0,
            };
            // 历史为辅助功能：写失败不影响打开
            if let Err(err) = history_store::append(&dir, &entry) {
                log::warn!(target: "sread::history", "历史写入失败：{err}");
            }
        }
        log::info!(
            target: "sread::ipc",
            "打开文件：标签 {}，{} 行，编码 {}{}",
            info.tab_id,
            info.rows_total,
            info.encoding,
            if reused { "（复用已有标签）" } else { "" }
        );
        Ok(info)
    })
}

/// 命令：取文本窗口（`count` 上限 2048，超出自动截断）。
#[tauri::command]
pub fn get_rows(
    tab_id: u64,
    start_row: u64,
    count: u32,
    state: State<'_, Mutex<AppState>>,
) -> Result<RowsPayload, IpcError> {
    with_context(LogContext::request(), || {
        lock_state(&state)?
            .rows(tab_id, start_row, count)
            .map_err(IpcError::from)
    })
}

/// 命令：切换标签编码（`None` 恢复自动检测）。
#[tauri::command]
pub fn set_encoding(
    tab_id: u64,
    encoding: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<TabInfo, IpcError> {
    with_context(LogContext::request(), || {
        let parsed = parse_encoding_opt(encoding)?;
        let info = lock_state(&state)?.set_encoding(tab_id, parsed)?;
        log::info!(
            target: "sread::ipc",
            "切换编码：标签 {} → {}",
            tab_id,
            info.encoding
        );
        Ok(info)
    })
}

/// 命令：支持的编码列表（界面编码菜单数据源，顺序即展示顺序）。
#[tauri::command]
pub fn list_encodings() -> Vec<&'static str> {
    FileEncoding::ALL
        .iter()
        .map(|encoding| encoding.label())
        .collect()
}

/// 标签视图（标签列表 + 活动标签；供前端重建标签栏）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabsView {
    /// 全部标签（按创建顺序）
    tabs: Vec<TabInfo>,
    /// 当前活动标签（无标签时为 None）
    active_tab_id: Option<u64>,
}

/// 命令：列出全部标签（前端启动同步/恢复时使用）。
#[tauri::command]
pub fn list_tabs(state: State<'_, Mutex<AppState>>) -> Result<TabsView, IpcError> {
    with_context(LogContext::request(), || {
        let guard = lock_state(&state)?;
        Ok(TabsView {
            tabs: guard.tabs_info(),
            active_tab_id: guard.active_tab(),
        })
    })
}

/// 命令：关闭标签并返回剩余标签视图。
///
/// 幂等：关闭不存在的标签不视为错误（返回当前视图，规则：可重试操作幂等）。
#[tauri::command]
pub fn close_tab(tab_id: u64, state: State<'_, Mutex<AppState>>) -> Result<TabsView, IpcError> {
    with_context(LogContext::request(), || {
        let mut guard = lock_state(&state)?;
        let closed = guard.close(tab_id);
        let view = TabsView {
            tabs: guard.tabs_info(),
            active_tab_id: guard.active_tab(),
        };
        if closed {
            log::info!(
                target: "sread::ipc",
                "关闭标签：{}（剩余 {} 个）",
                tab_id,
                view.tabs.len()
            );
        }
        Ok(view)
    })
}

/// 保存结果（IPC 载荷）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTabResult {
    /// 实际写入字节数（含 BOM）
    bytes_written: u64,
    /// `.bak` 路径（未写时为 None）
    backup_path: Option<String>,
    /// 实际保存编码（标签名）
    encoding: String,
    /// 保存后的标签信息（脏态/行数等以返回值刷新）
    tab: TabInfo,
}

/// 命令：切换编辑模式（首次进入创建编辑文档；单行超 64KB 拒绝）。
#[tauri::command]
pub fn toggle_edit(tab_id: u64, state: State<'_, Mutex<AppState>>) -> Result<TabInfo, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let settings = settings_store::load_app_settings(&dir);
        let info = lock_state(&state)?.toggle_edit(tab_id, &settings)?;
        log::info!(
            target: "sread::ipc",
            "编辑模式：标签 {} → {}",
            tab_id,
            if info.editing { "开" } else { "关" }
        );
        Ok(info)
    })
}

/// 命令：应用编辑批次（插入/删除/替换；批次 = 单个撤销步）。
#[tauri::command]
pub fn apply_edits(
    tab_id: u64,
    ops: Vec<EditOp>,
    state: State<'_, Mutex<AppState>>,
) -> Result<EditApplied, IpcError> {
    with_context(LogContext::request(), || {
        lock_state(&state)?
            .apply_edit_ops(tab_id, &ops)
            .map_err(IpcError::from)
    })
}

/// 命令：撤销一步（无可撤销内容时返回 null）。
#[tauri::command]
pub fn undo_edit(
    tab_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Option<EditApplied>, IpcError> {
    with_context(LogContext::request(), || {
        lock_state(&state)?
            .undo_edit(tab_id)
            .map_err(IpcError::from)
    })
}

/// 命令：重做一步（无可重做内容时返回 null）。
#[tauri::command]
pub fn redo_edit(
    tab_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Option<EditApplied>, IpcError> {
    with_context(LogContext::request(), || {
        lock_state(&state)?
            .redo_edit(tab_id)
            .map_err(IpcError::from)
    })
}

/// 命令：保存标签（编码询问流结果传入；`force` = 冲突时覆盖）。
#[tauri::command]
pub fn save_tab(
    tab_id: u64,
    target_encoding: Option<String>,
    make_backup: bool,
    force: bool,
    state: State<'_, Mutex<AppState>>,
) -> Result<SaveTabResult, IpcError> {
    with_context(LogContext::request(), || {
        let encoding = parse_encoding_opt(target_encoding)?;
        let mut guard = lock_state(&state)?;
        let outcome = guard.save_edit(tab_id, encoding, make_backup, force)?;
        let tab = guard
            .tab_info(tab_id)
            .ok_or_else(|| IpcError::new(CODE_TAB_NOT_FOUND, format!("标签不存在：{tab_id}")))?;
        log::info!(
            target: "sread::ipc",
            "保存：标签 {} → {}（{} 字节{}）",
            tab_id,
            tab.encoding,
            outcome.bytes_written,
            if outcome.backup_path.is_some() {
                "，已写 .bak"
            } else {
                ""
            }
        );
        Ok(SaveTabResult {
            bytes_written: outcome.bytes_written,
            backup_path: outcome
                .backup_path
                .map(|path| path.to_string_lossy().into_owned()),
            encoding: outcome.encoding.label().to_string(),
            tab,
        })
    })
}

/// 命令：另存为（成功后标签重定向到新路径；新路径写入历史）。
#[tauri::command]
pub fn save_tab_as(
    tab_id: u64,
    new_path: String,
    target_encoding: Option<String>,
    make_backup: bool,
    state: State<'_, Mutex<AppState>>,
) -> Result<SaveTabResult, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let settings = settings_store::load_app_settings(&dir);
        let encoding = parse_encoding_opt(target_encoding)?;
        let (result, entry) = {
            let mut guard = lock_state(&state)?;
            let outcome = guard.save_edit_as(
                tab_id,
                Path::new(&new_path),
                encoding,
                make_backup,
                &settings,
            )?;
            let tab = guard.tab_info(tab_id).ok_or_else(|| {
                IpcError::new(CODE_TAB_NOT_FOUND, format!("标签不存在：{tab_id}"))
            })?;
            let entry = HistoryEntry {
                path: tab.path.clone(),
                name: tab.name.clone(),
                size: tab.byte_len,
                encoding: tab.encoding.clone(),
                opened_at: time_util::now_rfc3339(),
                last_row: 0,
                last_percent: 0.0,
            };
            let result = SaveTabResult {
                bytes_written: outcome.bytes_written,
                backup_path: outcome
                    .backup_path
                    .map(|path| path.to_string_lossy().into_owned()),
                encoding: outcome.encoding.label().to_string(),
                tab,
            };
            (result, entry)
        };
        // 历史为辅助功能：写失败不影响保存结果（锁外做 IO）
        if let Err(err) = history_store::append(&dir, &entry) {
            log::warn!(target: "sread::history", "历史写入失败：{err}");
        }
        log::info!(
            target: "sread::ipc",
            "另存为：标签 {} → {}（{} 字节）",
            tab_id,
            entry.path,
            result.bytes_written
        );
        Ok(result)
    })
}

/// 命令：从磁盘重载（丢弃未保存修改；脏态确认由前端完成）。
#[tauri::command]
pub fn reload_tab(tab_id: u64, state: State<'_, Mutex<AppState>>) -> Result<TabInfo, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let settings = settings_store::load_app_settings(&dir);
        let info = lock_state(&state)?.reload_tab(tab_id, &settings)?;
        log::info!(target: "sread::ipc", "重载：标签 {}", tab_id);
        Ok(info)
    })
}

/// 命令：在编辑文档中查找（标准/正则；不环绕；`from` = 显示行 UTF-16 坐标，None 从头）。
#[tauri::command]
pub fn find_in_edit(
    tab_id: u64,
    query: String,
    case_sensitive: bool,
    mode: Option<String>,
    from: Option<(u64, u64)>,
    state: State<'_, Mutex<AppState>>,
) -> Result<Option<FindHit>, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        lock_state(&state)?
            .find_in_tab(tab_id, &query, case_sensitive, mode, from)
            .map_err(IpcError::from)
    })
}

/// 命令：替换一个命中（从 `from` 起）并返回结果与「新落点起的下一个命中」。
#[tauri::command]
pub fn replace_in_edit(
    tab_id: u64,
    query: String,
    case_sensitive: bool,
    mode: Option<String>,
    from: Option<(u64, u64)>,
    replacement: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<Option<ReplaceNextOutcome>, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        lock_state(&state)?
            .replace_next_in_tab(tab_id, &query, case_sensitive, mode, from, &replacement)
            .map_err(IpcError::from)
    })
}

/// 命令：全部替换（单个撤销步；命中过多报 QUERY_TOO_BROAD）。
///
/// 说明：前端正常流程使用「预览 → 二次确认 → apply_replace_all_in_edit」；
/// 本命令保留为直接入口（自动化测试与将来可能的“不再询问”偏好）。
#[tauri::command]
pub fn replace_all_in_edit(
    tab_id: u64,
    query: String,
    case_sensitive: bool,
    mode: Option<String>,
    replacement: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<ReplaceAllOutcome, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        let outcome = lock_state(&state)?.replace_all_in_tab(
            tab_id,
            &query,
            case_sensitive,
            mode,
            &replacement,
        )?;
        log::info!(
            target: "sread::ipc",
            "全部替换：标签 {}，命中 {} 处",
            tab_id,
            outcome.replaced
        );
        Ok(outcome)
    })
}

/// 命令：生成「全部替换」预览（命中总数 + 前 500 条前后文本；供二次确认弹窗）。
#[tauri::command]
pub fn preview_replace_all_in_edit(
    tab_id: u64,
    query: String,
    case_sensitive: bool,
    mode: Option<String>,
    replacement: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<ReplacePreview, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        lock_state(&state)?
            .preview_replace_all_in_tab(
                tab_id,
                &query,
                case_sensitive,
                mode,
                &replacement,
                PREVIEW_LIST_CAP,
            )
            .map_err(IpcError::from)
    })
}

/// 命令：执行「全部替换」（`selected = null` 全部；数组 = 仅替换所列序号；
/// `expect_state_id` 校验预览后文档未变化）。
#[tauri::command]
pub fn apply_replace_all_in_edit(
    tab_id: u64,
    query: String,
    case_sensitive: bool,
    mode: Option<String>,
    replacement: String,
    selected: Option<Vec<usize>>,
    expect_state_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<ReplaceAllOutcome, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        // 防御：下标排序去重（二分查找前置条件；前端正常已保证有序）
        let mut list = selected;
        if let Some(values) = &mut list {
            values.sort_unstable();
            values.dedup();
        }
        let outcome = lock_state(&state)?.replace_matches_in_tab(
            tab_id,
            &query,
            case_sensitive,
            mode,
            &replacement,
            list.as_deref(),
            expect_state_id,
        )?;
        log::info!(
            target: "sread::ipc",
            "全部替换（确认后）：标签 {}，命中 {} 处{}",
            tab_id,
            outcome.replaced,
            if list.is_none() { "（全部）" } else { "（已剔除部分）" }
        );
        Ok(outcome)
    })
}

/// 命令：显示行窗口内的命中（文档高亮用）。
#[tauri::command]
pub fn match_window_in_edit(
    tab_id: u64,
    query: String,
    case_sensitive: bool,
    mode: Option<String>,
    start_row: u64,
    count: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<FindHit>, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        lock_state(&state)?
            .match_window_in_tab(tab_id, &query, case_sensitive, mode, start_row, count)
            .map_err(IpcError::from)
    })
}

/// 命令：同步活动标签（前端点击/快捷键选择后调用；标签不存在报错）。
///
/// 说明：后端活动标签决定关闭回落方向与会话语义，必须与前端选择保持一致。
#[tauri::command]
pub fn set_active_tab(tab_id: u64, state: State<'_, Mutex<AppState>>) -> Result<(), IpcError> {
    with_context(LogContext::request(), || {
        lock_state(&state)?
            .set_active_tab(tab_id)
            .map_err(IpcError::from)
    })
}

/// 命令：调整标签展示顺序（拖拽排序；下标记「移除后再插入」语义，越界收敛到末尾）。
#[tauri::command]
pub fn reorder_tab(
    tab_id: u64,
    to_index: u32,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), IpcError> {
    with_context(LogContext::request(), || {
        lock_state(&state)?
            .reorder(tab_id, to_index as usize)
            .map_err(IpcError::from)
    })
}

/// 命令：更新历史条目阅读进度（标签关闭/退出前由前端调用；幂等）。
///
/// 说明：未找到对应条目（如历史已被清理）时静默成功——进度更新是尽力而为。
#[tauri::command]
pub fn update_history_progress(
    path: String,
    last_row: u64,
    last_percent: f64,
) -> Result<(), IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        history_store::update_progress(&dir, &path, last_row, last_percent)
            .map_err(|err| IpcError::new(CODE_HISTORY_SAVE, format!("历史进度保存失败：{err}")))?;
        Ok(())
    })
}

/// 设置窗口待打开的页签（`open_settings` 写入；设置窗口启动时经 `take_settings_tab` 取走）。
/// 用进程级静态而非 AppState：设置窗口生命周期与标签状态无关，且值极小。
static SETTINGS_PENDING_TAB: Mutex<Option<String>> = Mutex::new(None);

/// 命令：打开设置窗口（已存在则显示并聚焦；不存在按需创建）。
///
/// 说明：设置窗口按需创建而非启动时常驻——隐藏的 WebView 仍占内存（约数十 MB），
/// 会挤压「10 标签 <100MB」的内存红线；关闭后窗口销毁，再次打开重建。
/// `tab`：请求打开时定位的页签（如 `shortcuts` / `about`；None 用默认页签）。
#[tauri::command]
pub async fn open_settings(app: tauri::AppHandle, tab: Option<String>) -> Result<(), IpcError> {
    use tauri::Manager;
    if let Some(requested) = tab {
        if let Ok(mut guard) = SETTINGS_PENDING_TAB.lock() {
            *guard = Some(requested);
        }
    }
    if let Some(window) = app.get_webview_window("settings") {
        window
            .show()
            .map_err(|err| IpcError::internal(format!("显示设置窗口失败：{err}")))?;
        window
            .set_focus()
            .map_err(|err| IpcError::internal(format!("聚焦设置窗口失败：{err}")))?;
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("设置 - S-Read-TXT")
    .inner_size(800.0, 620.0)
    .resizable(false)
    .maximizable(false)
    .decorations(false)
    .visible(false)
    .center()
    .build()
    .map_err(|err| IpcError::internal(format!("创建设置窗口失败：{err}")))?;
    log::info!(target: "sread::ipc", "设置窗口已创建");
    Ok(())
}

/// 命令：取走设置窗口待打开页签（读取即清空；无待办返回 None）。
#[tauri::command]
pub fn take_settings_tab() -> Option<String> {
    SETTINGS_PENDING_TAB
        .lock()
        .ok()
        .and_then(|mut guard| guard.take())
}

/// 命令：列出已导入的自定义字体（数据目录 `fonts/`）。
#[tauri::command]
pub fn list_fonts() -> Result<Vec<FontEntry>, IpcError> {
    let (dir, _origin) = paths::resolve_data_dir();
    fonts::list_fonts(&dir)
        .map_err(|err| IpcError::new(CODE_IO, format!("读取字体目录失败：{err}")))
}

/// 命令：导入字体文件（复制到数据目录 `fonts/`；重名自动唯一化）。
#[tauri::command]
pub fn import_font(path: String) -> Result<FontEntry, IpcError> {
    let (dir, _origin) = paths::resolve_data_dir();
    let entry = fonts::import_font(&dir, std::path::Path::new(&path))?;
    log::info!(target: "sread::ipc", "字体已导入：{}", entry.file_name);
    Ok(entry)
}

/// 命令：删除已导入字体。
#[tauri::command]
pub fn remove_font(file_name: String) -> Result<(), IpcError> {
    let (dir, _origin) = paths::resolve_data_dir();
    fonts::remove_font(&dir, &file_name)?;
    log::info!(target: "sread::ipc", "字体已删除：{file_name}");
    Ok(())
}

/// 命令：读取字体字节（Base64 编码；前端经 FontFace 动态加载，用后由浏览器管理）。
#[tauri::command]
pub fn read_font_data(file_name: String) -> Result<String, IpcError> {
    let (dir, _origin) = paths::resolve_data_dir();
    let bytes = fonts::read_font_bytes(&dir, &file_name)?;
    Ok(fonts::base64_encode(&bytes))
}

/// 命令：主题清单（内置 + 用户主题；不含令牌，体积小）。
#[tauri::command]
pub fn list_themes() -> Vec<ThemeSummary> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        theme::list_themes(&dir)
    })
}

/// 命令：解析主题（`id` 为空/缺省 = 当前设置；`system` 解析为 light/dark。
/// 结果写入单主题驻留缓存，供 `get_theme` 的前端契约直读）。
#[tauri::command]
pub fn get_theme(id: Option<String>) -> Result<ResolvedTheme, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let requested = match id {
            Some(value) if !value.trim().is_empty() => value,
            _ => settings_store::load_reader_settings(&dir).theme_id,
        };
        theme::resolve_theme(&dir, &requested).map_err(theme_ipc_error)
    })
}

/// 命令：导入用户主题（强校验；内置 id 拒绝覆盖）。
#[tauri::command]
pub fn import_theme(path: String) -> Result<ThemeSummary, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let summary = theme::import_theme(&dir, Path::new(&path)).map_err(theme_ipc_error)?;
        log::info!(target: "sread::ipc", "主题已导入：{}", summary.id);
        Ok(summary)
    })
}

/// 命令：导出主题到指定路径（内置与用户主题均可）。
#[tauri::command]
pub fn export_theme(id: String, path: String) -> Result<(), IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        theme::export_theme(&dir, &id, Path::new(&path)).map_err(theme_ipc_error)?;
        log::info!(target: "sread::ipc", "主题已导出：{id}");
        Ok(())
    })
}

/// 命令：删除用户主题（内置主题拒绝删除）。
#[tauri::command]
pub fn remove_theme(id: String) -> Result<(), IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        theme::remove_theme(&dir, &id).map_err(theme_ipc_error)?;
        log::info!(target: "sread::ipc", "主题已删除：{id}");
        Ok(())
    })
}

/// 主题错误 → IPC 错误（统一 `THEME_INVALID` 码；消息含具体原因）。
fn theme_ipc_error(err: theme::ThemeError) -> IpcError {
    IpcError::new(CODE_THEME_INVALID, err.to_string())
}
