//! 标签跨窗口拖放：原生拖影窗口 + 命中测试 + 落点裁决。
//!
//! 坐标由前端提供（Windows 下按下的窗口隐式获得鼠标捕获，指针越界移动仍可达页面）：
//! 单位为屏幕物理像素；命中窗口按「后创建者在上」优先（`main-N` 序号大者）。

use std::sync::Mutex;

use serde::Serialize;
use tauri::{Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder};

use s_read_txt::app_state::{default_pane, AppState};
use s_read_txt::ipc_error::{IpcError, CODE_IO, CODE_TAB_NOT_FOUND};

use crate::commands::{self, EVENT_TABS_CHANGED};

/// 拖影窗口事件：告知拖影当前标签名/颜色/主题。
pub const EVENT_DRAG_GHOST: &str = "srt://tab-drag-ghost";
/// 悬停事件：目标窗口显示/清除插入指示（`clientX` 为内容坐标）。
pub const EVENT_DRAG_HOVER: &str = "srt://tab-drag-hover";
/// 落点事件：落点窗口收到后按 `clientX` 精确插入。
pub const EVENT_DRAG_DROPPED: &str = "srt://tab-drag-dropped";
/// 拖拽结束事件：源窗口清理拖拽状态。
pub const EVENT_DRAG_END: &str = "srt://tab-drag-end";

/// 拖影窗口 label（无边框、置顶、点击穿透）。
const GHOST_LABEL: &str = "drag-ghost";
/// 拖影窗口尺寸（宽 × 高，逻辑像素）。
const GHOST_SIZE: (f64, f64) = (180.0, 30.0);

/// 进行中的拖拽会话。
struct DragSession {
    tab_id: u64,
    source: String,
    over: Option<String>,
}

/// 拖放全局状态（同一时刻至多一个拖拽会话）。
#[derive(Default)]
pub struct TabDragState(Mutex<Option<DragSession>>);

/// 拖影内容载荷。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GhostPayload {
    name: String,
    color: Option<String>,
    dark: bool,
}

/// 悬停载荷（`clientX`/`clientY` 为内容坐标，供目标窗解析栏位与边缘区域）。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HoverPayload {
    active: bool,
    client_x: f64,
    client_y: f64,
}

/// 落点载荷。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DroppedPayload {
    tab_id: u64,
    client_x: f64,
    client_y: f64,
}

/// 主窗口 label 判定。
fn is_main_label(label: &str) -> bool {
    label == "main" || label.starts_with("main-")
}

/// 窗口序号（`main` = 0；`main-N` = N）。
fn ordinal(label: &str) -> u32 {
    label
        .strip_prefix("main-")
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// 命中测试：返回（窗口 label，内容坐标 clientX/clientY）。重叠时取序号大者（后创建者在上）。
fn hit_test(app: &tauri::AppHandle, x: f64, y: f64) -> Option<(String, f64, f64)> {
    let mut candidates: Vec<(u32, String, f64, f64)> = Vec::new();
    for (label, window) in app.webview_windows() {
        if !is_main_label(&label) {
            continue;
        }
        let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
            continue;
        };
        let left = f64::from(position.x);
        let top = f64::from(position.y);
        if x < left
            || y < top
            || x > left + f64::from(size.width)
            || y > top + f64::from(size.height)
        {
            continue;
        }
        let (Ok(inner), Ok(scale)) = (window.inner_position(), window.scale_factor()) else {
            continue;
        };
        let client_x = (x - f64::from(inner.x)) / scale;
        let client_y = (y - f64::from(inner.y)) / scale;
        candidates.push((ordinal(&label), label.clone(), client_x, client_y));
    }
    candidates.sort_by_key(|(order, _, _, _)| std::cmp::Reverse(*order));
    candidates
        .into_iter()
        .next()
        .map(|(_, label, client_x, client_y)| (label, client_x, client_y))
}

