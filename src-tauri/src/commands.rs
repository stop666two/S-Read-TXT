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
use tauri::{Emitter, Manager, State};

use s_read_txt::annotations::{FileAnnotations, NoteKind};
use s_read_txt::app_state::{default_pane, is_pane_of, AppState, RowsPayload, TabInfo};
use s_read_txt::background::{self, BackgroundEntry};
use s_read_txt::clipboard_history as clipboard_store;
use s_read_txt::clipboard_history::ClipboardEntry;
use s_read_txt::compare::{self, CompareRequest, CompareSide, CompareState};
use s_read_txt::fonts::{self, FontEntry};
use s_read_txt::history::entry::HistoryEntry;
use s_read_txt::history::store as history_store;
use s_read_txt::ipc_error::{
    IpcError, CODE_BACKGROUND_INVALID, CODE_CONFIG_SAVE, CODE_FILE_TOO_LARGE, CODE_HISTORY_SAVE,
    CODE_INVALID_ENCODING, CODE_INVALID_EOL, CODE_INVALID_POSITION, CODE_INVALID_REGEX,
    CODE_INVALID_SCOPE, CODE_IO, CODE_MIGRATE_FAILED, CODE_PRINT_TOO_LARGE, CODE_RENAME_INVALID,
    CODE_SESSION_SAVE, CODE_SETTINGS_EXPORT, CODE_SETTINGS_IMPORT, CODE_SETTINGS_RESET,
    CODE_SNAPSHOT_INVALID, CODE_SPLIT_INVALID, CODE_TAB_NOT_FOUND, CODE_THEME_INVALID,
};
use s_read_txt::logging;
use s_read_txt::logging::context::{with_context, LogContext};
use s_read_txt::outline::{FoldRegion, OutlineItem};
use s_read_txt::resources;
use s_read_txt::session::model::WindowSession;
use s_read_txt::session::store as session_store;
use s_read_txt::settings::registry::{self, SettingSpec};
use s_read_txt::settings::reset::{self as settings_reset, ResetScope};
use s_read_txt::settings::store as settings_store;
use s_read_txt::settings::theme::{self, ResolvedTheme, ThemeSummary};
use s_read_txt::settings::{bundle, shortcut_io, SettingsSaveRequest, SettingsSnapshot};
use s_read_txt::snapshots::SnapshotInfo;
use s_read_txt::storage::data_dir;
use s_read_txt::storage::migrate_dir::{self as migrate_dir, MigrationReport};
use s_read_txt::storage::paths::{self, DataDirOrigin};
use s_read_txt::textfile::editing::batch::{
    BatchNumberingConfig, BatchNumberingOutcome, BatchPreview,
};
use s_read_txt::textfile::editing::edit_doc::{EditApplied, EditOp};
use s_read_txt::textfile::editing::line_ops::{LineOpConfig, LineOpOutcome, LineOpPreview};
use s_read_txt::textfile::editing::search::{
    FindHit, ReplaceAllOutcome, ReplaceNextOutcome, ReplacePreview, SearchMode, PREVIEW_LIST_CAP,
};
use s_read_txt::textfile::encoding::FileEncoding;
use s_read_txt::time_util;
use s_read_txt::window_registry::LastFocused;

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

/// 标签表变更广播事件（跨窗口移动/拖放后，各窗口刷新自身视图）。
pub const EVENT_TABS_CHANGED: &str = "srt://tabs-changed";

/// 获取应用状态锁（中毒视为内部错误）。
pub(crate) fn lock_state<'a>(
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

/// 取走命令行/单实例转发的待打开文件列表。
#[tauri::command]
pub fn take_cli_files(pending: State<'_, s_read_txt::cli::PendingCliFiles>) -> Vec<String> {
    s_read_txt::cli::take_pending(&pending)
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

/// 命令：迁移数据目录到指定位置（复制校验 → 写指针 → 清理原目录）。
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

/// 命令：预览批量序号（仅编辑标签；格式超限/范围非法在预览时即报错）。
#[tauri::command]
pub fn preview_batch_numbering(
    tab_id: u64,
    config: BatchNumberingConfig,
    state: State<'_, Mutex<AppState>>,
) -> Result<BatchPreview, IpcError> {
    with_context(LogContext::request(), || {
        let app_state = lock_state(&state)?;
        Ok(app_state.preview_batch_numbering(tab_id, &config)?)
    })
}

/// 命令：执行批量序号（单次编辑 = 单撤销步；仅编辑标签）。
#[tauri::command]
pub fn apply_batch_numbering(
    tab_id: u64,
    config: BatchNumberingConfig,
    state: State<'_, Mutex<AppState>>,
) -> Result<BatchNumberingOutcome, IpcError> {
    with_context(LogContext::request(), || {
        let mut app_state = lock_state(&state)?;
        Ok(app_state.apply_batch_numbering(tab_id, &config)?)
    })
}

/// 命令：预览行操作（仅编辑标签）。
#[tauri::command]
pub fn preview_line_op(
    tab_id: u64,
    config: LineOpConfig,
    state: State<'_, Mutex<AppState>>,
) -> Result<LineOpPreview, IpcError> {
    with_context(LogContext::request(), || {
        let app_state = lock_state(&state)?;
        Ok(app_state.preview_line_op(tab_id, &config)?)
    })
}

/// 命令：执行行操作（单次编辑 = 单撤销步；仅编辑标签）。
#[tauri::command]
pub fn apply_line_op(
    tab_id: u64,
    config: LineOpConfig,
    state: State<'_, Mutex<AppState>>,
) -> Result<LineOpOutcome, IpcError> {
    with_context(LogContext::request(), || {
        let mut app_state = lock_state(&state)?;
        Ok(app_state.apply_line_op(tab_id, &config)?)
    })
}

use s_read_txt::textfile::filter::{FilterQuery, FilterResult};

/// 折叠区间：按显示设置中的折叠方式计算。
#[tauri::command]
pub fn fold_regions(
    tab_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<FoldRegion>, IpcError> {
    with_context(LogContext::request(), || {
        let settings = current_app_settings();
        let patterns = if settings.display.outline_patterns.is_empty() {
            s_read_txt::settings::defaults::DEFAULT_OUTLINE_PATTERNS
                .iter()
                .map(|pattern| (*pattern).to_string())
                .collect()
        } else {
            settings.display.outline_patterns.clone()
        };
        let app_state = lock_state(&state)?;
        let regions = app_state.fold_regions(tab_id, settings.display.folding, &patterns)?;
        log::debug!(target: "sread::commands", "fold_regions: tab={tab_id} mode={:?} regions={}", settings.display.folding, regions.len());
        Ok(regions)
    })
}

/// 大纲提取：按显示设置中的可编辑正则扫描当前标签文档。
#[tauri::command]
pub fn outline_items(
    tab_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<OutlineItem>, IpcError> {
    with_context(LogContext::request(), || {
        let settings = current_app_settings();
        let patterns = if settings.display.outline_patterns.is_empty() {
            s_read_txt::settings::defaults::DEFAULT_OUTLINE_PATTERNS
                .iter()
                .map(|pattern| (*pattern).to_string())
                .collect()
        } else {
            settings.display.outline_patterns.clone()
        };
        let app_state = lock_state(&state)?;
        let items = app_state.outline_items(tab_id, &patterns)?;
        log::debug!(target: "sread::commands", "outline_items: tab={tab_id} items={}", items.len());
        Ok(items)
    })
}

/// 命令：列举快照（版本历史；新→旧）。
#[tauri::command]
pub fn list_snapshots(
    tab_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<SnapshotInfo>, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let app_state = lock_state(&state)?;
        Ok(app_state.list_snapshots(tab_id, &dir)?)
    })
}

/// 命令：创建快照（手动与定时器共用；版本历史关闭时拒绝）。
#[tauri::command]
pub fn create_snapshot(
    tab_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Option<SnapshotInfo>, IpcError> {
    with_context(LogContext::request(), || {
        let settings = current_app_settings();
        if !settings.file.version_history {
            return Err(IpcError::new(CODE_SNAPSHOT_INVALID, "版本历史已关闭"));
        }
        let (dir, _origin) = paths::resolve_data_dir();
        let mut app_state = lock_state(&state)?;
        Ok(app_state.create_snapshot(
            tab_id,
            &dir,
            settings.file.snapshot_keep,
            settings.file.snapshot_max_mb,
        )?)
    })
}

/// 命令：恢复快照（编辑态；单撤销步）。
#[tauri::command]
pub fn restore_snapshot(
    tab_id: u64,
    name: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<EditApplied, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let mut app_state = lock_state(&state)?;
        Ok(app_state.restore_snapshot(tab_id, &dir, &name)?)
    })
}

