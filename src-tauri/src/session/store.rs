//! `session.json` 读写（自愈载入与 settings 共用 `storage::json_io`）。
//!
//! 策略：
//! - 载入：缺失/损坏 → 默认会话（损坏文件备份 `.corrupt-<纳秒>`）；随后归一；
//! - 归一：版本对齐；窗口尺寸越界回退默认；剔除空路径标签；活动下标收敛；
//! - 保存：原子落盘（pretty JSON、UTF-8 无 BOM）。

use std::io;
use std::path::{Path, PathBuf};

use crate::session::model::{
    SessionState, DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MAX_WINDOW_DIMENSION,
    MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, SESSION_SCHEMA_VERSION,
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

/// 保存会话（保存前归一，确保写入合法值）。
pub fn save(dir: &Path, state: &SessionState) -> io::Result<()> {
    let mut copy = state.clone();
    normalize(&mut copy);
    json_io::write_json_atomic(&session_path(dir), &copy)
}

/// 归一：版本对齐；窗口尺寸越界回退默认；剔除空路径标签；活动下标收敛到有效范围。
fn normalize(state: &mut SessionState) {
    state.schema_version = SESSION_SCHEMA_VERSION;

    if !(MIN_WINDOW_WIDTH..=MAX_WINDOW_DIMENSION).contains(&state.window.width) {
        state.window.width = DEFAULT_WINDOW_WIDTH;
    }
    if !(MIN_WINDOW_HEIGHT..=MAX_WINDOW_DIMENSION).contains(&state.window.height) {
        state.window.height = DEFAULT_WINDOW_HEIGHT;
    }

    state.tabs.retain_mut(|tab| {
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

    let max_index = state.tabs.len().saturating_sub(1) as u32;
    state.active_tab_index = state.active_tab_index.min(max_index);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{SessionTab, WindowState};

    fn data_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("创建临时目录失败")
    }

    /// 会话往返一致（含窗口坐标、标签编码与滚动锚点）。
    #[test]
    fn session_roundtrip() {
        let dir = data_dir();
        let state = SessionState {
            schema_version: 1,
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
                },
                SessionTab {
                    path: "D:/novels/b.txt".to_string(),
                    encoding: None,
                    scroll_row: 0,
                    edit_mode: true,
                },
            ],
        };
        save(dir.path(), &state).expect("保存失败");
        let loaded = load(dir.path());
        assert_eq!(loaded, state);
        let raw = std::fs::read_to_string(session_path(dir.path())).expect("读取失败");
        assert!(raw.contains("\"schemaVersion\": 1"));
    }

    /// 文件缺失 → 默认会话。
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

    /// 归一：窗口尺寸越界回退默认；空路径标签剔除；空编码归一为 None；活动下标收敛。
    #[test]
    fn normalize_fixes_invalid_values() {
        let dir = data_dir();
        std::fs::write(
            session_path(dir.path()),
            br#"{"schemaVersion":99,"window":{"x":null,"y":null,"width":100,"height":99999,"maximized":false},"activeTabIndex":5,"tabs":[{"path":"   ","encoding":null,"scrollRow":0,"editMode":false},{"path":"D:/ok.txt","encoding":"   ","scrollRow":9,"editMode":false}]}"#,
        )
        .expect("写会话失败");
        let loaded = load(dir.path());
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.window.width, DEFAULT_WINDOW_WIDTH);
        assert_eq!(loaded.window.height, DEFAULT_WINDOW_HEIGHT);
        assert_eq!(loaded.tabs.len(), 1);
        assert_eq!(loaded.tabs[0].path, "D:/ok.txt");
        assert_eq!(loaded.tabs[0].encoding, None);
        assert_eq!(loaded.active_tab_index, 0);
    }
}
