//! `session.json` 读写（自愈载入与 settings 共用 `storage::json_io`）。
//!
//! 策略：
//! - 载入：缺失/损坏 → 默认会话（损坏文件备份 `.corrupt-<纳秒>`）；随后归一；
//! - 归一：v1 单窗口 → v2 多窗口 → v3 分栏 → v4 光标/折叠锚点；版本对齐；窗口尺寸越界回退默认；
//!   剔除空 label / 空路径标签；重复 label 去重；栏位净化（键去重、上限、下标收敛）；
//!   布局树校验（叶子必须与栏位集合一致，否则按栏位重建默认布局）；焦点校验；折叠锚点净化；
//! - 保存：按窗口合并写入（read-modify-write，多窗口互不覆盖）；原子落盘。

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use crate::session::model::{
    PaneLayout, PaneSession, PaneSplitDir, SessionState, WindowSession, WindowState,
    DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH, MAX_PANES, MAX_SESSION_FOLDS,
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

/// 窗口默认栏位键（与 `app_state::default_pane` 约定一致；此处内联避免模块耦合）。
fn default_pane_key(label: &str) -> String {
    format!("{label}#1")
}

/// 归一：v1/v2 → v3 迁移；版本对齐；逐窗口校正几何、栏位与布局；焦点校验。
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
                legacy_active_tab_index: state.legacy_active_tab_index,
                legacy_tabs: std::mem::take(&mut state.legacy_tabs),
                ..WindowSession::default()
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

        // v2 单栏锚点 → v3 默认栏
        if entry.panes.is_empty() && !entry.legacy_tabs.is_empty() {
            entry.panes.push(PaneSession {
                pane: default_pane_key(&entry.label),
                active_tab_index: entry.legacy_active_tab_index,
                tabs: std::mem::take(&mut entry.legacy_tabs),
            });
        }
        entry.legacy_active_tab_index = 0;
        entry.legacy_tabs.clear();

        let mut pane_seen = HashSet::new();
        entry.panes.retain_mut(|pane| {
            pane.pane = pane.pane.trim().to_string();
            if pane.pane.is_empty() || !pane_seen.insert(pane.pane.clone()) {
                return false;
            }
            pane.tabs.retain_mut(|tab| {
                tab.path = tab.path.trim().to_string();
                if let Some(encoding) = tab.encoding.take() {
                    let trimmed = encoding.trim().to_string();
                    tab.encoding = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    };
                }
                if !tab.path.is_empty() {
                    // 折叠锚点防御：去重、排序、剔除零长，条数限幅
                    tab.folds.retain(|span| span.len > 0);
                    tab.folds.sort_by_key(|span| span.start_row);
                    tab.folds.dedup_by_key(|span| span.start_row);
                    tab.folds.truncate(MAX_SESSION_FOLDS);
                }
                !tab.path.is_empty()
            });
            let max_index = pane.tabs.len().saturating_sub(1) as u32;
            pane.active_tab_index = pane.active_tab_index.min(max_index);
            true
        });
        while entry.panes.len() > MAX_PANES {
            entry.panes.pop();
        }

        let pane_keys: Vec<String> = entry.panes.iter().map(|pane| pane.pane.clone()).collect();
        entry.layout = sanitize_layout(entry.layout.take(), &pane_keys);
        if entry.panes.is_empty() {
            entry.layout = None;
            entry.focused_pane = None;
        } else if let Some(focused) = entry.focused_pane.as_deref() {
            if !pane_keys.iter().any(|key| key == focused) {
                entry.focused_pane = None;
            }
        }
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

/// 布局树净化：叶子必须恰好覆盖栏位集合（无未知/重复/缺失），否则按栏位重建默认布局。
fn sanitize_layout(layout: Option<PaneLayout>, pane_keys: &[String]) -> Option<PaneLayout> {
    if pane_keys.is_empty() {
        return None;
    }
    let mut used: Vec<String> = Vec::new();
    let cleaned = layout.and_then(|node| clean_node(node, pane_keys, &mut used));
    let complete = cleaned.is_some()
        && used.len() == pane_keys.len()
        && pane_keys
            .iter()
            .all(|key| used.iter().any(|item| item == key));
    if complete {
        cleaned
    } else {
        Some(default_layout(pane_keys))
    }
}