/// 命令：删除快照（幂等）。
#[tauri::command]
pub fn delete_snapshot(
    tab_id: u64,
    name: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<bool, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let mut app_state = lock_state(&state)?;
        Ok(app_state.delete_snapshot(tab_id, &dir, &name)?)
    })
}

/// 命令：写入「干净退出」标记（退出路径调用）。
#[tauri::command]
pub fn mark_clean_exit() -> Result<(), IpcError> {
    let (dir, _origin) = paths::resolve_data_dir();
    s_read_txt::snapshots::mark_clean_exit(&dir)
        .map_err(|err| IpcError::new(CODE_IO, format!("写入退出标记失败：{err}")))
}

/// 命令：消费「干净退出」标记；返回 `true` 表示上次异常退出**且存在可恢复快照**。
#[tauri::command]
pub fn take_crash_flag() -> bool {
    let (dir, _origin) = paths::resolve_data_dir();
    let crashed = !s_read_txt::snapshots::take_clean_exit(&dir);
    crashed && s_read_txt::snapshots::has_any(&dir)
}

/// 拆分预览返回体（含实际输出目录）。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPreviewDto {
    #[serde(flatten)]
    plan: s_read_txt::split::SplitPlan,
    out_dir: String,
}

/// 拆分错误 → IPC 错误（模式非法/无输出/过多 → SPLIT_INVALID；正则 → INVALID_REGEX）。
fn split_ipc_error(err: s_read_txt::split::SplitError) -> IpcError {
    use s_read_txt::split::SplitError;
    match err {
        SplitError::InvalidPattern(reason) => {
            IpcError::new(CODE_INVALID_REGEX, format!("正则表达式无效：{reason}"))
        }
        SplitError::TooLarge(bytes) => IpcError::new(
            CODE_FILE_TOO_LARGE,
            format!("文件过大（{bytes} 字节），无法拆分"),
        ),
        SplitError::Io(err) => IpcError::new(CODE_IO, format!("读写失败：{err}")),
        other => IpcError::new(CODE_SPLIT_INVALID, other.to_string()),
    }
}

/// 解析拆分输出目录：显式指定，缺省为源文件所在目录。
fn resolve_split_out_dir(path: &std::path::Path, out_dir: Option<String>) -> std::path::PathBuf {
    out_dir
        .filter(|value| !value.trim().is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| path.parent().map(std::path::Path::to_path_buf))
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

/// 命令：拆分预览（不写盘）。
#[tauri::command]
pub fn preview_split(
    path: String,
    mode: s_read_txt::split::SplitMode,
    out_dir: Option<String>,
) -> Result<SplitPreviewDto, IpcError> {
    let source = std::path::PathBuf::from(&path);
    let resolved = resolve_split_out_dir(&source, out_dir);
    let plan =
        s_read_txt::split::plan_file(&source, &mode, Some(&resolved)).map_err(split_ipc_error)?;
    Ok(SplitPreviewDto {
        plan,
        out_dir: resolved.to_string_lossy().into_owned(),
    })
}

/// 命令：执行拆分（流式 + 原子写；同名覆盖）。
#[tauri::command]
pub fn apply_split(
    path: String,
    mode: s_read_txt::split::SplitMode,
    out_dir: Option<String>,
) -> Result<s_read_txt::split::AppliedSplit, IpcError> {
    let source = std::path::PathBuf::from(&path);
    let resolved = resolve_split_out_dir(&source, out_dir);
    s_read_txt::split::apply_file(&source, &mode, Some(&resolved)).map_err(split_ipc_error)
}

/// 命令：打开比较/合并窗口（已存在则更新请求、聚焦并通知重载）。
#[tauri::command]
pub async fn open_compare_window(
    app: tauri::AppHandle,
    request: CompareRequest,
    state: State<'_, CompareState>,
) -> Result<(), IpcError> {
    let mode_title = if request.mode == compare::CompareMode::Merge {
        "合并"
    } else {
        "比较"
    };
    state.set_request(request);
    if let Some(window) = app.get_webview_window(compare::COMPARE_WINDOW) {
        window
            .show()
            .map_err(|err| IpcError::new(CODE_IO, format!("显示{mode_title}窗口失败：{err}")))?;
        window
            .set_focus()
            .map_err(|err| IpcError::new(CODE_IO, format!("聚焦{mode_title}窗口失败：{err}")))?;
        let _ = app.emit_to(compare::COMPARE_WINDOW, compare::EVENT_COMPARE_REQUEST, ());
        return Ok(());
    }
    let window = tauri::WebviewWindowBuilder::new(
        &app,
        compare::COMPARE_WINDOW,
        tauri::WebviewUrl::App("compare.html".into()),
    )
    .title(format!("{mode_title} - S-Read-TXT"))
    .inner_size(1200.0, 800.0)
    .min_inner_size(900.0, 600.0)
    .decorations(false)
    .visible(false)
    .build()
    .map_err(|err| IpcError::new(CODE_IO, format!("打开{mode_title}窗口失败：{err}")))?;
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png"))
        .ok()
        .or_else(|| app.default_window_icon().cloned());
    if let Some(icon) = icon {
        let _ = window.set_icon(icon);
    }
    let (dir, _origin) = paths::resolve_data_dir();
    let theme =
        theme::resolve_theme(&dir, &settings_store::load_reader_settings(&dir).theme_id).ok();
    let color = theme
        .as_ref()
        .map(crate::theme_background_color)
        .unwrap_or(tauri::window::Color(0xFA, 0xF9, 0xF7, 0xFF));
    let _ = window.set_background_color(Some(color));
    window
        .show()
        .map_err(|err| IpcError::new(CODE_IO, format!("显示{mode_title}窗口失败：{err}")))?;
    window
        .set_focus()
        .map_err(|err| IpcError::new(CODE_IO, format!("聚焦{mode_title}窗口失败：{err}")))?;
    Ok(())
}

/// 命令：取走待处理比较请求（比较窗口启动时调用）。
#[tauri::command]
pub fn take_compare_request(state: State<'_, CompareState>) -> Option<CompareRequest> {
    state.take_request()
}

/// 命令：加载双栏比较（两个磁盘文件）。
#[tauri::command]
pub fn diff_docs(
    state: State<'_, CompareState>,
    left: String,
    right: String,
) -> Result<compare::DiffDocsDto, IpcError> {
    Ok(state.load_diff(&left, &right)?)
}

/// 命令：加载三方合并（base / ours / theirs）。
#[tauri::command]
pub fn merge3_docs(
    state: State<'_, CompareState>,
    base: String,
    ours: String,
    theirs: String,
) -> Result<compare::MergeDocsDto, IpcError> {
    Ok(state.load_merge(&base, &ours, &theirs)?)
}

/// 命令：取一侧行文本窗口（虚拟列表按可见区间调用）。
#[tauri::command]
pub fn compare_rows(
    state: State<'_, CompareState>,
    side: CompareSide,
    start: u64,
    count: u32,
) -> Result<Vec<String>, IpcError> {
    Ok(state.rows(side, start, count.min(10_000) as usize)?)
}

/// 命令：取合并输出行（按来源与区间）。
#[tauri::command]
pub fn merge_rows(
    state: State<'_, CompareState>,
    source: s_read_txt::merge3::MergedSource,
    start: u64,
    count: u32,
) -> Result<Vec<String>, IpcError> {
    Ok(state.merge_rows(source, start, count.min(10_000) as usize)?)
}

/// 合并冲突选择项（`region_index` = 冲突序号，0 起，与前端冲突列表顺序一致）。
#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeChoiceDto {
    pub region_index: u32,
    pub choice: String,
}

