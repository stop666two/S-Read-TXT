// S-Read-TXT 主进程入口（src-tauri/src/main.rs）
// 阶段 1：storage（便携数据目录）+ settings（三类配置）接入；
// 命令清单按设计文档 §6 分阶段扩充。

// 发布构建隐藏 Windows 控制台窗口；调试构建保留控制台以便查看日志
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod settings;
mod storage;

use serde::Serialize;
use settings::store as settings_store;
use settings::SettingsSnapshot;
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
    let (dir, origin) = paths::resolve_data_dir();
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: dir.to_string_lossy().into_owned(),
        data_dir_origin: origin,
    }
}

/// 命令：探测数据目录可写性（启动自检与「目录不可写」引导流程的数据源）。
///
/// 返回：Ok(DataDirStatus)——即使不可写也返回 Ok，由前端依据 `writable`
///       字段决定进入正常流程还是引导选择目录流程。
#[tauri::command]
fn data_dir_status() -> DataDirStatus {
    let (dir, origin) = paths::resolve_data_dir();
    let dir_text = dir.to_string_lossy().into_owned();
    match data_dir::probe_writable(&dir) {
        Ok(()) => DataDirStatus {
            dir: dir_text,
            writable: true,
            message: None,
            origin,
        },
        Err(err) => DataDirStatus {
            dir: dir_text,
            writable: false,
            message: Some(err.to_string()),
            origin,
        },
    }
}

/// 命令：读取全部配置（聚合快照；快捷键字段为「生效绑定」= 默认 + 覆盖）。
///
/// 说明：读取具备自愈能力——文件缺失回默认值；内容损坏则备份为
///       `<文件名>.corrupt-<纳秒>` 后回默认值（见 `settings::store`）。
#[tauri::command]
fn get_settings() -> SettingsSnapshot {
    let (dir, _origin) = paths::resolve_data_dir();
    settings_store::load_snapshot(&dir)
}

fn main() {
    tauri::Builder::default()
        // 原生对话框能力（文件选择/目录选择/消息框）
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            data_dir_status,
            get_settings
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 应用启动失败");
}