/// 命令：开始拖拽（前端越出源窗口边界时调用）。
///
/// 创建/复用拖影窗口并记录会话；失败返回 IO 错误（前端回退普通排序）。
#[tauri::command]
pub async fn begin_tab_drag(
    app: tauri::AppHandle,
    drag: State<'_, TabDragState>,
    tabs: State<'_, Mutex<AppState>>,
    tab_id: u64,
    name: String,
    color: Option<String>,
    dark: bool,
) -> Result<(), IpcError> {
    let source = {
        let guard = commands::lock_state(&tabs)?;
        guard
            .tab_info(tab_id)
            .map(|info| info.owner)
            .ok_or_else(|| IpcError::new(CODE_TAB_NOT_FOUND, "标签不存在"))?
    };
    let ghost = match app.get_webview_window(GHOST_LABEL) {
        Some(window) => window,
        None => {
            WebviewWindowBuilder::new(&app, GHOST_LABEL, WebviewUrl::App("drag-ghost.html".into()))
                .decorations(false)
                .resizable(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .focused(false)
                .shadow(false)
                .transparent(true)
                .inner_size(GHOST_SIZE.0, GHOST_SIZE.1)
                .visible(false)
                .build()
                .map_err(|err| IpcError::new(CODE_IO, format!("创建拖影窗口失败：{err}")))?
        }
    };
    let _ = ghost.set_ignore_cursor_events(true);
    let _ = ghost.set_always_on_top(true);
    let _ = app.emit_to(
        GHOST_LABEL,
        EVENT_DRAG_GHOST,
        GhostPayload { name, color, dark },
    );
    let mut guard = drag
        .0
        .lock()
        .map_err(|_| IpcError::internal("拖拽状态锁已被污染"))?;
    *guard = Some(DragSession {
        tab_id,
        source,
        over: None,
    });
    Ok(())
}

/// 命令：拖拽移动（屏幕物理坐标）。
#[tauri::command]
pub async fn drag_move(
    app: tauri::AppHandle,
    drag: State<'_, TabDragState>,
    x: f64,
    y: f64,
) -> Result<(), IpcError> {
    let over_now = hit_test(&app, x, y);
    let mut guard = drag
        .0
        .lock()
        .map_err(|_| IpcError::internal("拖拽状态锁已被污染"))?;
    let Some(session) = guard.as_mut() else {
        return Ok(());
    };
    if let Some(ghost) = app.get_webview_window(GHOST_LABEL) {
        let _ = ghost.set_position(PhysicalPosition::new(x + 12.0, y + 14.0));
        let _ = ghost.show();
    }
    let next = over_now.as_ref().map(|(label, _, _)| label.clone());
    if session.over != next {
        if let Some(previous) = session.over.clone() {
            let _ = app.emit_to(
                previous,
                EVENT_DRAG_HOVER,
                HoverPayload {
                    active: false,
                    client_x: 0.0,
                    client_y: 0.0,
                },
            );
        }
        if let Some((label, client_x, client_y)) = over_now {
            let _ = app.emit_to(
                label.clone(),
                EVENT_DRAG_HOVER,
                HoverPayload {
                    active: true,
                    client_x,
                    client_y,
                },
            );
        }
        session.over = next;
    }
    Ok(())
}

/// 命令：结束拖拽（坐标用于落点裁决；`cancelled` = Esc 取消，不产生移动）。
#[tauri::command]
pub async fn drag_end(
    app: tauri::AppHandle,
    drag: State<'_, TabDragState>,
    tabs: State<'_, Mutex<AppState>>,
    x: f64,
    y: f64,
    cancelled: bool,
) -> Result<(), IpcError> {
    let session = {
        let mut guard = drag
            .0
            .lock()
            .map_err(|_| IpcError::internal("拖拽状态锁已被污染"))?;
        guard.take()
    };
    let Some(session) = session else {
        return Ok(());
    };
    if let Some(ghost) = app.get_webview_window(GHOST_LABEL) {
        let _ = ghost.destroy();
    }
    if let Some(previous) = session.over.clone() {
        let _ = app.emit_to(
            previous,
            EVENT_DRAG_HOVER,
            HoverPayload {
                active: false,
                client_x: 0.0,
                client_y: 0.0,
            },
        );
    }
    if !cancelled {
        match hit_test(&app, x, y) {
            Some((target, client_x, client_y)) if target == session.source => {
                // 拖回源窗口：交由源窗口按 clientX 精确插入
                let _ = app.emit_to(
                    &session.source,
                    EVENT_DRAG_DROPPED,
                    DroppedPayload {
                        tab_id: session.tab_id,
                        client_x,
                        client_y,
                    },
                );
            }
            Some((target, client_x, client_y)) => {
                {
                    let mut guard = commands::lock_state(&tabs)?;
                    guard
                        .move_tab(session.tab_id, &default_pane(&target), usize::MAX)
                        .map_err(IpcError::from)?;
                }
                let _ = app.emit(EVENT_TABS_CHANGED, ());
                let _ = app.emit_to(
                    &target,
                    EVENT_DRAG_DROPPED,
                    DroppedPayload {
                        tab_id: session.tab_id,
                        client_x,
                        client_y,
                    },
                );
            }
            None => {
                // 桌面落点：新建窗口并把标签迁入其默认栏（置于落点附近）
                let label = commands::next_window_label(&app);
                let position = Some((x as i32 - 60, (y as i32 - 16).max(0)));
                commands::build_main_window(&app, &label, position, 1100.0, 760.0, false)?;
                {
                    let mut guard = commands::lock_state(&tabs)?;
                    guard
                        .move_tab(session.tab_id, &default_pane(&label), usize::MAX)
                        .map_err(IpcError::from)?;
                }
                let _ = app.emit(EVENT_TABS_CHANGED, ());
            }
        }
    }
    let _ = app.emit_to(&session.source, EVENT_DRAG_END, ());
    Ok(())
}