/// 合并写回结果。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeWriteDto {
    pub target: String,
    pub backup: Option<String>,
    pub bytes: u64,
    pub lines: u64,
}

/// 命令：按当前冲突选择物化合并输出并写回（`makeBackup` 时覆盖前生成 `.bak`）。
#[tauri::command]
pub fn write_merge_output(
    state: State<'_, CompareState>,
    target: String,
    choices: Vec<MergeChoiceDto>,
    make_backup: bool,
) -> Result<MergeWriteDto, IpcError> {
    let pairs: Vec<(u32, String)> = choices
        .into_iter()
        .map(|item| (item.region_index, item.choice))
        .collect();
    let (backup, bytes, lines) = state.write_output(&target, &pairs, make_backup)?;
    log::info!(
        target: "sread::ipc",
        "合并写回：{target}（{lines} 行；备份：{}）",
        backup.clone().unwrap_or_else(|| "无".to_string())
    );
    Ok(MergeWriteDto {
        target,
        backup,
        bytes,
        lines,
    })
}

/// 命令：撤销合并写回（以 `<目标>.bak` 覆盖目标；无备份返回 false）。
#[tauri::command]
pub fn undo_merge_writeback(
    state: State<'_, CompareState>,
    target: String,
) -> Result<bool, IpcError> {
    Ok(state.undo_writeback(&target)?)
}

/// 重命名扫描返回体。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameFileDto {
    name: String,
    size: u64,
}

/// 重命名对（旧名 → 新名）。
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenamePairDto {
    old: String,
    new: String,
}

/// 重命名应用返回体。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameAppliedDto {
    pairs: Vec<RenamePairDto>,
    log_saved: bool,
}

/// 重命名撤销日志（供对话框恢复「撤销本次重命名」）。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameLogDto {
    dir: String,
    pairs: Vec<RenamePairDto>,
    at: u64,
}

/// 重命名扫描上限（超出提示缩小范围）。
const RENAME_SCAN_CAP: usize = 2000;

/// 撤销日志文件名（数据目录内）。
const RENAME_LOG_FILE: &str = "rename-log.json";

/// 重命名错误 → IPC 错误（IO 与非法分类）。
fn rename_ipc_error(err: s_read_txt::rename::RenameError) -> IpcError {
    use s_read_txt::rename::RenameError;
    match err {
        RenameError::Io(_, err) => IpcError::new(CODE_IO, format!("读写失败：{err}")),
        other => IpcError::new(CODE_RENAME_INVALID, other.to_string()),
    }
}

/// 撤销日志路径（数据目录）。
fn rename_log_path() -> std::path::PathBuf {
    let (dir, _origin) = paths::resolve_data_dir();
    dir.join(RENAME_LOG_FILE)
}

/// 当前时间戳（Unix 毫秒）。
fn rename_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// 命令：扫描目录（仅文件、扩展名白名单、自然序、上限 2000）。
#[tauri::command]
pub fn scan_rename_dir(
    dir: String,
    extensions: Option<Vec<String>>,
) -> Result<Vec<RenameFileDto>, IpcError> {
    let base = std::path::PathBuf::from(&dir);
    if !base.is_dir() {
        return Err(IpcError::new(CODE_IO, format!("目录不存在：{dir}")));
    }
    let exts = extensions
        .filter(|list| !list.is_empty())
        .unwrap_or_else(s_read_txt::rename::default_extensions);
    let names =
        s_read_txt::rename::scan(&base, &exts, RENAME_SCAN_CAP).map_err(rename_ipc_error)?;
    let items = names
        .into_iter()
        .map(|name| {
            let size = std::fs::metadata(base.join(&name))
                .map(|meta| meta.len())
                .unwrap_or(0);
            RenameFileDto { name, size }
        })
        .collect();
    Ok(items)
}

/// 命令：重命名预览（纯计算，不触盘）。
#[tauri::command]
pub fn preview_rename(
    dir: String,
    files: Vec<String>,
    rules: s_read_txt::rename::RenameRules,
) -> Result<Vec<s_read_txt::rename::RenameEntry>, IpcError> {
    Ok(s_read_txt::rename::plan(
        std::path::Path::new(&dir),
        &files,
        &rules,
    ))
}

/// 命令：执行重命名（两阶段 + 失败回滚；成功后写撤销日志）。
#[tauri::command]
pub fn apply_rename(dir: String, pairs: Vec<RenamePairDto>) -> Result<RenameAppliedDto, IpcError> {
    let base = std::path::PathBuf::from(&dir);
    let raw: Vec<(String, String)> = pairs
        .iter()
        .map(|pair| (pair.old.clone(), pair.new.clone()))
        .collect();
    s_read_txt::rename::validate_pairs(&base, &raw).map_err(rename_ipc_error)?;
    let applied = s_read_txt::rename::apply(&base, &raw).map_err(rename_ipc_error)?;
    let dto_pairs: Vec<RenamePairDto> = applied
        .into_iter()
        .map(|(old, new)| RenamePairDto { old, new })
        .collect();
    let log = RenameLogDto {
        dir: dir.clone(),
        pairs: dto_pairs.clone(),
        at: rename_now_ms(),
    };
    let saved = match serde_json::to_string_pretty(&log) {
        Ok(text) => {
            s_read_txt::storage::atomic::write_atomic_str(&rename_log_path(), &text).is_ok()
        }
        Err(_) => false,
    };
    Ok(RenameAppliedDto {
        pairs: dto_pairs,
        log_saved: saved,
    })
}

