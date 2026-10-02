//! 应用运行状态：打开标签集合与单文件会话管理。
//!
//! 职责：
//! - 维护标签表（`tab_id` 单调递增）与活动标签；
//! - 打开文件（含**重复打开去重**：同一路径命中已有标签则激活并复用）；
//! - 标签数量上限（来自 `settings.json` 的 `maxTabs`）；
//! - 文本窗口取行（带 IPC 保护上限 [`MAX_ROWS_PER_FETCH`]）与编码切换。
//!
//! 线程模型：由 IPC 层以 `Mutex<AppState>` 托管（`commands.rs`）；
//! 本模块为纯逻辑，可独立单元测试。
//!
//! 阶段说明：索引构建目前为同步（100MB 量级几十毫秒）；「后台构建 + 惰性恢复」
//! 属后续优化项（见设计文档 §4.1），接口保持不变。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::settings::model::AppSettings;
use crate::textfile::encoding::FileEncoding;
use crate::textfile::session::{FileSession, TextFileError};
use crate::textfile::window::RowText;

/// 单次取行的最大行数（IPC 防御上限；前端按可视区 + 预取分批请求）。
pub const MAX_ROWS_PER_FETCH: u32 = 2048;

/// 应用状态错误。
#[derive(Debug, thiserror::Error)]
pub enum AppStateError {
    /// 指定标签不存在
    #[error("标签不存在：{0}")]
    TabNotFound(u64),
    /// 标签数量已达上限
    #[error("标签数量已达上限：{limit}")]
    MaxTabs {
        /// 上限值（来自设置）
        limit: u32,
    },
    /// 底层文本文件错误（透传）
    #[error(transparent)]
    TextFile(#[from] TextFileError),
}

/// 标签的对外描述（IPC 载荷；不暴露 mmap 等内部状态）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabInfo {
    /// 标签 id（单调递增）
    pub tab_id: u64,
    /// 文件绝对路径（规范化后）
    pub path: String,
    /// 文件名（标签展示用）
    pub name: String,
    /// 当前生效编码（标签名，如 `UTF-8`）
    pub encoding: String,
    /// 手动编码（`None` = 自动检测）
    pub encoding_override: Option<String>,
    /// 总显示行数
    pub rows_total: u64,
    /// 文件总字节数
    pub byte_len: u64,
}

/// 文本窗口载荷（`get_rows` 返回体）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RowsPayload {
    /// 标签 id
    pub tab_id: u64,
    /// 请求起始行（原样回显，便于前端对账）
    pub start_row: u64,
    /// 总行数（前端滚动高度依据）
    pub rows_total: u64,
    /// 起始行对应的阅读百分比（状态栏显示）
    pub start_percent: f64,
    /// 行内容
    pub rows: Vec<RowText>,
}

/// 单个标签的运行时状态（内部）。
struct Tab {
    id: u64,
    session: FileSession,
}

/// 应用运行状态。
pub struct AppState {
    tabs: BTreeMap<u64, Tab>,
    next_tab_id: u64,
    active_tab: Option<u64>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// 创建空状态。
    pub fn new() -> Self {
        Self {
            tabs: BTreeMap::new(),
            next_tab_id: 1,
            active_tab: None,
        }
    }

    /// 打开文件：命中已有标签则激活复用，否则新建标签。
    ///
    /// 返回：`(标签信息, 是否为复用已有标签)`。
    /// 错误：文件不存在/过大（透传 `TextFileError`）；标签数达上限（`MaxTabs`）。
    pub fn open_file(
        &mut self,
        path: &Path,
        settings: &AppSettings,
    ) -> Result<(TabInfo, bool), AppStateError> {
        let canonical = canonicalize_lossy(path);
        if let Some(tab) = self
            .tabs
            .values()
            .find(|tab| canonicalize_lossy(tab.session.path()) == canonical)
        {
            self.active_tab = Some(tab.id);
            return Ok((tab_info(tab), true));
        }
        if self.tabs.len() as u32 >= settings.max_tabs {
            return Err(AppStateError::MaxTabs {
                limit: settings.max_tabs,
            });
        }
        let session = FileSession::open(&canonical, None, settings.max_file_size_mb)?;
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let tab = Tab { id, session };
        let info = tab_info(&tab);
        self.tabs.insert(id, tab);
        self.active_tab = Some(id);
        Ok((info, false))
    }

    /// 取文本窗口（`count` 受 [`MAX_ROWS_PER_FETCH`] 限制）。
    pub fn rows(
        &self,
        tab_id: u64,
        start_row: u64,
        count: u32,
    ) -> Result<RowsPayload, AppStateError> {
        let tab = self.tab(tab_id)?;
        let count = count.min(MAX_ROWS_PER_FETCH) as usize;
        let rows = tab.session.rows(start_row, count);
        let percent_row = rows
            .first()
            .map(|row| row.row)
            .unwrap_or_else(|| start_row.min(tab.session.rows_total().saturating_sub(1)));
        Ok(RowsPayload {
            tab_id,
            start_row,
            rows_total: tab.session.rows_total(),
            start_percent: tab.session.percent_at_row(percent_row),
            rows,
        })
    }

    /// 切换标签编码（`None` 恢复自动检测），重建索引后返回最新标签信息。
    pub fn set_encoding(
        &mut self,
        tab_id: u64,
        encoding: Option<FileEncoding>,
    ) -> Result<TabInfo, AppStateError> {
        let tab = self
            .tabs
            .get_mut(&tab_id)
            .ok_or(AppStateError::TabNotFound(tab_id))?;
        tab.session.set_encoding(encoding);
        Ok(tab_info(tab))
    }

