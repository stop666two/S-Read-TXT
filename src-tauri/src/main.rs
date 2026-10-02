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
use s_read_txt::session::store as session_store;
use s_read_txt::settings::reader::Theme;
use s_read_txt::settings::store as settings_store;
use s_read_txt::storage::data_dir;
use s_read_txt::storage::paths;

/// WebView2 启动附加参数（内存优化 + 离线加固；决策与实测见 docs/plan/progress.md 阶段 9）。
///
/// 依据 2026-10-02 实测（10×100MiB 标签、专用工作集口径）：
/// - 基线（独立 GPU 进程 + 默认特性）97.7MiB → 本组合 81.9MiB（-15.8MiB）；
/// - `--in-process-gpu`：GPU 服务并入浏览器进程（图片光栅/合成加速保留，
///   未来 EPUB 内嵌图片不受影响；代价是 GPU 异常会连带浏览器进程重启）；
/// - 禁用特性：系统拼写检查、原生遮挡计算、Office/PDF 弹层等本应用不需要的组件；
/// - 禁用后台联网/组件更新/扩展/同步：与「完全离线」承诺一致，减少后台活动。
///
/// 外部已显式传入 `--in-process-gpu`（测试变体/高级用户手改）时保持原样。
const WEBVIEW2_EXTRA_ARGS: &str = "--in-process-gpu \
--disable-features=WinUseBrowserSpellChecker,CalculateNativeWinOcclusion,msWebOOUI,msPdfOOUI \
--disable-background-networking --disable-component-update --disable-extensions --disable-sync";

/// 合并调用方已有的 WebView2 参数（如 `--remote-debugging-port` 调试端口）并追加本应用默认项。
///
/// 必须在 Tauri Builder 之前调用：WebView2Loader 在创建 WebView2 环境时读取
/// `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`，创建之后再设置不会生效。
fn configure_webview2_extra_args() {
    let existing = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
    if existing.contains("--in-process-gpu") {
        return;
    }
    let merged = if existing.trim().is_empty() {
        WEBVIEW2_EXTRA_ARGS.to_string()
    } else {
        format!("{existing} {WEBVIEW2_EXTRA_ARGS}")
    };
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", merged);
}

/// 主题 → 窗口背景色（窗口显示瞬间的底色，避免白色/黑色闪烁）。
///
/// - `system`：读取系统明暗（窗口查询失败回退浅色）；
/// - 护眼：米黄底。
/// 与 `src/styles/base.css` 的主题令牌保持一致（浅 `#FAF9F7` / 深 `#1E1E1E` / 护眼 `#F5EFE0`）。
fn theme_background_color(theme: Theme, window: &tauri::WebviewWindow) -> tauri::window::Color {
    let dark = match theme {
        Theme::Dark => true,
        Theme::Eye => return tauri::window::Color(0xF5, 0xEF, 0xE0, 0xFF),
        Theme::Light => false,
        Theme::System | Theme::Unknown => window
            .theme()
            .map(|value| value == tauri::Theme::Dark)
            .unwrap_or(false),
    };
    if dark {
        tauri::window::Color(0x1E, 0x1E, 0x1E, 0xFF)
    } else {
        tauri::window::Color(0xFA, 0xF9, 0xF7, 0xFF)
    }
}