/// 命令：撤销本次重命名（反向改名成功后删除日志）。
#[tauri::command]
pub fn undo_rename(dir: String, pairs: Vec<RenamePairDto>) -> Result<(), IpcError> {
    let base = std::path::PathBuf::from(&dir);
    let raw: Vec<(String, String)> = pairs
        .iter()
        .map(|pair| (pair.old.clone(), pair.new.clone()))
        .collect();
    s_read_txt::rename::undo(&base, &raw).map_err(rename_ipc_error)?;
    let _ = std::fs::remove_file(rename_log_path());
    Ok(())
}

/// 命令：读取撤销日志（无日志或损坏返回 None）。
#[tauri::command]
pub fn read_rename_log() -> Option<RenameLogDto> {
    let text = std::fs::read_to_string(rename_log_path()).ok()?;
    serde_json::from_str(&text).ok()
}

/// 解析调用窗口的目标栏位键：显式 pane 属于本窗口时采用，否则回落默认栏 `#1`。
fn resolve_pane(window: &tauri::WebviewWindow, pane: Option<String>) -> String {
    let label = window.label();
    match pane {
        Some(key) if is_pane_of(&key, label) => key,
        _ => default_pane(label),
    }
}

/// 命令：新建未命名文件：按 `file` 设置生成临时文件并以编辑模式打开。
/// 新标签归属 `pane`（缺省为调用窗口默认栏 `#1`）。
#[tauri::command]
pub fn new_file(
    window: tauri::WebviewWindow,
    pane: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<TabInfo, IpcError> {
    let settings = current_app_settings();
    let (dir, _origin) = paths::resolve_data_dir();
    let owner = resolve_pane(&window, pane);
    lock_state(&state)?
        .open_untitled(&owner, &dir, &settings)
        .map_err(Into::into)
}

/// 构建并显示一个主窗口（「新建窗口」命令与启动多窗口恢复共用）。
///
/// `position`：物理坐标（`None` = 保持系统默认）；`maximized` 在显示前应用。
pub fn build_main_window(
    app: &tauri::AppHandle,
    label: &str,
    position: Option<(i32, i32)>,
    width: f64,
    height: f64,
    maximized: bool,
) -> Result<tauri::WebviewWindow, IpcError> {
    let new_window =
        tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("index.html".into()))
            .title("S-Read-TXT")
            .inner_size(width, height)
            .min_inner_size(720.0, 480.0)
            .decorations(false)
            .visible(false)
            .build()
            .map_err(|err| IpcError::new(CODE_IO, format!("新建窗口失败：{err}")))?;
    if let Some((px, py)) = position {
        if let Err(err) = new_window.set_position(tauri::PhysicalPosition::new(px, py)) {
            log::warn!(target: "sread::ipc", "新建窗口定位失败：{err}");
        }
    }
    // 图标与主题背景（与主窗口启动装配一致；失败不阻塞窗口使用）。
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/128x128.png"))
        .ok()
        .or_else(|| app.default_window_icon().cloned());
    if let Some(icon) = icon {
        let _ = new_window.set_icon(icon);
    }
    let (dir, _origin) = paths::resolve_data_dir();
    let theme =
        theme::resolve_theme(&dir, &settings_store::load_reader_settings(&dir).theme_id).ok();
    let color = theme
        .as_ref()
        .map(crate::theme_background_color)
        .unwrap_or(tauri::window::Color(0xFA, 0xF9, 0xF7, 0xFF));
    let _ = new_window.set_background_color(Some(color));
    if maximized {
        if let Err(err) = new_window.maximize() {
            log::warn!(target: "sread::ipc", "最大化新窗口失败：{err}");
        }
    }
    new_window
        .show()
        .map_err(|err| IpcError::new(CODE_IO, format!("显示新窗口失败：{err}")))?;
    Ok(new_window)
}

/// 命令：新建主窗口。
///
/// 参数（均可选）：`x`/`y` 新窗口左上角**物理坐标**（缺省 = 相对调用窗口级联 +32px）；
/// `width`/`height` 初始内尺寸（缺省 1100×760，与主窗口一致）。
/// 返回：新窗口 label（`main-2`、`main-3`…，取最小可用序号）。
#[tauri::command]
pub async fn new_window(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
) -> Result<String, IpcError> {
    with_context(LogContext::request(), || {
        let label = next_window_label(&app);
        // 初始位置：显式坐标优先；否则相对来源窗口级联偏移 +32px。
        let (init_x, init_y) = match (x, y) {
            (Some(px), Some(py)) => (px.round() as i32, py.round() as i32),
            _ => match window.outer_position() {
                Ok(position) => (position.x + 32, position.y + 32),
                Err(_) => (120, 120),
            },
        };
        build_main_window(
            &app,
            &label,
            Some((init_x, init_y)),
            width.unwrap_or(1100.0),
            height.unwrap_or(760.0),
            false,
        )?;
        log::info!(target: "sread::ipc", "已新建窗口：{label}");
        Ok(label)
    })
}

/// 生成下一个可用的主窗口 label（`main-2`、`main-3`…；已存在则递增）。
pub(crate) fn next_window_label(app: &tauri::AppHandle) -> String {
    use tauri::Manager;
    let existing = app.webview_windows();
    let mut index = 2u32;
    loop {
        let label = format!("main-{index}");
        if !existing.contains_key(&label) {
            return label;
        }
        index += 1;
    }
}

