// S-Read-TXT 启动入口（薄层）：应用状态托管、日志初始化与 Tauri 装配。
// 业务逻辑位于 `s_read_txt` 库；IPC 命令位于 `commands` 模块。

// 始终隐藏 Windows 控制台窗口：
// - 发布构建：不弹控制台；
// - 调试构建：同样隐藏（此前 debug 会弹一个空白控制台窗口，易被误认为程序故障）；
//   调试期日志一律走 data/logs 文件日志与 CDP 验证（SRT_LOG_LEVEL 控制级别）。
#![windows_subsystem = "windows"]

mod commands;
mod tab_drag;

use std::sync::Mutex;

use s_read_txt::app_state::AppState;
use s_read_txt::compare::CompareState;
use s_read_txt::logging;
use s_read_txt::session::store as session_store;
use s_read_txt::settings::store as settings_store;
use s_read_txt::settings::theme;
use s_read_txt::storage::data_dir;
use s_read_txt::storage::paths;
use s_read_txt::window_registry::LastFocused;
use tauri::Manager;

/// WebView2 启动附加参数（内存优化 + 离线加固；决策与实测见 docs/plan/progress.md）。
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

/// 主题令牌 → 窗口背景色（窗口显示瞬间的底色，避免明暗闪烁）。
///
/// 取解析后主题的 `base` 令牌（`#RRGGBB`）；解析失败回退浅色 `#FAF9F7`。
fn theme_background_color(theme: &theme::ResolvedTheme) -> tauri::window::Color {
    let fallback = tauri::window::Color(0xFA, 0xF9, 0xF7, 0xFF);
    let Some(hex) = theme
        .tokens
        .get("base")
        .and_then(|value| value.strip_prefix('#'))
    else {
        return fallback;
    };
    if hex.len() < 6 {
        return fallback;
    }
    let channel = |start: usize| u8::from_str_radix(&hex[start..start + 2], 16).ok();
    match (channel(0), channel(2), channel(4)) {
        (Some(r), Some(g), Some(b)) => tauri::window::Color(r, g, b, 0xFF),
        _ => fallback,
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
    // （避免 WebView 初始化失败；已接入「选择可写目录」引导）。
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
    // 解析失败（含用户主题缺失）回退跟随系统，保证启动不受阻。
    let startup_theme = theme::resolve_theme(
        &startup_dir,
        &settings_store::load_reader_settings(&startup_dir).theme_id,
    )
    .ok();
    if let Err(err) = logging::init(&startup_dir, startup_settings.log_level) {
        eprintln!("[s-read-txt] 日志初始化失败（应用继续运行）：{err}");
    }
    // 崩溃日志开关（设置 `app.system.crashLog`；修改后下次启动生效）
    logging::install_panic_hook(&startup_dir, startup_settings.system.crash_log);
    log::info!(
        target: "sread::main",
        "S-Read-TXT v{} 启动（数据目录：{}）",
        env!("CARGO_PKG_VERSION"),
        startup_dir.display()
    );
    log::info!(target: "sread::main", "权限检查：{access_outcome:?}");

    // 配置版本迁移：老版配置文件升级到当前 schema（先备份、失败保留原文件）。
    for (name, status) in s_read_txt::settings::migrate::migrate_all(&startup_dir) {
        log::info!(target: "sread::main", "配置迁移检查：{name} → {status:?}");
    }

    // 迁移后的原目录延迟清理（上次迁移时被占用；此刻 WebView2 尚未启动）。
    if let Some(old_dir) = s_read_txt::storage::migrate_dir::cleanup_pending(&startup_dir) {
        log::info!(target: "sread::main", "迁移残留已清理：{}", old_dir.display());
    }

    let app_result = tauri::Builder::default()
        // 单实例：第二次启动只把文件参数转发给已运行实例并退出。
        // 必须最先注册（官方要求）；同时避免多实例并写同一便携数据目录。
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let paths = s_read_txt::cli::file_args(argv.iter().skip(1).map(String::as_str));
            if !paths.is_empty() {
                s_read_txt::cli::push_pending(app, paths);
            }
        }))
        // 原生对话框能力（文件选择/目录选择/消息框）
        .plugin(tauri_plugin_dialog::init())
        // 剪贴板能力（复制/剪切/粘贴走 Rust 侧，避免 WebView 的剪贴板权限弹窗）
        .plugin(tauri_plugin_clipboard_manager::init())
        // 打开链接/定位文件（更新检查的「打开发布页」「打开文件位置」）
        .plugin(tauri_plugin_opener::init())
        // 窗口级图标：显式设置（任务栏小图标 + Alt-Tab 大图标均可见）。
        // 优先使用 128px 图标（缩放到各尺寸更清晰），解码失败回退构建期内置图标。
        .setup(move |app| {
            use tauri::{Manager, PhysicalPosition, PhysicalSize};
            // 启动参数中的文件路径入队（前端挂载后经 take_cli_files 取走打开）。
            let startup_files = s_read_txt::cli::file_args(std::env::args().skip(1));
            if !startup_files.is_empty() {
                s_read_txt::cli::push_pending(app.handle(), startup_files);
            }
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
                // 会话缺失/损坏时保持 tauri.conf.json 默认值（居中 1100×760）；
                // 启动行为设置可关闭恢复（关闭后使用默认几何）。
                let restore_window =
                    s_read_txt::settings::store::load_app_settings(&startup_dir)
                        .startup
                        .restore_window;
                let window_state = if restore_window {
                    session_store::load_window(&startup_dir, "main").window
                } else {
                    log::info!(target: "sread::main", "启动设置：不恢复窗口几何（使用默认）");
                    Default::default()
                };
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
                let color = startup_theme
                    .as_ref()
                    .map(theme_background_color)
                    .unwrap_or(tauri::window::Color(0xFA, 0xF9, 0xF7, 0xFF));
                if let Err(err) = window.set_background_color(Some(color)) {
                    log::warn!(target: "sread::main", "设置主题背景色失败：{err}");
                }
                match window.show() {
                    Ok(()) => log::info!(target: "sread::main", "主窗口已显示"),
                    Err(err) => log::warn!(target: "sread::main", "显示主窗口失败：{err}"),
                }
            }
            // 多窗口会话恢复：按会话记录重建其余主窗口（各窗口前端自行恢复自己的标签切片）。
            // 总开关关闭时仍按「窗口布局」细项恢复窗口数量与几何（不恢复标签内容）。
            // 上限来自设置 `app.startup.maxWindows`（手改会话写入超量窗口时同样受控）。
            let max_restore_windows = startup_settings.startup.max_windows as usize;
            let restore_layout = startup_settings.startup.restore_items.layout;
            if startup_settings.startup.restore_session || restore_layout {
                let session = session_store::load(&startup_dir);
                let focused_label = session.focused_label.clone();
                let extras: Vec<_> = session
                    .windows
                    .into_iter()
                    .filter(|entry| {
                        entry.label.starts_with("main-")
                            && entry.label.len() > 5
                            && entry
                                .label
                                .chars()
                                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
                    })
                    // 上限为「窗口总数（含主窗口）」：额外恢复数 = 上限 − 1
                    .take(max_restore_windows.saturating_sub(1))
                    .collect();
                if !extras.is_empty() {
                    let app_handle = app.handle().clone();
                    let restore_geometry = startup_settings.startup.restore_window;
                    tauri::async_runtime::spawn(async move {
                        for entry in extras {
                            let (position, width, height, maximized) = if restore_geometry {
                                (
                                    match (entry.window.x, entry.window.y) {
                                        (Some(x), Some(y)) => Some((x, y)),
                                        _ => None,
                                    },
                                    f64::from(entry.window.width),
                                    f64::from(entry.window.height),
                                    entry.window.maximized,
                                )
                            } else {
                                (None, 1100.0, 760.0, false)
                            };
                            match commands::build_main_window(
                                &app_handle,
                                &entry.label,
                                position,
                                width,
                                height,
                                maximized,
                            ) {
                                Ok(_) => {
                                    log::info!(target: "sread::main", "会话恢复窗口：{}", entry.label)
                                }
                                Err(err) => log::warn!(
                                    target: "sread::main",
                                    "会话恢复窗口 {} 失败：{}",
                                    entry.label,
                                    err.message
                                ),
                            }
                        }
                        if let Some(label) = focused_label.as_deref() {
                            if let Some(window) = app_handle.get_webview_window(label) {
                                if let Err(err) = window.set_focus() {
                                    log::warn!(target: "sread::main", "恢复窗口焦点失败：{err}");
                                }
                            }
                        }
                    });
                }
            }
            Ok(())
        })
        // 干净退出标记（多窗口并发收尾的安全网）：
        // 「退出所有窗口」协议下每扇窗各自收尾，前端查询剩余窗口数存在竞态
        // （两窗会同时看到对方而都不写标记）；这里在主窗口销毁事件里裁决：
        // 当销毁的是主窗口（`main` / `main-*`）且已无任何窗口时写入标记。
        // 进程被强杀（taskkill /F）不会触发销毁事件，标记保持缺失，崩溃恢复照常。
        .on_window_event(|window, event| {
            let label = window.label();
            // 焦点追踪：会话保存写入 focusedLabel，启动时激活最后使用的窗口。
            if let tauri::WindowEvent::Focused(true) = event {
                if label == "main" || label.starts_with("main-") {
                    window.app_handle().state::<LastFocused>().set(label);
                }
                return;
            }
            if !matches!(event, tauri::WindowEvent::Destroyed) {
                return;
            }
            // 比较窗口关闭：释放装载的文档（mmap 会话与行哈希）并修剪工作集
            if label == s_read_txt::compare::COMPARE_WINDOW {
                window.app_handle().state::<CompareState>().clear();
                s_read_txt::mem::trim_working_set();
                return;
            }
            if label != "main" && !label.starts_with("main-") {
                return;
            }
            let app = window.app_handle();
            let remaining = app
                .webview_windows()
                .keys()
                .filter(|other| other.as_str() != label)
                .count();
            if remaining > 0 {
                return;
            }
            let (dir, _origin) = paths::resolve_data_dir();
            if let Err(err) = s_read_txt::snapshots::mark_clean_exit(&dir) {
                log::warn!(target: "sread::main", "写入干净退出标记失败：{err}");
            }
        })
        // 运行状态：打开标签集合（由命令层以 Mutex 访问）
        .manage(Mutex::new(AppState::new()))
        // 最后聚焦主窗口追踪（会话 focusedLabel）
        .manage(LastFocused::default())
        // 多窗口退出协调器（两阶段：请求 → 全部就绪 → 放行）
        .manage(s_read_txt::quit::QuitState::default())
        .manage(tab_drag::TabDragState::default())
        // 命令行/单实例待打开队列
        .manage(s_read_txt::cli::PendingCliFiles::default())
        // 比较/合并窗口文档状态
        .manage(CompareState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::data_dir_status,
            commands::take_cli_files,
            commands::new_window,
            commands::forget_window_session,
            commands::set_data_dir,
            commands::get_settings,
            commands::save_settings,
            commands::export_settings,
            commands::import_settings,
            commands::reset_settings,
            commands::get_settings_registry,
            commands::export_shortcuts,
            commands::import_shortcuts,
            commands::get_disk_usage,
            commands::clear_cache,
            commands::privacy_usage,
            commands::privacy_clear,
            commands::check_update,
            commands::download_update,
            commands::reveal_update_file,
            commands::open_update_page,
            commands::migrate_data_dir,
            commands::restart_app,
            commands::set_background_file,
            commands::clear_background_file,
            commands::read_background_image,
            commands::list_themes,
            commands::get_theme,
            commands::import_theme,
            commands::save_theme,
            commands::export_theme,
            commands::remove_theme,
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
            commands::preview_batch_numbering,
            commands::apply_batch_numbering,
            commands::list_clipboard_history,
            commands::add_clipboard_entry,
            commands::remove_clipboard_entry,
            commands::clear_clipboard_history,
        commands::count_matches_in_edit,
        commands::list_find_history,
        commands::add_find_history,
        commands::clear_find_history,
            commands::preview_line_op,
            commands::apply_line_op,
            commands::filter_rows,
        commands::outline_items,
        commands::list_snapshots,
        commands::create_snapshot,
        commands::restore_snapshot,
        commands::delete_snapshot,
        commands::mark_clean_exit,
        commands::take_crash_flag,
        commands::preview_split,
        commands::apply_split,
        commands::scan_rename_dir,
        commands::preview_rename,
        commands::apply_rename,
        commands::undo_rename,
        commands::read_rename_log,
        commands::open_compare_window,
        commands::take_compare_request,
        commands::diff_docs,
        commands::merge3_docs,
        commands::compare_rows,
        commands::merge_rows,
        commands::write_merge_output,
        commands::undo_merge_writeback,
        commands::new_file,
        commands::export_text,
        commands::print_document,
        commands::fold_regions,
            commands::fetch_rows_at,
            commands::list_encodings,
            commands::list_tabs,
            commands::close_tab,
            commands::close_window_tabs,
            commands::main_window_count,
            commands::begin_quit_all,
            commands::report_quit_ready,
            commands::report_quit_cancel,
            commands::set_active_tab,
            commands::set_tab_color,
            commands::list_windows,
            commands::move_tab_to_window,
            commands::move_tab_to_pane,
            tab_drag::begin_tab_drag,
            tab_drag::drag_move,
            tab_drag::drag_end,
            commands::reorder_tab,
            commands::open_settings,
            commands::take_settings_tab,
    commands::list_fonts,
    commands::import_font,
    commands::remove_font,
    commands::read_font_data,
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
            commands::match_window_in_edit,
            commands::search_workspace,
            commands::replace_workspace,
            commands::document_stats,
            commands::edit_display_pos,
            commands::list_annotations,
            commands::add_bookmark,
            commands::remove_bookmark,
            commands::add_highlight,
            commands::remove_highlight,
            commands::add_note,
            commands::update_note,
            commands::remove_note,
            commands::clear_annotations,
            commands::selection_stats,
            commands::get_reading_stats,
            commands::add_reading_seconds,
            commands::convert_eol,
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