    /// 关闭标签；若关闭的是活动标签，则活动标签回落为剩余最后一个。
    ///
    /// 返回：`true` 表示确实关闭了一个标签。
    pub fn close(&mut self, tab_id: u64) -> bool {
        let removed = self.tabs.remove(&tab_id).is_some();
        if removed && self.active_tab == Some(tab_id) {
            self.active_tab = self.tabs.keys().next_back().copied();
        }
        removed
    }

    /// 当前活动标签 id。
    pub fn active_tab(&self) -> Option<u64> {
        self.active_tab
    }

    /// 单个标签信息（不存在返回 None）。
    pub fn tab_info(&self, tab_id: u64) -> Option<TabInfo> {
        self.tabs.get(&tab_id).map(tab_info)
    }

    /// 全部标签信息（按创建顺序）。
    pub fn tabs_info(&self) -> Vec<TabInfo> {
        self.tabs.values().map(tab_info).collect()
    }

    /// 内部：取标签（不存在报错）。
    fn tab(&self, tab_id: u64) -> Result<&Tab, AppStateError> {
        self.tabs
            .get(&tab_id)
            .ok_or(AppStateError::TabNotFound(tab_id))
    }
}

/// 标签信息转换（内部辅助）。
fn tab_info(tab: &Tab) -> TabInfo {
    let session = &tab.session;
    TabInfo {
        tab_id: tab.id,
        path: session.path().to_string_lossy().into_owned(),
        name: file_name_of(session.path()),
        encoding: session.encoding().label().to_string(),
        encoding_override: session
            .encoding_override()
            .map(|encoding| encoding.label().to_string()),
        rows_total: session.rows_total(),
        byte_len: session.byte_len(),
    }
}

/// 取文件名（无法取得时退回完整路径字符串）。
fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// 规范化路径（失败时退回原路径；用于重复打开去重）。
fn canonicalize_lossy(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::model::AppSettings;

    fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, content).expect("写测试文件失败");
        path
    }

    /// 打开文件返回正确信息并设为活动标签。
    #[test]
    fn open_file_returns_info_and_activates() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "a.txt", "第一行\n第二行\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, reused) = state.open_file(&path, &settings).expect("打开失败");
        assert!(!reused);
        assert_eq!(info.name, "a.txt");
        assert_eq!(info.rows_total, 2);
        assert_eq!(info.encoding, "UTF-8");
        assert_eq!(state.active_tab(), Some(info.tab_id));
    }

    /// 重复打开同一路径：复用标签、不新增、激活切换。
    #[test]
    fn reopening_same_path_reuses_tab() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let first = write_file(dir.path(), "a.txt", "a\n");
        let second = write_file(dir.path(), "b.txt", "b\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info_a, _) = state.open_file(&first, &settings).expect("打开失败");
        let (info_b, _) = state.open_file(&second, &settings).expect("打开失败");
        let (info_a2, reused) = state.open_file(&first, &settings).expect("重开失败");
        assert!(reused);
        assert_eq!(info_a2.tab_id, info_a.tab_id);
        assert_eq!(state.tabs_info().len(), 2);
        assert_eq!(state.active_tab(), Some(info_a.tab_id));
        assert_ne!(info_b.tab_id, info_a.tab_id);
    }

    /// 标签数量上限：超出后返回 MaxTabs 错误。
    #[test]
    fn max_tabs_is_enforced() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let first = write_file(dir.path(), "a.txt", "a\n");
        let second = write_file(dir.path(), "b.txt", "b\n");
        let mut state = AppState::new();
        let mut settings = AppSettings::default();
        settings.max_tabs = 1;
        state.open_file(&first, &settings).expect("第一个应成功");
        let result = state.open_file(&second, &settings);
        assert!(matches!(result, Err(AppStateError::MaxTabs { limit: 1 })));
    }

    /// 取行窗口：载荷字段与内容正确；未知标签报错。
    #[test]
    fn rows_payload_and_missing_tab() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "a.txt", "one\ntwo\nthree\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        let payload = state.rows(info.tab_id, 1, 2).expect("取行失败");
        assert_eq!(payload.start_row, 1);
        assert_eq!(payload.rows_total, 3);
        assert_eq!(payload.rows.len(), 2);
        assert_eq!(payload.rows[0].text, "two");
        assert!(payload.start_percent > 0.0);
        assert!(matches!(
            state.rows(999, 0, 10),
            Err(AppStateError::TabNotFound(999))
        ));
    }

    /// 取行数受 IPC 防御上限约束。
    #[test]
    fn rows_count_is_capped() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "a.txt", "x\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        let payload = state
            .rows(info.tab_id, 0, MAX_ROWS_PER_FETCH + 1000)
            .expect("取行失败");
        assert!(payload.rows.len() <= MAX_ROWS_PER_FETCH as usize);
    }

    /// 切换编码更新标签信息；关闭标签回落活动标签。
    #[test]
    fn set_encoding_and_close() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "a.txt", "中文\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        let updated = state
            .set_encoding(info.tab_id, Some(FileEncoding::Gb18030))
            .expect("切换失败");
        assert_eq!(updated.encoding, "GB18030");
        assert_eq!(updated.encoding_override.as_deref(), Some("GB18030"));
        assert!(state.close(info.tab_id));
        assert_eq!(state.active_tab(), None);
        assert!(state.tabs_info().is_empty());
        assert!(!state.close(info.tab_id), "重复关闭应返回 false");
    }
}
