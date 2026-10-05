//! `session.json` 读写（自愈载入与 settings 共用 `storage::json_io`）。
//!
//! 策略：
//! - 载入：缺失/损坏 → 默认会话（损坏文件备份 `.corrupt-<纳秒>`）；随后归一；
//! - 归一：v1 单窗口迁移到 v2 多窗口；版本对齐；窗口尺寸越界回退默认；
//!   剔除空 label / 空路径标签；重复 label 去重；活动下标收敛；焦点窗口校验；
//! - 保存：按窗口合并写入（read-modify-write，多窗口互不覆盖）；原子落盘。

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use crate::session::model::{
    SessionState, WindowSession, WindowState, DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH,
    MAX_WINDOW_DIMENSION, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, SESSION_SCHEMA_VERSION,
};
use crate::storage::json_io;

/// 会话文件名
pub const FILE_SESSION: &str = "session.json";

/// 会话文件路径（数据目录内）
pub fn session_path(dir: &Path) -> PathBuf {
    dir.join(FILE_SESSION)
}

/// 载入会话（自愈：缺失/损坏回默认；随后归一）。
pub fn load(dir: &Path) -> SessionState {
    json_io::load_json_or_default(&session_path(dir), normalize)
}

/// 载入指定窗口的会话切片（不存在时返回带 label 的空切片）。
pub fn load_window(dir: &Path, label: &str) -> WindowSession {
    let state = load(dir);
    state
        .windows
        .into_iter()
        .find(|entry| entry.label == label)
        .unwrap_or_else(|| WindowSession {
            label: label.to_string(),
            ..WindowSession::default()
        })
}

/// 保存整个会话（保存前归一，确保写入合法值；测试与全量覆写用）。
pub fn save(dir: &Path, state: &SessionState) -> io::Result<()> {
    let mut copy = state.clone();
    normalize(&mut copy);
    json_io::write_json_atomic(&session_path(dir), &copy)
}

/// 按窗口合并保存：读取现有会话 → 替换/追加该 label 的切片 → 原子写回。
/// `focused`：最后聚焦的主窗口 label（由后端运行态提供；`None` 时保留原值）。
pub fn save_window(
    dir: &Path,
    label: &str,
    slice: &WindowSession,
    focused: Option<&str>,
) -> io::Result<()> {
    let mut state = load(dir);
    let mut entry = slice.clone();
    entry.label = label.to_string();
    match state.windows.iter_mut().find(|item| item.label == label) {
        Some(existing) => *existing = entry,
        None => state.windows.push(entry),
    }
    if let Some(focused) = focused {
        state.focused_label = Some(focused.to_string());
    }
    save(dir, &state)
}

/// 移除某窗口的会话切片（单窗口关闭时调用；应用整体退出不调用）。
pub fn forget_window(dir: &Path, label: &str) -> io::Result<()> {
    let mut state = load(dir);
    state.windows.retain(|entry| entry.label != label);
    save(dir, &state)
}

/// 归一：v1 → v2 迁移；版本对齐；逐窗口校正几何、标签与活动下标；焦点校验。
fn normalize(state: &mut SessionState) {
    state.schema_version = SESSION_SCHEMA_VERSION;

    // v1 单窗口文件：顶层 window/activeTabIndex/tabs → windows[0]（label = main）
    if state.windows.is_empty() {
        let has_legacy = !state.legacy_tabs.is_empty()
            || state.legacy_window != WindowState::default()
            || state.legacy_active_tab_index != 0;
        if has_legacy {
            state.windows.push(WindowSession {
                label: "main".to_string(),
                window: std::mem::take(&mut state.legacy_window),
                active_tab_index: state.legacy_active_tab_index,
                tabs: std::mem::take(&mut state.legacy_tabs),
            });
        }
    }
    state.legacy_window = WindowState::default();
    state.legacy_active_tab_index = 0;
    state.legacy_tabs.clear();

    let mut seen = HashSet::new();
    state.windows.retain_mut(|entry| {
        entry.label = entry.label.trim().to_string();
        if entry.label.is_empty() || !seen.insert(entry.label.clone()) {
            return false;
        }
        normalize_window(&mut entry.window);
        entry.tabs.retain_mut(|tab| {
            tab.path = tab.path.trim().to_string();
            if let Some(encoding) = tab.encoding.take() {
                let trimmed = encoding.trim().to_string();
                tab.encoding = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                };
            }
            !tab.path.is_empty()
        });
        let max_index = entry.tabs.len().saturating_sub(1) as u32;
        entry.active_tab_index = entry.active_tab_index.min(max_index);
        true
    });

    if let Some(focused) = state.focused_label.as_deref() {
        if !state.windows.iter().any(|entry| entry.label == focused) {
            state.focused_label = None;
        }
    }
}