/// 命令：导出当前标签（含未保存编辑）到指定路径；格式由扩展名推断。
#[tauri::command]
pub fn export_text(
    tab_id: u64,
    path: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<u64, IpcError> {
    lock_state(&state)?
        .export_text(tab_id, Path::new(&path))
        .map_err(Into::into)
}

/// 命令：打开打印窗口（data URL 承载转义 HTML，页面加载后自动调起系统打印）。
/// 注意：必须是 async —— 同步命令在主线程执行，调 `run_on_main_thread` 会自锁（实测挂起）。
#[tauri::command]
pub async fn print_document(
    app: tauri::AppHandle,
    tab_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), IpcError> {
    // 自动化测试路径（SRT_PRINT_NO_AUTO）：不自动弹系统打印对话框
    let auto_print = std::env::var_os("SRT_PRINT_NO_AUTO").is_none();
    let html = match lock_state(&state)?.print_html(tab_id, auto_print) {
        Ok(html) => html,
        Err(s_read_txt::app_state::AppStateError::Export(
            s_read_txt::export::ExportError::TooLarge { .. },
        )) => {
            return Err(IpcError::new(
                CODE_PRINT_TOO_LARGE,
                "内容过大，无法打印；请改用「导出」",
            ))
        }
        Err(err) => return Err(err.into()),
    };
    if let Some(existing) = tauri::Manager::get_webview_window(&app, "print-preview") {
        let _ = existing.close();
    }
    let encoded = fonts::base64_encode(html.as_bytes());
    let url = format!("data:text/html;charset=utf-8;base64,{encoded}");
    let parsed = url
        .parse::<tauri::Url>()
        .map_err(|err| IpcError::new(CODE_IO, format!("打印窗口 URL 无效：{err}")))?;
    // 必须在主线程创建窗口：从命令线程同步 build 会阻塞（实测挂起）
    let app_for_window = app.clone();
    app.run_on_main_thread(move || {
        if let Some(existing) = tauri::Manager::get_webview_window(&app_for_window, "print-preview")
        {
            let _ = existing.close();
        }
        let result = tauri::WebviewWindowBuilder::new(
            &app_for_window,
            "print-preview",
            tauri::WebviewUrl::External(parsed),
        )
        .title("S-Read-TXT 打印")
        .inner_size(820.0, 920.0)
        .build();
        if let Err(err) = result {
            log::error!(target: "sread::print", "打印窗口创建失败：{err}");
        }
    })
    .map_err(|err| IpcError::new(CODE_IO, format!("无法调度打印窗口：{err}")))?;
    Ok(())
}

/// 命令：过滤扫描（只读会话；返回命中显示行号供阅读态虚拟化）。
#[tauri::command]
pub fn filter_rows(
    tab_id: u64,
    query: FilterQuery,
    state: State<'_, Mutex<AppState>>,
) -> Result<FilterResult, IpcError> {
    with_context(LogContext::request(), || {
        let app_state = lock_state(&state)?;
        Ok(app_state.filter_rows(tab_id, &query)?)
    })
}

/// 命令：稀疏按行取文本（过滤视图虚拟窗口；单次 ≤512 行）。
#[tauri::command]
pub fn fetch_rows_at(
    tab_id: u64,
    rows: Vec<u64>,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<s_read_txt::textfile::window::RowText>, IpcError> {
    with_context(LogContext::request(), || {
        let app_state = lock_state(&state)?;
        let result = app_state.rows_at(tab_id, &rows)?;
        drop(app_state);
        Ok(result)
    })
}

/// 读取剪贴板历史的有效配置：`(上限, 是否持久化)`（上限 0 = 功能禁用）。
fn clipboard_config() -> (u32, bool) {
    let (dir, _origin) = paths::resolve_data_dir();
    let settings = s_read_txt::settings::store::load_app_settings(&dir);
    (
        settings.editor.clipboard.history_limit,
        settings.editor.clipboard.persist,
    )
}

/// 命令：列出剪贴板历史（最新在前；上限 0 时为空）。
#[tauri::command]
pub fn list_clipboard_history() -> Vec<ClipboardEntry> {
    let (dir, _origin) = paths::resolve_data_dir();
    let (limit, persist) = clipboard_config();
    clipboard_store::list(&dir, persist, limit)
}

/// 命令：记录一条剪贴板文本（空文本与禁用时无操作）。
#[tauri::command]
pub fn add_clipboard_entry(text: String) -> Vec<ClipboardEntry> {
    let (dir, _origin) = paths::resolve_data_dir();
    let (limit, persist) = clipboard_config();
    clipboard_store::add(&dir, persist, limit, &text)
}

/// 命令：删除指定条目（越界无操作）。
#[tauri::command]
pub fn remove_clipboard_entry(index: u32) -> Vec<ClipboardEntry> {
    let (dir, _origin) = paths::resolve_data_dir();
    let (limit, persist) = clipboard_config();
    clipboard_store::remove(&dir, persist, limit, index as usize)
}

/// 命令：清空剪贴板历史。
#[tauri::command]
pub fn clear_clipboard_history() -> Vec<ClipboardEntry> {
    let (dir, _origin) = paths::resolve_data_dir();
    let (_, persist) = clipboard_config();
    clipboard_store::clear(&dir, persist)
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

/// 命令：数据目录磁盘占用分项统计（目录缺失视为全 0）。
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

/// 命令：读取**调用窗口**的会话切片（自愈载入；无记录时返回空切片）。
#[tauri::command]
pub fn get_session(window: tauri::WebviewWindow) -> WindowSession {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        session_store::load_window(&dir, window.label())
    })
}

/// 命令：保存**调用窗口**的会话切片（按窗口合并写入，多窗口互不覆盖）。
/// `focused`：后端记录的最后聚焦主窗口 label，随保存写入。
/// 返回保存后的切片以便前端确认。
#[tauri::command]
pub fn save_session(
    window: tauri::WebviewWindow,
    session: WindowSession,
    focused: State<'_, LastFocused>,
) -> Result<WindowSession, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let label = window.label();
        session_store::save_window(&dir, label, &session, Some(&focused.get()))
            .map_err(|err| IpcError::new(CODE_SESSION_SAVE, format!("保存会话失败：{err}")))?;
        log::debug!(
            target: "sread::ipc",
            "会话切片已保存（窗口 {label}，{} 个标签）",
            session.panes.iter().map(|pane| pane.tabs.len()).sum::<usize>()
        );
        Ok(session_store::load_window(&dir, label))
    })
}

/// 命令：移除**调用窗口**的会话切片（单窗口关闭时调用；应用整体退出保持记录）。
#[tauri::command]
pub fn forget_window_session(window: tauri::WebviewWindow) -> Result<(), IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let label = window.label();
        session_store::forget_window(&dir, label)
            .map_err(|err| IpcError::new(CODE_SESSION_SAVE, format!("移除窗口会话失败：{err}")))?;
        log::info!(target: "sread::ipc", "已移除窗口会话记录：{label}");
        Ok(())
    })
}