fn main() {
    // 提权助手模式最先处理：只创建数据目录并授予当前用户修改权限后立即退出，
    // 绝不进入 Tauri/WebView2（整体提权运行会破坏 WebView2 子进程的数据目录读写，
    // 详见 elevation.rs 模块文档）。
    if let Some(code) = s_read_txt::elevation::maybe_run_prepare_mode() {
        std::process::exit(code);
    }
    // WebView2 附加参数必须在任何 WebView 创建之前设置（含内存策略与离线加固项）。
    configure_webview2_extra_args();
    // 日志先行：级别来源 SRT_LOG_LEVEL > settings.json 的 logLevel > 默认 info；
    // 日志初始化失败不阻塞应用（降级为无文件日志）。
    let (startup_dir, _origin) = paths::resolve_data_dir();
    // 管理员权限按需使用：仅当数据目录不可写（如按机器安装到 Program Files）且当前非管理员时，
    // 弹一次 UAC，由助手进程完成「创建目录 + 授权当前用户」后立即退出；应用自身始终以普通权限
    // 运行（WebView2 不受提权影响），此后启动不再提示。用户取消 UAC 或修复失败时，
    // 回落到前端「数据目录引导」对话框流程（选择可写目录 / 只读运行）。
    let access_outcome = s_read_txt::elevation::ensure_data_dir_access(&startup_dir);
    // WebView2 用户数据目录重定向到便携 data/webview（默认写 %LOCALAPPDATA%，
    // 违反「数据全部在程序目录」红线；`WEBVIEW2_USER_DATA_FOLDER` 由 WebView2Loader
    // 在创建环境时读取，必须在 Builder 之前设置）。目录不可写时暂不重定向
    // （避免 WebView 初始化失败；阶段 8 已接入「选择可写目录」引导）。
    match data_dir::probe_writable(&startup_dir) {
        Ok(()) => {
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", startup_dir.join("webview"));
        }
        Err(err) => {
            eprintln!("[s-read-txt] 数据目录不可写，WebView 缓存暂用系统默认位置：{err}");
        }
    }
    let startup_settings = settings_store::load_app_settings(&startup_dir);
    // 主题属于阅读排版配置（reader.json）；窗口显示前需要它来决定背景色。
    let startup_theme = settings_store::load_reader_settings(&startup_dir).theme;
    if let Err(err) = logging::init(&startup_dir, startup_settings.log_level) {
        eprintln!("[s-read-txt] 日志初始化失败（应用继续运行）：{err}");
    }
    log::info!(
        target: "sread::main",
        "S-Read-TXT v{} 启动（数据目录：{}）",
        env!("CARGO_PKG_VERSION"),
        startup_dir.display()
    );
    log::info!(target: "sread::main", "权限检查：{access_outcome:?}");

    let app_result = tauri::Builder::default()
        // 原生对话框能力（文件选择/目录选择/消息框）
        .plugin(tauri_plugin_dialog::init())
        // 剪贴板能力（复制/剪切/粘贴走 Rust 侧，避免 WebView 的剪贴板权限弹窗）
        .plugin(tauri_plugin_clipboard_manager::init())
        // 窗口级图标：显式设置（任务栏小图标 + Alt-Tab 大图标均可见）。
        // 优先使用 128px 图标（缩放到各尺寸更清晰），解码失败回退构建期内置图标。
        .setup(move |app| {
            use tauri::{Manager, PhysicalPosition, PhysicalSize};
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
                // 窗口几何恢复：在显示之前应用（避免先显示再跳动的观感）；
                // 会话缺失/损坏时保持 tauri.conf.json 默认值（居中 1100×760）。
                let window_state = session_store::load(&startup_dir).window;
                if window_state.maximized {
                    if let Err(err) = window.maximize() {
                        log::warn!(target: "sread::main", "恢复窗口最大化失败：{err}");
                    }
                } else {
                    if let (Some(x), Some(y)) = (window_state.x, window_state.y) {
                        if let Err(err) = window.set_position(PhysicalPosition::new(x, y)) {
                            log::warn!(target: "sread::main", "恢复窗口位置失败：{err}");
                        }
                    }
                    if let Err(err) =
                        window.set_size(PhysicalSize::new(window_state.width, window_state.height))
                    {
                        log::warn!(target: "sread::main", "恢复窗口尺寸失败：{err}");
                    }
                }
                // 启动显示策略（维护者确认「立即显示 + 内置占位」）：
                // 先设主题背景色并立即显示窗口（HTML 内置占位随后接替），
                // 避免弱磁盘/冷缓存下长时间空白等待；前端首帧就绪后会归还键盘焦点。
                let color = theme_background_color(startup_theme, &window);
                if let Err(err) = window.set_background_color(Some(color)) {
                    log::warn!(target: "sread::main", "设置主题背景色失败：{err}");
                }
                match window.show() {
                    Ok(()) => log::info!(target: "sread::main", "主窗口已显示"),
                    Err(err) => log::warn!(target: "sread::main", "显示主窗口失败：{err}"),
                }
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
            commands::replace_all_in_edit,
            commands::preview_replace_all_in_edit,
            commands::apply_replace_all_in_edit,
            commands::match_window_in_edit
        ])
        .run(tauri::generate_context!());
    // 启动失败不再无声退出：写日志 + 弹原生错误框（窗口子系统下无控制台，用户需可见反馈）。
    if let Err(err) = app_result {
        log::error!(target: "sread::main", "应用启动失败：{err}");
        s_read_txt::elevation::show_fatal_error(&format!(
            "S-Read-TXT 启动失败：{err}\n\n详情见程序目录 data/logs/app.log；\n若为「所有用户」安装，请允许启动时的一次管理员权限请求以初始化数据目录。"
        ));
        std::process::exit(1);
    }
}