/// 递归清洗布局节点：未知/重复叶子返回 `None`；分支子项缺失时按剩余项收敛。
fn clean_node(
    node: PaneLayout,
    pane_keys: &[String],
    used: &mut Vec<String>,
) -> Option<PaneLayout> {
    match node {
        PaneLayout::Leaf { pane } => {
            let pane = pane.trim().to_string();
            if pane_keys.iter().any(|key| key == &pane) && !used.iter().any(|item| item == &pane) {
                used.push(pane.clone());
                Some(PaneLayout::Leaf { pane })
            } else {
                None
            }
        }
        PaneLayout::Split {
            dir,
            sizes,
            children,
        } => {
            let mut kept_children = Vec::new();
            let mut kept_sizes = Vec::new();
            for (index, child) in children.into_iter().enumerate() {
                if let Some(cleaned) = clean_node(child, pane_keys, used) {
                    kept_children.push(cleaned);
                    kept_sizes.push(sizes.get(index).copied().unwrap_or(1.0));
                }
            }
            match kept_children.len() {
                0 => None,
                1 => kept_children.pop(),
                _ => Some(PaneLayout::Split {
                    dir,
                    sizes: normalize_sizes(&kept_sizes),
                    children: kept_children,
                }),
            }
        }
    }
}

/// 比例归一：非有限/非正值记 0；合计为 0 时等分；否则按合计缩放为 1。
fn normalize_sizes(sizes: &[f64]) -> Vec<f64> {
    let cleaned: Vec<f64> = sizes
        .iter()
        .map(|value| {
            if value.is_finite() && *value > 0.0 {
                *value
            } else {
                0.0
            }
        })
        .collect();
    let total: f64 = cleaned.iter().sum();
    if total <= f64::EPSILON {
        let equal = 1.0 / cleaned.len().max(1) as f64;
        return cleaned.iter().map(|_| equal).collect();
    }
    cleaned.iter().map(|value| value / total).collect()
}

