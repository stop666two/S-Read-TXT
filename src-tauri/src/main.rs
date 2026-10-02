// S-Read-TXT 主进程入口（src-tauri/src/main.rs）
// 阶段 1：storage/settings/logging/history/session 接入；
// 每个 IPC 命令携带请求链路上下文（req id），日志可串联同一次调用。

// 发布构建隐藏 Windows 控制台窗口；调试构建保留控制台以便查看日志
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod history;
mod logging;
mod session;
mod settings;
mod storage;
// textfile 目前由单元测试驱动；阶段 2 接线阅读命令后移除该 allow（见进度台账 1.7）
#[allow(dead_code)]
mod textfile;
mod time_util;

use serde::Serialize;

use history::entry::HistoryEntry;
use history::store as history_store;
use logging::context::{with_context, LogContext};
use session::model::SessionState;
use session::store as session_store;
use settings::store as settings_store;
use settings::{SettingsSaveRequest, SettingsSnapshot};
use storage::data_dir;
use storage::paths::{self, DataDirOrigin};

/// 应用信息（IPC 返回体；字段序列化为 camelCase 供前端直接消费）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
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
struct DataDirStatus {
    /// 数据目录绝对路径
    dir: String,
    /// 当前是否可写（真实写探针结果，非权限位推断）
    writable: bool,
    /// 不可写原因（可写时为 None；供前端提示与日志回溯）
    message: Option<String>,
    /// 数据目录来源（portable / envOverride）
    origin: DataDirOrigin,
}

/// 命令：返回应用版本与数据目录（关于页数据源；亦用于 IPC 冒烟自检）。
#[tauri::command]
fn get_app_info() -> AppInfo {
    with_context(LogContext::request(), || {
        let (dir, origin) = paths::resolve_data_dir();
        AppInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            data_dir: dir.to_string_lossy().into_owned(),
            data_dir_origin: origin,
        }
    })
}

/// 命令：探测数据目录可写性（启动自检与「目录不可写」引导流程的数据源）。
///
/// 返回：Ok(DataDirStatus)——即使不可写也返回 Ok，由前端依据 `writable`
///       字段决定进入正常流程还是引导选择目录流程。
#[tauri::command]
fn data_dir_status() -> DataDirStatus {
    with_context(LogContext::request(), || {
        let (dir, origin) = paths::resolve_data_dir();
        let dir_text = dir.to_string_lossy().into_owned();
        match data_dir::probe_writable(&dir) {
            Ok(()) => DataDirStatus {
                dir: dir_text,
                writable: true,
                message: None,
                origin,
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
                }
            }
        }
    })
}

/// 命令：读取全部配置（聚合快照；快捷键字段为「生效绑定」= 默认 + 覆盖）。
///
/// 说明：读取具备自愈能力——文件缺失回默认值；内容损坏则备份为
///       `<文件名>.corrupt-<纳秒>` 后回默认值（见 `settings::store`）。
#[tauri::command]
fn get_settings() -> SettingsSnapshot {
    with_context(LogContext::request(), || {
        log::debug!(target: "sread::ipc", "读取配置快照");
        let (dir, _origin) = paths::resolve_data_dir();
        settings_store::load_snapshot(&dir)
    })
}

/// 命令：保存全部配置并返回保存后的快照（前端以返回值刷新状态）。
///
/// 入参 `shortcuts` 为生效绑定全表；后端只落盘与默认不同的覆盖项。
/// 返回：Ok(保存后的快照)；Err(中文错误文案)。
#[tauri::command]
fn save_settings(request: SettingsSaveRequest) -> Result<SettingsSnapshot, String> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        settings_store::save_snapshot(&dir, &request)
            .map_err(|err| format!("保存配置失败：{err}"))?;
        log::info!(target: "sread::ipc", "配置已保存");
        Ok(settings_store::load_snapshot(&dir))
    })
}

/// 命令：读取历史记录（去重 + 修剪 + 时间倒序；必要时自愈压缩文件）。
#[tauri::command]
fn get_history() -> Vec<HistoryEntry> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let settings = settings_store::load_app_settings(&dir);
        history_store::load(&dir, &settings.history)
    })
}

/// 命令：删除单条历史（以文件路径为键；返回更新后的列表）。
#[tauri::command]
fn remove_history(file_path: String) -> Result<Vec<HistoryEntry>, String> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let settings = settings_store::load_app_settings(&dir);
        let mut entries = history_store::load(&dir, &settings.history);
        entries.retain(|entry| entry.path != file_path);
        history_store::write_all(&dir, &entries).map_err(|err| format!("保存历史失败：{err}"))?;
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
fn clear_history() -> Result<(), String> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        history_store::write_all(&dir, &[]).map_err(|err| format!("清空历史失败：{err}"))?;
        log::info!(target: "sread::ipc", "历史已清空");
        Ok(())
    })
}

/// 命令：读取会话（窗口状态 + 标签锚点；自愈载入，损坏回退默认）。
#[tauri::command]
fn get_session() -> SessionState {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        session_store::load(&dir)
    })
}

/// 命令：保存会话（退出/周期性调用；返回保存后的会话以便前端确认）。
#[tauri::command]
fn save_session(state: SessionState) -> Result<SessionState, String> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        session_store::save(&dir, &state).map_err(|err| format!("保存会话失败：{err}"))?;
        log::debug!(
            target: "sread::ipc",
            "会话已保存（{} 个标签）",
            state.tabs.len()
        );
        Ok(session_store::load(&dir))
    })
}

fn main() {
    // 日志先行：级别来源 SRT_LOG_LEVEL > settings.json 的 logLevel > 默认 info；
    // 日志初始化失败不阻塞应用（降级为无文件日志）。
    let (startup_dir, _origin) = paths::resolve_data_dir();
    let startup_settings = settings_store::load_app_settings(&startup_dir);
    if let Err(err) = logging::init(&startup_dir, startup_settings.log_level) {
        eprintln!("[s-read-txt] 日志初始化失败（应用继续运行）：{err}");
    }
    log::info!(
        target: "sread::main",
        "S-Read-TXT v{} 启动（数据目录：{}）",
        env!("CARGO_PKG_VERSION"),
        startup_dir.display()
    );

    tauri::Builder::default()
        // 原生对话框能力（文件选择/目录选择/消息框）
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            data_dir_status,
            get_settings,
            save_settings,
            get_history,
            remove_history,
            clear_history,
            get_session,
            save_session
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 应用启动失败");
}