/// 命令：打开文件（重复打开自动复用**本栏位**已有标签；首次打开成功时记录历史）。
/// 新标签归属 `pane`（缺省为调用窗口默认栏 `#1`）。
#[tauri::command]
pub fn open_file(
    window: tauri::WebviewWindow,
    path: String,
    pane: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<TabInfo, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let settings = settings_store::load_app_settings(&dir);
        let owner = resolve_pane(&window, pane);
        let (info, reused) = lock_state(&state)?.open_file(&owner, Path::new(&path), &settings)?;
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
        if !reused {
            s_read_txt::mem::trim_after_large_work(info.byte_len);
        }
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
        let payload = lock_state(&state)?
            .rows(tab_id, start_row, count)
            .map_err(IpcError::from)?;
        Ok(payload)
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
        s_read_txt::mem::trim_after_large_work(info.byte_len);
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

/// 命令：列出指定栏位的标签（缺省为调用窗口默认栏 `#1`；前端启动同步/恢复时使用）。
#[tauri::command]
pub fn list_tabs(
    window: tauri::WebviewWindow,
    pane: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<TabsView, IpcError> {
    with_context(LogContext::request(), || {
        let guard = lock_state(&state)?;
        let owner = resolve_pane(&window, pane);
        Ok(TabsView {
            tabs: guard.tabs_info(&owner),
            active_tab_id: guard.active_tab(&owner),
        })
    })
}

/// 命令：关闭标签并返回**其所属窗口**的剩余标签视图。
///
/// 幂等：关闭不存在的标签不视为错误（返回调用窗口视图，规则：可重试操作幂等）。
#[tauri::command]
pub fn close_tab(
    window: tauri::WebviewWindow,
    tab_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<TabsView, IpcError> {
    with_context(LogContext::request(), || {
        let mut guard = lock_state(&state)?;
        // 关闭后标签不在表中，无法再反查 owner —— 先取 owner，关闭失败则回退调用窗口
        let owner = guard
            .owner_of(tab_id)
            .unwrap_or_else(|| window.label().to_string());
        let closed = guard.close(tab_id);
        let view = TabsView {
            tabs: guard.tabs_info(&owner),
            active_tab_id: guard.active_tab(&owner),
        };
        if closed {
            log::info!(
                target: "sread::ipc",
                "关闭标签：{}（窗口 {}，剩余 {} 个）",
                tab_id,
                owner,
                view.tabs.len()
            );
        }
        Ok(view)
    })
}

/// 命令：关闭**调用窗口全部栏位**的标签（窗口关闭前清理全局标签表）。
#[tauri::command]
pub fn close_window_tabs(
    window: tauri::WebviewWindow,
    state: State<'_, Mutex<AppState>>,
) -> Result<TabsView, IpcError> {
    with_context(LogContext::request(), || {
        let mut guard = lock_state(&state)?;
        let label = window.label().to_string();
        let closed = guard.close_window_panes(&label);
        let view = TabsView {
            tabs: guard.window_tabs_info(&label),
            active_tab_id: None,
        };
        log::info!(
            target: "sread::ipc",
            "关闭窗口标签：窗口 {}，共 {} 个",
            label,
            closed.len()
        );
        Ok(view)
    })
}

/// 主窗口 label 列表（`main`、`main-N`；设置/打印/拖影等辅助窗口不计）。
fn main_window_labels(app: &tauri::AppHandle) -> Vec<String> {
    let mut labels: Vec<String> = app
        .webview_windows()
        .keys()
        .filter(|label| label.as_str() == "main" || label.starts_with("main-"))
        .cloned()
        .collect();
    labels.sort();
    labels
}

/// 命令：主窗口数量（前端判断「最后一个窗口」用于干净退出标记）。
#[tauri::command]
pub fn main_window_count(app: tauri::AppHandle) -> usize {
    main_window_labels(&app).len()
}

/// 命令：发起「退出所有窗口」。
///
/// 多窗口：建立协调会话并广播 `srt://quit-request`，返回 `true`；
/// 单窗口：不建会话、返回 `false`（前端按窗口本地退出流程处理）。
#[tauri::command]
pub fn begin_quit_all(
    app: tauri::AppHandle,
    state: State<'_, s_read_txt::quit::QuitState>,
) -> Result<bool, IpcError> {
    with_context(LogContext::request(), || {
        let labels = main_window_labels(&app);
        if !state.begin(labels) {
            return Ok(false);
        }
        log::info!(target: "sread::ipc", "发起退出所有窗口");
        let _ = app.emit(s_read_txt::quit::EVENT_QUIT_REQUEST, ());
        Ok(true)
    })
}

/// 命令：回报「本窗口已就绪」；全部就绪时广播 `srt://quit-proceed` 放行。
#[tauri::command]
pub fn report_quit_ready(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, s_read_txt::quit::QuitState>,
) -> Result<(), IpcError> {
    with_context(LogContext::request(), || {
        if matches!(
            state.mark_ready(window.label()),
            s_read_txt::quit::ReadyOutcome::AllReady
        ) {
            log::info!(target: "sread::ipc", "全部窗口就绪，放行退出");
            let _ = app.emit(s_read_txt::quit::EVENT_QUIT_PROCEED, ());
        }
        Ok(())
    })
}

/// 命令：取消「退出所有窗口」并广播 `srt://quit-cancelled`。
#[tauri::command]
pub fn report_quit_cancel(
    app: tauri::AppHandle,
    state: State<'_, s_read_txt::quit::QuitState>,
) -> Result<(), IpcError> {
    with_context(LogContext::request(), || {
        if state.cancel() {
            log::info!(target: "sread::ipc", "退出所有窗口已取消");
            let _ = app.emit(s_read_txt::quit::EVENT_QUIT_CANCELLED, ());
        }
        Ok(())
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
        s_read_txt::mem::trim_after_large_work(info.byte_len);
        Ok(info)
    })
}

/// 读取当前应用设置（目录解析失败/文件损坏时回退默认——与启动路径一致）。
fn current_app_settings() -> s_read_txt::settings::model::AppSettings {
    let (dir, _origin) = paths::resolve_data_dir();
    s_read_txt::settings::store::load_app_settings(&dir)
}

/// 查找操作的超时：仅正则模式生效（字面扫描不受毫秒级超时影响）。
fn search_timeout(
    mode: SearchMode,
    settings: &s_read_txt::settings::model::AppSettings,
) -> Option<u32> {
    match mode {
        SearchMode::Regex => Some(settings.regex.timeout_ms),
        SearchMode::Literal => None,
    }
}

/// 命令：在编辑文档中查找（标准/正则；不环绕；`from` = 显示行 UTF-16 坐标，None 从头）。
#[tauri::command]
pub fn find_in_edit(
    tab_id: u64,
    query: String,
    case_sensitive: bool,
    mode: Option<String>,
    whole_word: Option<bool>,
    from: Option<(u64, u64)>,
    state: State<'_, Mutex<AppState>>,
) -> Result<Option<FindHit>, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        let timeout = search_timeout(mode, &current_app_settings());
        lock_state(&state)?
            .find_in_tab(
                tab_id,
                &query,
                case_sensitive,
                mode,
                whole_word.unwrap_or(false),
                timeout,
                from,
            )
            .map_err(IpcError::from)
    })
}

/// 命令：统计命中总数（计数显示；超过 20 万处仅报截断标志）。
#[tauri::command]
pub fn count_matches_in_edit(
    tab_id: u64,
    query: String,
    case_sensitive: bool,
    mode: Option<String>,
    whole_word: Option<bool>,
    state: State<'_, Mutex<AppState>>,
) -> Result<s_read_txt::textfile::editing::search::MatchCount, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        let timeout = search_timeout(mode, &current_app_settings());
        lock_state(&state)?
            .count_in_tab(
                tab_id,
                &query,
                case_sensitive,
                mode,
                whole_word.unwrap_or(false),
                timeout,
            )
            .map_err(IpcError::from)
    })
}

/// 命令：跨标签（工作区）搜索（设置 `app.find.multifileEnabled` 关闭时报错）。
#[tauri::command]
pub fn search_workspace(
    query: String,
    case_sensitive: bool,
    mode: Option<String>,
    whole_word: Option<bool>,
    state: State<'_, Mutex<AppState>>,
) -> Result<s_read_txt::app_state::WorkspaceSearchResponse, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        let settings = current_app_settings();
        if !settings.find.multifile_enabled {
            return Err(IpcError::new(
                s_read_txt::ipc_error::CODE_MULTIFILE_DISABLED,
                "多文件搜索已在设置中关闭（app.find.multifileEnabled）",
            ));
        }
        let timeout = search_timeout(mode, &settings);
        lock_state(&state)?
            .search_workspace(
                &query,
                case_sensitive,
                mode,
                whole_word.unwrap_or(false),
                timeout,
                settings.find.multifile_concurrency,
                settings.hard_limit_mb,
            )
            .map_err(IpcError::from)
    })
}