/// 窗口几何归一：尺寸越界回退默认。
fn normalize_window(window: &mut WindowState) {
    if !(MIN_WINDOW_WIDTH..=MAX_WINDOW_DIMENSION).contains(&window.width) {
        window.width = DEFAULT_WINDOW_WIDTH;
    }
    if !(MIN_WINDOW_HEIGHT..=MAX_WINDOW_DIMENSION).contains(&window.height) {
        window.height = DEFAULT_WINDOW_HEIGHT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::SessionTab;

    fn data_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("创建临时目录失败")
    }

    fn tab(path: &str) -> SessionTab {
        SessionTab {
            path: path.to_string(),
            encoding: None,
            scroll_row: 0,
            edit_mode: false,
            color: None,
        }
    }

    /// v2 往返一致（含两个窗口的几何、编码与滚动锚点）。
    #[test]
    fn session_roundtrip_v2() {
        let dir = data_dir();
        let state = SessionState {
            schema_version: 2,
            windows: vec![
                WindowSession {
                    label: "main".to_string(),
                    window: WindowState {
                        x: Some(120),
                        y: Some(80),
                        width: 1280,
                        height: 800,
                        maximized: true,
                    },
                    active_tab_index: 1,
                    tabs: vec![
                        SessionTab {
                            path: "D:/novels/a.txt".to_string(),
                            encoding: Some("GB18030".to_string()),
                            scroll_row: 1234,
                            edit_mode: false,
                            color: None,
                        },
                        SessionTab {
                            path: "D:/novels/b.txt".to_string(),
                            encoding: None,
                            scroll_row: 0,
                            edit_mode: true,
                            color: Some("green".to_string()),
                        },
                    ],
                },
                WindowSession {
                    label: "main-2".to_string(),
                    window: WindowState {
                        x: Some(900),
                        y: Some(60),
                        width: 900,
                        height: 700,
                        maximized: false,
                    },
                    active_tab_index: 0,
                    tabs: vec![tab("D:/novels/c.txt")],
                },
            ],
            focused_label: Some("main-2".to_string()),
            ..SessionState::default()
        };
        save(dir.path(), &state).expect("保存失败");
        let loaded = load(dir.path());
        assert_eq!(loaded, state);
        let raw = std::fs::read_to_string(session_path(dir.path())).expect("读取失败");
        assert!(raw.contains("\"schemaVersion\": 2"));
        assert!(raw.contains("\"focusedLabel\": \"main-2\""));
    }

    /// v1 单窗口文件迁移到 v2（顶层 window/tabs 归入 main）。
    #[test]
    fn migrates_v1_single_window() {
        let dir = data_dir();
        std::fs::write(
            session_path(dir.path()),
            br#"{"schemaVersion":1,"window":{"x":5,"y":6,"width":1280,"height":800,"maximized":false},"activeTabIndex":1,"tabs":[{"path":"D:/a.txt","encoding":null,"scrollRow":3,"editMode":false},{"path":"D:/b.txt","encoding":"UTF-8","scrollRow":9,"editMode":true}]}"#,
        )
        .expect("写会话失败");
        let loaded = load(dir.path());
        assert_eq!(loaded.schema_version, 2);
        assert_eq!(loaded.windows.len(), 1);
        let main = &loaded.windows[0];
        assert_eq!(main.label, "main");
        assert_eq!(main.window.x, Some(5));
        assert_eq!(main.active_tab_index, 1);
        assert_eq!(main.tabs.len(), 2);
        assert_eq!(main.tabs[1].path, "D:/b.txt");
    }

    /// 文件缺失 → 默认会话（无窗口，前端不恢复任何标签）。
    #[test]
    fn missing_session_returns_default() {
        let dir = data_dir();
        assert_eq!(load(dir.path()), SessionState::default());
    }

    /// 损坏文件 → 备份 + 默认会话。
    #[test]
    fn corrupt_session_backs_up_and_defaults() {
        let dir = data_dir();
        std::fs::write(session_path(dir.path()), b"{ broken").expect("写损坏文件失败");
        assert_eq!(load(dir.path()), SessionState::default());
        let backups: Vec<_> = std::fs::read_dir(dir.path())
            .expect("列目录失败")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("session.json.corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1, "应生成备份：{backups:?}");
    }

    /// 归一：几何越界回退默认；空 label/重复 label 剔除；空路径标签剔除；
    /// 活动下标收敛；焦点窗口失效清空。
    #[test]
    fn normalize_fixes_invalid_values() {
        let dir = data_dir();
        std::fs::write(
            session_path(dir.path()),
            br#"{"schemaVersion":99,"focusedLabel":"main-9","windows":[{"label":"main","window":{"x":null,"y":null,"width":100,"height":99999,"maximized":false},"activeTabIndex":5,"tabs":[{"path":"   ","encoding":null,"scrollRow":0,"editMode":false},{"path":"D:/ok.txt","encoding":"   ","scrollRow":9,"editMode":false}]},{"label":"  ","window":{"x":null,"y":null,"width":800,"height":600,"maximized":false},"activeTabIndex":0,"tabs":[]},{"label":"main-2","window":{"x":null,"y":null,"width":800,"height":600,"maximized":false},"activeTabIndex":0,"tabs":[{"path":"D:/c.txt","encoding":null,"scrollRow":0,"editMode":false}]}]}"#,
        )
        .expect("写会话失败");
        let loaded = load(dir.path());
        assert_eq!(loaded.schema_version, 2);
        assert_eq!(loaded.windows.len(), 2, "空 label 应剔除");
        let main = &loaded.windows[0];
        assert_eq!(main.window.width, DEFAULT_WINDOW_WIDTH);
        assert_eq!(main.window.height, DEFAULT_WINDOW_HEIGHT);
        assert_eq!(main.tabs.len(), 1);
        assert_eq!(main.tabs[0].encoding, None);
        assert_eq!(main.active_tab_index, 0);
        assert_eq!(loaded.focused_label, None, "失效焦点应清空");
    }

    /// 按窗口合并保存：两个窗口互不覆盖；随后移除其一不影响另一。
    #[test]
    fn save_window_merges_and_forget_removes() {
        let dir = data_dir();
        let main = WindowSession {
            label: "main".to_string(),
            window: WindowState {
                x: Some(10),
                y: Some(20),
                width: 1200,
                height: 800,
                maximized: false,
            },
            active_tab_index: 0,
            tabs: vec![tab("D:/a.txt")],
        };
        let second = WindowSession {
            label: "main-2".to_string(),
            window: WindowState {
                x: Some(500),
                y: Some(30),
                width: 900,
                height: 700,
                maximized: false,
            },
            active_tab_index: 0,
            tabs: vec![tab("D:/b.txt")],
        };
        save_window(dir.path(), "main", &main, Some("main-2")).expect("保存主窗失败");
        save_window(dir.path(), "main-2", &second, Some("main-2")).expect("保存二窗失败");
        let loaded = load(dir.path());
        assert_eq!(loaded.windows.len(), 2);
        assert_eq!(loaded.focused_label.as_deref(), Some("main-2"));
        assert_eq!(loaded.windows[0].tabs[0].path, "D:/a.txt");
        assert_eq!(loaded.windows[1].tabs[0].path, "D:/b.txt");

        forget_window(dir.path(), "main-2").expect("移除二窗失败");
        let after = load(dir.path());
        assert_eq!(after.windows.len(), 1);
        assert_eq!(after.windows[0].label, "main");
    }

    /// 按窗口载入：不存在的 label 返回空切片。
    #[test]
    fn load_window_missing_returns_empty() {
        let dir = data_dir();
        let slice = load_window(dir.path(), "main-2");
        assert_eq!(slice.label, "main-2");
        assert!(slice.tabs.is_empty());
    }
}
