// S-Read-TXT 启动入口（薄层）：应用状态托管、日志初始化与 Tauri 装配。
// 业务逻辑位于 `s_read_txt` 库；IPC 命令位于 `commands` 模块。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use std::sync::Mutex;

use s_read_txt::app_state::AppState;
use s_read_txt::logging;
use s_read_txt::settings::store as settings_store;
use s_read_txt::storage::paths;

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
        // 运行状态：打开标签集合（由命令层以 Mutex 访问）
        .manage(Mutex::new(AppState::new()))
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::data_dir_status,
            commands::get_settings,
            commands::save_settings,
            commands::get_history,
            commands::remove_history,
            commands::clear_history,
            commands::get_session,
            commands::save_session,
            commands::open_file,
            commands::get_rows,
            commands::set_encoding,
            commands::list_encodings,
            commands::list_tabs,
            commands::close_tab
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 应用启动失败");
}