/// 命令：跨标签（工作区）替换（仅编辑态标签；逐文件单撤销步）。
#[tauri::command]
pub fn replace_workspace(
    query: String,
    case_sensitive: bool,
    mode: Option<String>,
    whole_word: Option<bool>,
    replacement: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<s_read_txt::app_state::WorkspaceReplaceResponse, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        let settings = current_app_settings();
        if !settings.find.multifile_enabled {
            return Err(IpcError::new(
                s_read_txt::ipc_error::CODE_MULTIFILE_DISABLED,
                "多文件搜索已在设置中关闭（app.find.multifileEnabled）",
            ));
        }
        let timeout = search_timeout(mode, &settings);
        lock_state(&state)?
            .replace_workspace(
                &query,
                case_sensitive,
                mode,
                whole_word.unwrap_or(false),
                &replacement,
                timeout,
            )
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
    whole_word: Option<bool>,
    from: Option<(u64, u64)>,
    replacement: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<Option<ReplaceNextOutcome>, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        let timeout = search_timeout(mode, &current_app_settings());
        lock_state(&state)?
            .replace_next_in_tab(
                tab_id,
                &query,
                case_sensitive,
                mode,
                whole_word.unwrap_or(false),
                timeout,
                from,
                &replacement,
            )
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
    whole_word: Option<bool>,
    replacement: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<ReplaceAllOutcome, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        let timeout = search_timeout(mode, &current_app_settings());
        let outcome = lock_state(&state)?.replace_all_in_tab(
            tab_id,
            &query,
            case_sensitive,
            mode,
            whole_word.unwrap_or(false),
            timeout,
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
    whole_word: Option<bool>,
    replacement: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<ReplacePreview, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        let timeout = search_timeout(mode, &current_app_settings());
        lock_state(&state)?
            .preview_replace_all_in_tab(
                tab_id,
                &query,
                case_sensitive,
                mode,
                whole_word.unwrap_or(false),
                timeout,
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
    whole_word: Option<bool>,
    replacement: String,
    selected: Option<Vec<usize>>,
    expect_state_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<ReplaceAllOutcome, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        let timeout = search_timeout(mode, &current_app_settings());
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
            whole_word.unwrap_or(false),
            timeout,
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
    whole_word: Option<bool>,
    start_row: u64,
    count: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<FindHit>, IpcError> {
    with_context(LogContext::request(), || {
        let mode = parse_search_mode(mode)?;
        let timeout = search_timeout(mode, &current_app_settings());
        lock_state(&state)?
            .match_window_in_tab(
                tab_id,
                &query,
                case_sensitive,
                mode,
                whole_word.unwrap_or(false),
                timeout,
                start_row,
                count,
            )
            .map_err(IpcError::from)
    })
}

/// 命令：读取查找历史（最新在前；上限取自 `app.find.historyLimit`，0 = 空）。
#[tauri::command]
pub fn list_find_history() -> Vec<String> {
    let (dir, _origin) = paths::resolve_data_dir();
    let limit = current_app_settings().find.history_limit;
    s_read_txt::find_history::list(&dir, limit)
}

/// 命令：记录一条查找历史（去重置顶、按上限截断；返回最新列表）。
#[tauri::command]
pub fn add_find_history(query: String) -> Vec<String> {
    let (dir, _origin) = paths::resolve_data_dir();
    let limit = current_app_settings().find.history_limit;
    s_read_txt::find_history::add(&dir, limit, &query)
}

/// 命令：清空查找历史（返回空列表）。
#[tauri::command]
pub fn clear_find_history() -> Vec<String> {
    let (dir, _origin) = paths::resolve_data_dir();
    s_read_txt::find_history::clear(&dir)
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

/// 命令：设置标签颜色（`None` 清除；颜色 id 由后端调色板校验）。
#[tauri::command]
pub fn set_tab_color(
    tab_id: u64,
    color: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<TabInfo, IpcError> {
    with_context(LogContext::request(), || {
        lock_state(&state)?
            .set_tab_color(tab_id, color.as_deref())
            .map_err(IpcError::from)
    })
}

/// 主窗口选项（标签跨窗口移动菜单用）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowOption {
    /// 窗口 label（`main` / `main-2`…）
    label: String,
    /// 展示标题（活动标签文件名；无活动标签时退回 label）
    title: String,
    /// 该窗口的标签数量
    tab_count: u32,
}

/// 命令：列出全部主窗口（含调用窗口；前端按自身 label 过滤）。
#[tauri::command]
pub fn list_windows(
    app: tauri::AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<WindowOption>, IpcError> {
    with_context(LogContext::request(), || {
        let guard = lock_state(&state)?;
        let mut labels: Vec<String> = app
            .webview_windows()
            .keys()
            .filter(|label| label.as_str() == "main" || label.starts_with("main-"))
            .cloned()
            .collect();
        labels.sort_by_key(|label| {
            label
                .strip_prefix("main")
                .and_then(|rest| rest.strip_prefix('-'))
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0)
        });
        Ok(labels
            .into_iter()
            .map(|label| {
                let tabs = guard.window_tabs_info(&label);
                let active_name = guard
                    .window_active_tab(&label)
                    .and_then(|id| tabs.iter().find(|tab| tab.tab_id == id))
                    .map(|tab| tab.name.clone());
                WindowOption {
                    title: active_name.unwrap_or_else(|| label.clone()),
                    tab_count: tabs.len() as u32,
                    label,
                }
            })
            .collect())
    })
}

/// 命令：把标签移动到其他主窗口（目标窗口默认栏 `#1`；已打开同一文件时合并并激活）。
///
/// 返回**源栏位**的剩余标签视图；目标窗口经 `srt://tabs-changed` 事件自行刷新。
#[tauri::command]
pub fn move_tab_to_window(
    app: tauri::AppHandle,
    tab_id: u64,
    target_label: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<TabsView, IpcError> {
    with_context(LogContext::request(), || {
        let view = {
            let mut guard = lock_state(&state)?;
            let source = guard
                .tab_info(tab_id)
                .map(|info| info.owner)
                .ok_or_else(|| IpcError::new(CODE_TAB_NOT_FOUND, "标签不存在"))?;
            guard
                .move_tab(tab_id, &default_pane(&target_label), usize::MAX)
                .map_err(IpcError::from)?;
            TabsView {
                tabs: guard.tabs_info(&source),
                active_tab_id: guard.active_tab(&source),
            }
        };
        let _ = app.emit(EVENT_TABS_CHANGED, ());
        Ok(view)
    })
}

/// 命令：把标签移动/并入指定栏位（同栏位即重排）。
///
/// `to_index = None` 表示追加到末尾。返回**源栏位**的剩余标签视图；
/// 所有窗口经 `srt://tabs-changed` 事件自行刷新。
#[tauri::command]
pub fn move_tab_to_pane(
    app: tauri::AppHandle,
    tab_id: u64,
    target_pane: String,
    to_index: Option<u32>,
    state: State<'_, Mutex<AppState>>,
) -> Result<TabsView, IpcError> {
    with_context(LogContext::request(), || {
        let view = {
            let mut guard = lock_state(&state)?;
            let source = guard
                .tab_info(tab_id)
                .map(|info| info.owner)
                .ok_or_else(|| IpcError::new(CODE_TAB_NOT_FOUND, "标签不存在"))?;
            guard
                .move_tab(
                    tab_id,
                    &target_pane,
                    to_index.map(|index| index as usize).unwrap_or(usize::MAX),
                )
                .map_err(IpcError::from)?;
            TabsView {
                tabs: guard.tabs_info(&source),
                active_tab_id: guard.active_tab(&source),
            }
        };
        let _ = app.emit(EVENT_TABS_CHANGED, ());
        Ok(view)
    })
}

/// 命令：文档级文本统计（供状态栏展示）。
#[tauri::command]
pub fn document_stats(
    tab_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<s_read_txt::stats::TextStats, IpcError> {
    with_context(LogContext::request(), || {
        let stats = lock_state(&state)?
            .document_stats(tab_id, s_read_txt::stats::STATS_MAX_CHARS)
            .map_err(IpcError::from)?;
        s_read_txt::mem::trim_after_large_work(stats.bytes);
        Ok(stats)
    })
}

// ---------- 标注（书签/高亮/注释） ----------

/// 命令：逻辑坐标 → 显示坐标（编辑态长行分段；供标注创建使用）。
#[tauri::command]
pub fn edit_display_pos(
    tab_id: u64,
    row: u64,
    utf16: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<(u64, u64), IpcError> {
    with_context(LogContext::request(), || {
        let guard = lock_state(&state)?;
        guard
            .display_pos(tab_id, row, utf16)
            .map_err(IpcError::from)
    })
}

/// 命令：列出当前标签的标注（读取时自动按摘录重定位）。
#[tauri::command]
pub fn list_annotations(
    tab_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<FileAnnotations, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        lock_state(&state)?
            .list_annotations(&dir, tab_id)
            .map_err(IpcError::from)
    })
}

/// 命令：添加书签。
#[tauri::command]
pub fn add_bookmark(
    tab_id: u64,
    row: u64,
    utf16: u64,
    label: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<FileAnnotations, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        lock_state(&state)?
            .add_bookmark(&dir, tab_id, row, utf16, label)
            .map_err(IpcError::from)
    })
}

/// 命令：删除书签。
#[tauri::command]
pub fn remove_bookmark(
    tab_id: u64,
    id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<FileAnnotations, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        lock_state(&state)?
            .remove_bookmark(&dir, tab_id, id)
            .map_err(IpcError::from)
    })
}

/// 命令：添加高亮（区间半开）。
#[tauri::command]
pub fn add_highlight(
    tab_id: u64,
    row: u64,
    start_utf16: u64,
    end_utf16: u64,
    color: Option<String>,
    state: State<'_, Mutex<AppState>>,
) -> Result<FileAnnotations, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        lock_state(&state)?
            .add_highlight(&dir, tab_id, row, start_utf16, end_utf16, color)
            .map_err(IpcError::from)
    })
}