/// 默认布局：单栏 → 叶；多栏 → 横向等分。
fn default_layout(pane_keys: &[String]) -> PaneLayout {
    if pane_keys.len() == 1 {
        return PaneLayout::Leaf {
            pane: pane_keys[0].clone(),
        };
    }
    let equal = 1.0 / pane_keys.len() as f64;
    PaneLayout::Split {
        dir: PaneSplitDir::Row,
        sizes: pane_keys.iter().map(|_| equal).collect(),
        children: pane_keys
            .iter()
            .map(|pane| PaneLayout::Leaf { pane: pane.clone() })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::model::{PaneLayout, PaneSession, PaneSplitDir, SessionTab};

    fn data_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("创建临时目录失败")
    }

    fn tab(path: &str) -> SessionTab {
        SessionTab {
            path: path.to_string(),
            ..SessionTab::default()
        }
    }

    fn leaf(pane: &str) -> PaneLayout {
        PaneLayout::Leaf {
            pane: pane.to_string(),
        }
    }

    /// v3 往返一致（含分栏布局、每栏标签与聚焦栏、光标与折叠锚点）。
    #[test]
    fn session_roundtrip_v3() {
        let dir = data_dir();
        let state = SessionState {
            schema_version: 4,
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
                    panes: vec![
                        PaneSession {
                            pane: "main#1".to_string(),
                            active_tab_index: 1,
                            tabs: vec![
                                SessionTab {
                                    path: "D:/novels/a.txt".to_string(),
                                    encoding: Some("GB18030".to_string()),
                                    scroll_row: 1234,
                                    edit_mode: false,
                                    color: None,
                                    caret_row: Some(1200),
                                    caret_col: Some(7),
                                    folds: vec![
                                        crate::session::model::FoldSpan {
                                            start_row: 100,
                                            len: 42,
                                        },
                                        crate::session::model::FoldSpan {
                                            start_row: 600,
                                            len: 3,
                                        },
                                    ],
                                },
                                SessionTab {
                                    path: "D:/novels/b.txt".to_string(),
                                    encoding: None,
                                    scroll_row: 0,
                                    edit_mode: true,
                                    color: Some("green".to_string()),
                                    caret_row: None,
                                    caret_col: None,
                                    folds: Vec::new(),
                                },
                            ],
                        },
                        PaneSession {
                            pane: "main#2".to_string(),
                            active_tab_index: 0,
                            tabs: vec![tab("D:/novels/c.txt")],
                        },
                    ],
                    layout: Some(PaneLayout::Split {
                        dir: PaneSplitDir::Row,
                        sizes: vec![0.6, 0.4],
                        children: vec![leaf("main#1"), leaf("main#2")],
                    }),
                    focused_pane: Some("main#2".to_string()),
                    ..WindowSession::default()
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
                    panes: vec![PaneSession {
                        pane: "main-2#1".to_string(),
                        active_tab_index: 0,
                        tabs: vec![tab("D:/novels/d.txt")],
                    }],
                    layout: Some(leaf("main-2#1")),
                    focused_pane: Some("main-2#1".to_string()),
                    ..WindowSession::default()
                },
            ],
            focused_label: Some("main-2".to_string()),
            ..SessionState::default()
        };
        save(dir.path(), &state).expect("保存失败");
        let loaded = load(dir.path());
        assert_eq!(loaded, state);
        let raw = std::fs::read_to_string(session_path(dir.path())).expect("读取失败");
        assert!(raw.contains("\"schemaVersion\": 4"));
        assert!(raw.contains("\"focusedLabel\": \"main-2\""));
        assert!(raw.contains("\"focusedPane\": \"main#2\""));
        assert!(!raw.contains("legacy"));
    }

    /// v1 单窗口文件迁移到当前版本（顶层 window/tabs 归入 main#1）。
    #[test]
    fn migrates_v1_single_window() {
        let dir = data_dir();
        std::fs::write(
            session_path(dir.path()),
            br#"{"schemaVersion":1,"window":{"x":5,"y":6,"width":1280,"height":800,"maximized":false},"activeTabIndex":1,"tabs":[{"path":"D:/a.txt","encoding":null,"scrollRow":3,"editMode":false},{"path":"D:/b.txt","encoding":"UTF-8","scrollRow":9,"editMode":true}]}"#,
        )
        .expect("写会话失败");
        let loaded = load(dir.path());
        assert_eq!(loaded.schema_version, 4);
        assert_eq!(loaded.windows.len(), 1);
        let main = &loaded.windows[0];
        assert_eq!(main.label, "main");
        assert_eq!(main.window.x, Some(5));
        assert_eq!(main.panes.len(), 1);
        assert_eq!(main.panes[0].pane, "main#1");
        assert_eq!(main.panes[0].active_tab_index, 1);
        assert_eq!(main.panes[0].tabs.len(), 2);
        assert_eq!(main.panes[0].tabs[1].path, "D:/b.txt");
        assert_eq!(main.layout, Some(leaf("main#1")));
    }

    /// v2 多窗口切片（window 级 tabs/activeTabIndex）迁移为单栏当前版本。
    #[test]
    fn migrates_v2_slice_to_single_pane() {
        let dir = data_dir();
        let v2 = r#"{
          "schemaVersion": 2,
          "focusedLabel": "main",
          "windows": [{ "label": "main", "window": { "width": 1100, "height": 760, "maximized": false },
            "activeTabIndex": 1, "tabs": [
              { "path": "C:/a.txt", "encoding": null, "scrollRow": 3, "editMode": false, "color": null },
              { "path": "C:/b.txt", "encoding": null, "scrollRow": 0, "editMode": false, "color": null } ] }]
        }"#;
        std::fs::write(session_path(dir.path()), v2).expect("写会话失败");
        let loaded = load(dir.path());
        assert_eq!(loaded.schema_version, 4);
        let main = &loaded.windows[0];
        assert_eq!(main.panes.len(), 1);
        assert_eq!(main.panes[0].pane, "main#1");
        assert_eq!(main.panes[0].active_tab_index, 1);
        assert_eq!(main.panes[0].tabs.len(), 2);
        assert_eq!(main.layout, Some(leaf("main#1")));
        assert_eq!(loaded.focused_label.as_deref(), Some("main"));
    }

    /// v3 旧文件（标签无 caret/folds 字段）迁移：字段缺失按默认补齐。
    #[test]
    fn migrates_v3_tab_without_caret_and_folds() {
        let dir = data_dir();
        let v3 = r#"{
          "schemaVersion": 3,
          "windows": [{ "label": "main", "window": { "width": 1100, "height": 760, "maximized": false },
            "panes": [{ "pane": "main#1", "activeTabIndex": 0, "tabs": [
              { "path": "C:/a.txt", "encoding": null, "scrollRow": 3, "editMode": true, "color": "red" } ] }],
            "layout": { "type": "leaf", "pane": "main#1" }, "focusedPane": "main#1" }]
        }"#;
        std::fs::write(session_path(dir.path()), v3).expect("写会话失败");
        let loaded = load(dir.path());
        assert_eq!(loaded.schema_version, 4);
        let tab = &loaded.windows[0].panes[0].tabs[0];
        assert_eq!(tab.caret_row, None);
        assert_eq!(tab.caret_col, None);
        assert!(tab.folds.is_empty());
    }

    /// 折叠锚点净化：零长剔除、起始行去重、按行号升序、条数限幅。
    #[test]
    fn normalizes_fold_anchors() {
        let dir = data_dir();
        let mut folds = String::new();
        for index in (0..600).rev() {
            if !folds.is_empty() {
                folds.push(',');
            }
            folds.push_str(&format!("{{\"startRow\":{},\"len\":2}}", index));
        }
        let v4 = format!(
            r#"{{"schemaVersion":4,"windows":[{{"label":"main","window":{{"width":1100,"height":760,"maximized":false}},
            "panes":[{{"pane":"main#1","activeTabIndex":0,"tabs":[
              {{"path":"C:/a.txt","scrollRow":0,"folds":[{{"startRow":9,"len":0}},{{"startRow":5,"len":4}},{{"startRow":5,"len":9}}{folds_extra}]}}]}}],
            "layout":{{"type":"leaf","pane":"main#1"}}}}]}}"#,
            folds_extra = format!(",{folds}")
        );
        std::fs::write(session_path(dir.path()), v4).expect("写会话失败");
        let loaded = load(dir.path());
        let tab = &loaded.windows[0].panes[0].tabs[0];
        assert!(tab.folds.len() <= crate::session::model::MAX_SESSION_FOLDS);
        assert_eq!(tab.folds.len(), crate::session::model::MAX_SESSION_FOLDS);
        assert!(tab.folds.iter().all(|span| span.len > 0));
        assert!(tab
            .folds
            .windows(2)
            .all(|pair| pair[0].start_row < pair[1].start_row));
        assert_eq!(tab.folds[0].start_row, 0);
    }

    /// 布局净化：未知叶子剔除、缺失栏位补齐 → 不完整时整体回退默认布局。
    #[test]
    fn layout_sanitize_falls_back_when_incomplete() {
        let dir = data_dir();
        let raw = r#"{
          "schemaVersion": 3,
          "windows": [{
            "label": "main",
            "window": { "width": 1100, "height": 760, "maximized": false },
            "panes": [
              { "pane": "main#1", "activeTabIndex": 0, "tabs": [ { "path": "D:/a.txt", "scrollRow": 0 } ] },
              { "pane": "main#2", "activeTabIndex": 0, "tabs": [ { "path": "D:/b.txt", "scrollRow": 0 } ] }
            ],
            "layout": { "type": "split", "dir": "row", "sizes": [0.5, 0.5],
              "children": [ { "type": "leaf", "pane": "main#9" }, { "type": "leaf", "pane": "main#1" } ] }
          }]
        }"#;
        std::fs::write(session_path(dir.path()), raw).expect("写会话失败");
        let loaded = load(dir.path());
        let main = &loaded.windows[0];
        assert_eq!(
            main.layout,
            Some(PaneLayout::Split {
                dir: PaneSplitDir::Row,
                sizes: vec![0.5, 0.5],
                children: vec![leaf("main#1"), leaf("main#2")],
            }),
            "不完整布局应回退为按栏位重建"
        );
    }

    /// 布局净化：完整布局保留并按合计归一化比例；单孩子分支收敛为叶。
    #[test]
    fn layout_sanitize_keeps_complete_layout_and_normalizes() {
        let dir = data_dir();
        let raw = r#"{
          "schemaVersion": 3,
          "windows": [{
            "label": "main",
            "window": { "width": 1100, "height": 760, "maximized": false },
            "panes": [
              { "pane": "main#1", "activeTabIndex": 0, "tabs": [ { "path": "D:/a.txt", "scrollRow": 0 } ] },
              { "pane": "main#2", "activeTabIndex": 0, "tabs": [ { "path": "D:/b.txt", "scrollRow": 0 } ] }
            ],
            "layout": { "type": "split", "dir": "column", "sizes": [3, 1], "children": [
              { "type": "leaf", "pane": "main#1" },
              { "type": "split", "dir": "row", "sizes": [7], "children": [ { "type": "leaf", "pane": "main#2" } ] }
            ] }
          }]
        }"#;
        std::fs::write(session_path(dir.path()), raw).expect("写会话失败");
        let loaded = load(dir.path());
        let main = &loaded.windows[0];
        assert_eq!(
            main.layout,
            Some(PaneLayout::Split {
                dir: PaneSplitDir::Column,
                sizes: vec![0.75, 0.25],
                children: vec![leaf("main#1"), leaf("main#2")],
            })
        );
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

    /// 归一：几何越界回退默认；空/重复 label 剔除；空路径标签剔除；
    /// 栏位上限与键去重；活动下标收敛；焦点窗口失效清空。
    #[test]
    fn normalize_fixes_invalid_values() {
        let dir = data_dir();
        std::fs::write(
            session_path(dir.path()),
            r#"{"schemaVersion":99,"focusedLabel":"main-9","windows":[
              {"label":"main","window":{"x":null,"y":null,"width":100,"height":99999,"maximized":false},"activeTabIndex":5,"tabs":[{"path":"   ","encoding":null,"scrollRow":0,"editMode":false},{"path":"D:/ok.txt","encoding":"   ","scrollRow":9,"editMode":false}]},
              {"label":"  ","window":{"x":null,"y":null,"width":800,"height":600,"maximized":false},"activeTabIndex":0,"tabs":[]},
              {"label":"main-2","window":{"x":null,"y":null,"width":800,"height":600,"maximized":false},"panes":[{"pane":" main-2#1 ","activeTabIndex":9,"tabs":[{"path":"D:/c.txt","scrollRow":0}]},{"pane":"main-2#1","activeTabIndex":0,"tabs":[]}],"layout":{"type":"leaf","pane":"main-2#9"}}
            ]}"#
            .as_bytes(),
        )
        .expect("写会话失败");
        let loaded = load(dir.path());
        assert_eq!(loaded.schema_version, 4);
        assert_eq!(loaded.windows.len(), 2, "空 label 应剔除");
        let main = &loaded.windows[0];
        assert_eq!(main.window.width, DEFAULT_WINDOW_WIDTH);
        assert_eq!(main.window.height, DEFAULT_WINDOW_HEIGHT);
        assert_eq!(main.panes.len(), 1);
        assert_eq!(main.panes[0].pane, "main#1", "v2 标签应迁移到默认栏");
        assert_eq!(main.panes[0].tabs.len(), 1);
        assert_eq!(main.panes[0].tabs[0].encoding, None);
        assert_eq!(main.panes[0].active_tab_index, 0);
        let second = &loaded.windows[1];
        assert_eq!(second.panes.len(), 1, "重复栏位键应剔除");
        assert!(second.layout.is_some(), "无效叶子布局应回退默认");
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
            panes: vec![PaneSession {
                pane: "main#1".to_string(),
                active_tab_index: 0,
                tabs: vec![tab("D:/a.txt")],
            }],
            layout: Some(leaf("main#1")),
            focused_pane: Some("main#1".to_string()),
            ..WindowSession::default()
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
            panes: vec![PaneSession {
                pane: "main-2#1".to_string(),
                active_tab_index: 0,
                tabs: vec![tab("D:/b.txt")],
            }],
            layout: Some(leaf("main-2#1")),
            ..WindowSession::default()
        };
        save_window(dir.path(), "main", &main, Some("main-2")).expect("保存主窗失败");
        save_window(dir.path(), "main-2", &second, Some("main-2")).expect("保存二窗失败");
        let loaded = load(dir.path());
        assert_eq!(loaded.windows.len(), 2);
        assert_eq!(loaded.focused_label.as_deref(), Some("main-2"));
        assert_eq!(loaded.windows[0].panes[0].tabs[0].path, "D:/a.txt");
        assert_eq!(loaded.windows[1].panes[0].tabs[0].path, "D:/b.txt");

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
        assert!(slice.panes.is_empty());
    }
}
