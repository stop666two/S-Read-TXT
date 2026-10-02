// S-Read-TXT 启动入口（薄层）：应用状态托管、日志初始化与 Tauri 装配。
// 业务逻辑位于 `s_read_txt` 库；IPC 命令位于 `commands` 模块。

// 始终隐藏 Windows 控制台窗口：
// - 发布构建：不弹控制台；
// - 调试构建：同样隐藏（此前 debug 会弹一个空白控制台窗口，易被误认为程序故障）；
//   调试期日志一律走 data/logs 文件日志与 CDP 验证（SRT_LOG_LEVEL 控制级别）。
#![windows_subsystem = "windows"]

mod commands;

use std::sync::Mutex;

use s_read_txt::app_state::AppState;
use s_read_txt::logging;
use s_read_txt::settings::store as settings_store;
use s_read_txt::storage::data_dir;
use s_read_txt::storage::paths;

fn main() {
    // 日志先行：级别来源 SRT_LOG_LEVEL > settings.json 的 logLevel > 默认 info；
    // 日志初始化失败不阻塞应用（降级为无文件日志）。
    let (startup_dir, _origin) = paths::resolve_data_dir();
    // WebView2 用户数据目录重定向到便携 data/webview（默认写 %LOCALAPPDATA%，
    // 违反「数据全部在程序目录」红线；`WEBVIEW2_USER_DATA_FOLDER` 由 WebView2Loader
    // 在创建环境时读取，必须在 Builder 之前设置）。目录不可写时暂不重定向
    // （避免 WebView 初始化失败；阶段 8 接入「选择可写目录」引导）。
    match data_dir::probe_writable(&startup_dir) {
        Ok(()) => {
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", startup_dir.join("webview"));
        }
        Err(err) => {
            eprintln!("[s-read-txt] 数据目录不可写，WebView 缓存暂用系统默认位置：{err}");
        }
    }
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
        // 剪贴板能力（复制/剪切/粘贴走 Rust 侧，避免 WebView 的剪贴板权限弹窗）
        .plugin(tauri_plugin_clipboard_manager::init())
        // 窗口级图标：显式设置（任务栏小图标 + Alt-Tab 大图标均可见）。
        // 优先使用 128px 图标（缩放到各尺寸更清晰），解码失败回退构建期内置图标。
        .setup(|app| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png"))
                    .ok()
                    .or_else(|| app.default_window_icon().cloned());
                match icon {
                    Some(icon) => {
                        if let Err(err) = window.set_icon(icon) {
                            log::warn!(target: "sread::main", "设置窗口图标失败：{err}");
                        }
                    }
                    None => {
                        log::warn!(target: "sread::main", "未找到可用窗口图标（检查 bundle.icon 配置）");
                    }
                }
                // 冷启动兜底：窗口初始隐藏（visible=false），前端首帧就绪后自行显示；
                // 若 8 秒后仍未显示（前端异常），强制显示，避免不可见的僵尸进程。
                let fallback = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(8));
                    if matches!(fallback.is_visible(), Ok(false)) {
                        let _ = fallback.show();
                    }
                });
            }
            Ok(())
        })
        // 运行状态：打开标签集合（由命令层以 Mutex 访问）
        .manage(Mutex::new(AppState::new()))
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::data_dir_status,
            commands::set_data_dir,
            commands::get_settings,
            commands::save_settings,
            commands::get_default_shortcuts,
            commands::get_history,
            commands::remove_history,
            commands::clear_history,
            commands::update_history_progress,
            commands::get_session,
            commands::save_session,
            commands::open_file,
            commands::get_rows,
            commands::set_encoding,
            commands::list_encodings,
            commands::list_tabs,
            commands::close_tab,
            commands::set_active_tab,
            commands::reorder_tab,
            commands::open_settings,
            commands::take_settings_tab,
            commands::toggle_edit,
            commands::apply_edits,
            commands::undo_edit,
            commands::redo_edit,
            commands::save_tab,
            commands::save_tab_as,
            commands::reload_tab,
            commands::find_in_edit,
            commands::replace_in_edit,
            commands::replace_all_in_edit
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 应用启动失败");
}