/// 命令：删除高亮。
#[tauri::command]
pub fn remove_highlight(
    tab_id: u64,
    id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<FileAnnotations, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        lock_state(&state)?
            .remove_highlight(&dir, tab_id, id)
            .map_err(IpcError::from)
    })
}

/// 命令：添加注释 / 待办 / 行内批注。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn add_note(
    tab_id: u64,
    row: u64,
    utf16: u64,
    end_utf16: Option<u64>,
    text: String,
    kind: NoteKind,
    state: State<'_, Mutex<AppState>>,
) -> Result<FileAnnotations, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        lock_state(&state)?
            .add_note(&dir, tab_id, row, utf16, end_utf16, text, kind)
            .map_err(IpcError::from)
    })
}

/// 命令：更新注释文本与完成状态。
#[tauri::command]
pub fn update_note(
    tab_id: u64,
    id: u64,
    text: Option<String>,
    done: Option<bool>,
    state: State<'_, Mutex<AppState>>,
) -> Result<FileAnnotations, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        lock_state(&state)?
            .update_note(&dir, tab_id, id, text, done)
            .map_err(IpcError::from)
    })
}

/// 命令：删除注释。
#[tauri::command]
pub fn remove_note(
    tab_id: u64,
    id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<FileAnnotations, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        lock_state(&state)?
            .remove_note(&dir, tab_id, id)
            .map_err(IpcError::from)
    })
}

/// 命令：清空当前标签全部标注。
#[tauri::command]
pub fn clear_annotations(
    tab_id: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<FileAnnotations, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        lock_state(&state)?
            .clear_annotations(&dir, tab_id)
            .map_err(IpcError::from)
    })
}

/// 命令：选区统计（逻辑行坐标，半开区间，仅编辑态）。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn selection_stats(
    tab_id: u64,
    from_row: u64,
    from_utf16: u64,
    to_row: u64,
    to_utf16: u64,
    state: State<'_, Mutex<AppState>>,
) -> Result<s_read_txt::stats::TextStats, IpcError> {
    with_context(LogContext::request(), || {
        lock_state(&state)?
            .selection_stats(
                tab_id,
                (from_row, from_utf16),
                (to_row, to_utf16),
                s_read_txt::stats::SELECTION_STATS_MAX_CHARS,
            )
            .map_err(IpcError::from)
    })
}

/// 命令：读取阅读时长统计（跨天自动重置当日值）。
#[tauri::command]
pub fn get_reading_stats() -> Result<s_read_txt::reading_stats::ReadingStats, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let today = s_read_txt::time_util::local_day_string();
        Ok(s_read_txt::reading_stats::load(&dir, &today))
    })
}

/// 命令：累计阅读秒数（上限 3600/次；返回最新快照）。
#[tauri::command]
pub fn add_reading_seconds(
    seconds: u64,
) -> Result<s_read_txt::reading_stats::ReadingStats, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let today = s_read_txt::time_util::local_day_string();
        s_read_txt::reading_stats::add_seconds(&dir, seconds, &today)
            .map_err(|e| IpcError::new(CODE_IO, format!("读取时长统计写入失败：{e}")))
    })
}

/// 命令：将标签全文换行符统一为 `lf` / `crlf` / `cr`（仅编辑态；≤32MB，单撤销步）。
#[tauri::command]
pub fn convert_eol(
    tab_id: u64,
    target: String,
    state: State<'_, Mutex<AppState>>,
) -> Result<s_read_txt::textfile::editing::edit_doc::EolConvertOutcome, IpcError> {
    with_context(LogContext::request(), || {
        let target = match target.as_str() {
            "lf" => s_read_txt::textfile::eol::EolTarget::Lf,
            "crlf" => s_read_txt::textfile::eol::EolTarget::CrLf,
            "cr" => s_read_txt::textfile::eol::EolTarget::Cr,
            other => {
                return Err(IpcError::new(
                    CODE_INVALID_EOL,
                    format!("未知换行符：{other}"),
                ))
            }
        };
        lock_state(&state)?
            .convert_eol(tab_id, target)
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

/// 命令：保存用户主题（主题编辑器：新建或覆盖；强校验；内置 id 拒绝）。
#[tauri::command]
pub fn save_theme(manifest: theme::ThemeManifest) -> Result<theme::ThemeSummary, IpcError> {
    with_context(LogContext::request(), || {
        let (dir, _origin) = paths::resolve_data_dir();
        let summary = theme::save_manifest(&dir, manifest).map_err(theme_ipc_error)?;
        log::info!(target: "sread::ipc", "用户主题已保存：{}", summary.id);
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
