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
use crate::textfile::editing::edit_doc::{EditApplied, EditDoc, EditError, EditOp};
use crate::textfile::editing::save::{
    save_doc, snapshot_of, DiskSnapshot, SaveError, SaveOptions, SaveOutcome,
};
use crate::textfile::editing::search::{FindHit, ReplaceAllOutcome, ReplaceNextOutcome};
use crate::textfile::encoding::FileEncoding;
use crate::textfile::session::{FileSession, TextFileError};
use crate::textfile::source::DocumentSource;
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
    /// 编辑引擎错误（透传）
    #[error(transparent)]
    Edit(#[from] EditError),
    /// 保存链错误（透传）
    #[error(transparent)]
    Save(#[from] SaveError),
    /// 底层 IO 错误（磁盘快照 / 文件操作）
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// 标签未创建编辑文档
    #[error("标签 {0} 没有可用的编辑文档")]
    NotEditing(u64),
    /// 存在未保存修改，破坏性操作被阻止
    #[error("标签 {0} 有未保存的修改")]
    DirtyEdit(u64),
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
    /// 是否处于编辑模式（UI 开关；编辑文档首次进入时创建并保留）
    pub editing: bool,
    /// 是否有未保存修改（编辑文档存在且脏）
    pub dirty: bool,
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
    /// 只读会话（磁盘视图；编辑文档存在时由其供数）
    session: FileSession,
    /// 编辑文档（首次进入编辑模式时创建；保留到保存/重载/关闭）
    edit: Option<EditDoc>,
    /// 是否处于编辑模式（UI 开关；与编辑文档是否存在解耦）
    editing: bool,
    /// 打开/上次保存时的磁盘快照（外部修改冲突检测基准）
    edit_baseline: Option<DiskSnapshot>,
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
        let tab = Tab {
            id,
            session,
            edit: None,
            editing: false,
            edit_baseline: None,
        };
        let info = tab_info(&tab);
        self.tabs.insert(id, tab);
        self.active_tab = Some(id);
        Ok((info, false))
    }

    /// 取文本窗口（`count` 受 [`MAX_ROWS_PER_FETCH`] 限制）。
    ///
    /// 供数来源经 [`DocumentSource`] 契约：存在编辑文档时用编辑视图
    /// （含未保存修改），否则用只读会话——未来解析器/新格式实现同一契约即可接入。
    pub fn rows(
        &self,
        tab_id: u64,
        start_row: u64,
        count: u32,
    ) -> Result<RowsPayload, AppStateError> {
        let tab = self.tab(tab_id)?;
        let count = count.min(MAX_ROWS_PER_FETCH) as usize;
        let source: &dyn DocumentSource = match &tab.edit {
            Some(doc) => doc,
            None => &tab.session,
        };
        let rows = source.fetch_rows(start_row, count);
        let percent_row = rows
            .first()
            .map(|row| row.row)
            .unwrap_or_else(|| start_row.min(source.rows_total().saturating_sub(1)));
        Ok(RowsPayload {
            tab_id,
            start_row,
            rows_total: source.rows_total(),
            start_percent: source.percent_at_row(percent_row),
            rows,
        })
    }

    /// 切换标签编码（`None` 恢复自动检测），重建索引后返回最新标签信息。
    ///
    /// 编辑联动：存在未保存修改时拒绝（切换会作废编辑文档）；干净时
    /// 一并丢弃编辑文档（编码变化使旧解码失效），下次进入编辑时重建。
    pub fn set_encoding(
        &mut self,
        tab_id: u64,
        encoding: Option<FileEncoding>,
    ) -> Result<TabInfo, AppStateError> {
        let tab = self
            .tabs
            .get_mut(&tab_id)
            .ok_or(AppStateError::TabNotFound(tab_id))?;
        if tab.edit.as_ref().is_some_and(|doc| doc.is_dirty()) {
            return Err(AppStateError::DirtyEdit(tab_id));
        }
        tab.edit = None;
        tab.editing = false;
        tab.edit_baseline = None;
        tab.session.set_encoding(encoding);
        Ok(tab_info(tab))
    }

    // ---- 编辑模式（阶段 4a：编辑能力接线；UI 交互在阶段 4b） ----

    /// 切换编辑模式：首次进入时创建编辑文档并记录磁盘基准快照。
    ///
    /// 语义：
    /// - 编辑文档创建后保留（脏态跨模式持续），`rows()` 改由编辑文档供数；
    /// - 关闭编辑模式仅切换 `editing` 标志，不影响未保存修改；
    /// - 超长行由显示分段承载（编辑视图与只读一致），可正常进入编辑。
    pub fn toggle_edit(
        &mut self,
        tab_id: u64,
        settings: &AppSettings,
    ) -> Result<TabInfo, AppStateError> {
        let tab = self
            .tabs
            .get_mut(&tab_id)
            .ok_or(AppStateError::TabNotFound(tab_id))?;
        if tab.edit.is_none() {
            let doc = EditDoc::open(
                tab.session.path(),
                tab.session.encoding_override(),
                settings.max_file_size_mb,
            )?;
            tab.edit_baseline = snapshot_of(doc.path())?;
            tab.edit = Some(doc);
            tab.editing = true;
        } else {
            tab.editing = !tab.editing;
        }
        Ok(tab_info(tab))
    }

    /// 应用编辑批次（插入/删除/替换；批次原子 = 单个撤销步）。
    pub fn apply_edit_ops(
        &mut self,
        tab_id: u64,
        ops: &[EditOp],
    ) -> Result<EditApplied, AppStateError> {
        let doc = self.edit_doc_mut(tab_id)?;
        Ok(doc.apply_edits(ops)?)
    }

    /// 撤销一步（无可撤销内容时返回 `None`）。
    pub fn undo_edit(&mut self, tab_id: u64) -> Result<Option<EditApplied>, AppStateError> {
        let doc = self.edit_doc_mut(tab_id)?;
        Ok(doc.undo())
    }

    /// 重做一步（无可重做内容时返回 `None`）。
    pub fn redo_edit(&mut self, tab_id: u64) -> Result<Option<EditApplied>, AppStateError> {
        let doc = self.edit_doc_mut(tab_id)?;
        Ok(doc.redo())
    }

    /// 在编辑文档中查找下一个命中（普通文本，可大小写敏感；不环绕）。
    ///
    /// `from` 为逻辑行 UTF-16 坐标（`None` = 从文档开头）。
    pub fn find_in_tab(
        &self,
        tab_id: u64,
        query: &str,
        case_sensitive: bool,
        from: Option<(u64, u64)>,
    ) -> Result<Option<FindHit>, AppStateError> {
        let doc = self.edit_doc(tab_id)?;
        Ok(doc.find(query, case_sensitive, from)?)
    }

    /// 替换一个命中（从 `from` 起）并返回应用结果与「新落点起的下一个命中」。
    ///
    /// 未命中时返回 `None`（不产生编辑、不改变脏态）。
    pub fn replace_next_in_tab(
        &mut self,
        tab_id: u64,
        query: &str,
        case_sensitive: bool,
        from: Option<(u64, u64)>,
        replacement: &str,
    ) -> Result<Option<ReplaceNextOutcome>, AppStateError> {
        let doc = self.edit_doc_mut(tab_id)?;
        Ok(doc.replace_next(query, case_sensitive, from, replacement)?)
    }

    /// 全部替换（单次编辑 = 单个撤销步；命中数超过上限时报 `TooManyMatches`）。
    pub fn replace_all_in_tab(
        &mut self,
        tab_id: u64,
        query: &str,
        case_sensitive: bool,
        replacement: &str,
    ) -> Result<ReplaceAllOutcome, AppStateError> {
        let doc = self.edit_doc_mut(tab_id)?;
        Ok(doc.replace_all(query, case_sensitive, replacement)?)
    }

    /// 保存编辑文档到原路径。
    ///
    /// 参数：`target_encoding` = 编码询问结果（`None` = 保持当前文档编码）；
    ///       `make_backup` = 是否写 `.bak`（设置 + 首存判定，由命令层传入）；
    ///       `force` = 冲突时强制覆盖。
    /// 成功后将冲突基准刷新为保存后的磁盘快照。
    pub fn save_edit(
        &mut self,
        tab_id: u64,
        target_encoding: Option<FileEncoding>,
        make_backup: bool,
        force: bool,
    ) -> Result<SaveOutcome, AppStateError> {
        let tab = self
            .tabs
            .get_mut(&tab_id)
            .ok_or(AppStateError::TabNotFound(tab_id))?;
        let doc = tab.edit.as_mut().ok_or(AppStateError::NotEditing(tab_id))?;
        let options = SaveOptions {
            target_encoding: target_encoding.unwrap_or_else(|| doc.encoding()),
            make_backup,
            force,
            expected: tab.edit_baseline,
        };
        let path = doc.path().to_path_buf();
        let outcome = save_doc(doc, &path, &options)?;
        tab.edit_baseline = snapshot_of(&path)?;
        Ok(outcome)
    }

    /// 另存为：内容写入 `new_path` 后把标签重定向到新文件（会话与编辑文档重建，
    /// 编码显式设为实际保存编码），脏态清零；语义与编辑器一致——后续保存/阅读
    /// 都针对新路径。
    ///
    /// 边界：新内容若因用户输入产生超长行而无法重开编辑文档，则退出编辑模式
    /// （保存已成功，数据不丢失）。
    pub fn save_edit_as(
        &mut self,
        tab_id: u64,
        new_path: &Path,
        target_encoding: Option<FileEncoding>,
        make_backup: bool,
        settings: &AppSettings,
    ) -> Result<SaveOutcome, AppStateError> {
        let tab = self
            .tabs
            .get_mut(&tab_id)
            .ok_or(AppStateError::TabNotFound(tab_id))?;
        let doc = tab.edit.as_mut().ok_or(AppStateError::NotEditing(tab_id))?;
        let saved_encoding = target_encoding.unwrap_or_else(|| doc.encoding());
        let options = SaveOptions {
            target_encoding: saved_encoding,
            make_backup,
            force: false,   // 目标覆盖确认由系统保存对话框负责
            expected: None, // 另存为不做「外部修改」冲突检测
        };
        let outcome = save_doc(doc, new_path, &options)?;

        // 重定向标签到新路径
        let canonical = canonicalize_lossy(new_path);
        let session =
            FileSession::open(&canonical, Some(saved_encoding), settings.max_file_size_mb)?;
        let reopened =
            match EditDoc::open(&canonical, Some(saved_encoding), settings.max_file_size_mb) {
                Ok(doc) => Some(doc),
                Err(err) => {
                    log::warn!(
                        target: "sread::edit",
                        "另存为后重开编辑文档失败（退出编辑模式）：{err}"
                    );
                    None
                }
            };
        tab.edit_baseline = snapshot_of(&canonical)?;
        tab.editing = reopened.is_some();
        tab.edit = reopened;
        tab.session = session;
        Ok(outcome)
    }

    /// 从磁盘重载标签（丢弃编辑文档与未保存修改；脏态确认由命令层/前端完成）。
    pub fn reload_tab(
        &mut self,
        tab_id: u64,
        settings: &AppSettings,
    ) -> Result<TabInfo, AppStateError> {
        let tab = self
            .tabs
            .get_mut(&tab_id)
            .ok_or(AppStateError::TabNotFound(tab_id))?;
        let path = tab.session.path().to_path_buf();
        let override_encoding = tab.session.encoding_override();
        tab.session = FileSession::open(&path, override_encoding, settings.max_file_size_mb)?;
        tab.edit = None;
        tab.editing = false;
        tab.edit_baseline = None;
        Ok(tab_info(tab))
    }

    /// 只读访问标签的编辑文档（未进入编辑时报 `NotEditing`）。
    fn edit_doc(&self, tab_id: u64) -> Result<&EditDoc, AppStateError> {
        self.tab(tab_id)?
            .edit
            .as_ref()
            .ok_or(AppStateError::NotEditing(tab_id))
    }

    /// 可变访问标签的编辑文档（未进入编辑时报 `NotEditing`）。
    fn edit_doc_mut(&mut self, tab_id: u64) -> Result<&mut EditDoc, AppStateError> {
        self.tabs
            .get_mut(&tab_id)
            .ok_or(AppStateError::TabNotFound(tab_id))?
            .edit
            .as_mut()
            .ok_or(AppStateError::NotEditing(tab_id))
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
///
/// 行数/字节数统一走文档源契约：编辑文档存在时取编辑视图
/// （含未保存修改的行数与字节数），避免列表/保存返回的标签信息与阅读区不一致。
fn tab_info(tab: &Tab) -> TabInfo {
    let session = &tab.session;
    let source: &dyn DocumentSource = match &tab.edit {
        Some(doc) => doc,
        None => &tab.session,
    };
    TabInfo {
        tab_id: tab.id,
        path: session.path().to_string_lossy().into_owned(),
        name: file_name_of(session.path()),
        encoding: session.encoding().label().to_string(),
        encoding_override: session
            .encoding_override()
            .map(|encoding| encoding.label().to_string()),
        editing: tab.editing,
        dirty: tab.edit.as_ref().is_some_and(|doc| doc.is_dirty()),
        rows_total: source.rows_total(),
        byte_len: source.byte_len(),
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

    /// 进入编辑：创建编辑文档并由其供数；切回只读保留未保存修改。
    #[test]
    fn toggle_edit_creates_doc_and_serves_rows() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "a.txt", "abc\ndef\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");

        let toggled = state
            .toggle_edit(info.tab_id, &settings)
            .expect("进入编辑失败");
        assert!(toggled.editing);
        assert!(!toggled.dirty);

        state
            .apply_edit_ops(
                info.tab_id,
                &[EditOp::Insert {
                    row: 0,
                    utf16: 1,
                    text: "X".to_string(),
                }],
            )
            .expect("插入失败");
        let info_after = state.tab_info(info.tab_id).expect("标签缺失");
        assert!(info_after.dirty);
        assert!(info_after.editing, "编辑操作不改变模式标志");
        assert_eq!(
            state.rows(info.tab_id, 0, 10).expect("取行失败").rows[0].text,
            "aXbc"
        );

        // 切回只读：编辑文档保留（脏态持续），供数仍来自编辑文档
        let off = state
            .toggle_edit(info.tab_id, &settings)
            .expect("退出编辑失败");
        assert!(!off.editing);
        assert!(off.dirty);
        assert_eq!(
            state.rows(info.tab_id, 0, 1).expect("取行失败").rows[0].text,
            "aXbc"
        );
    }

    /// 撤销/重做经状态层生效；栈耗尽返回 None。
    #[test]
    fn undo_redo_via_state() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "a.txt", "abc\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        state
            .toggle_edit(info.tab_id, &settings)
            .expect("进入编辑失败");
        state
            .apply_edit_ops(
                info.tab_id,
                &[EditOp::Insert {
                    row: 0,
                    utf16: 0,
                    text: "Z".to_string(),
                }],
            )
            .expect("插入失败");

        assert!(state.undo_edit(info.tab_id).expect("撤销失败").is_some());
        assert_eq!(
            state.rows(info.tab_id, 0, 1).expect("取行失败").rows[0].text,
            "abc"
        );
        assert!(state.redo_edit(info.tab_id).expect("重做失败").is_some());
        assert_eq!(
            state.rows(info.tab_id, 0, 1).expect("取行失败").rows[0].text,
            "Zabc"
        );
        assert!(state.undo_edit(info.tab_id).expect("撤销失败").is_some());
        assert!(state.undo_edit(info.tab_id).expect("撤销失败").is_none());
    }

    /// 保存：写盘、脏态清零；外部修改后报冲突；force 覆盖成功。
    #[test]
    fn save_edit_and_conflict() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "a.txt", "abc");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        state
            .toggle_edit(info.tab_id, &settings)
            .expect("进入编辑失败");
        state
            .apply_edit_ops(
                info.tab_id,
                &[EditOp::Insert {
                    row: 0,
                    utf16: 0,
                    text: "Z".to_string(),
                }],
            )
            .expect("插入失败");

        let outcome = state
            .save_edit(info.tab_id, None, false, false)
            .expect("保存失败");
        assert_eq!(outcome.bytes_written, 4);
        assert!(!state.tab_info(info.tab_id).expect("标签缺失").dirty);
        assert_eq!(std::fs::read(&path).expect("读回失败"), b"Zabc");

        // 再次编辑 + 外部修改（临时文件 + rename 模拟：in-place 写会被 mmap 拒绝）
        state
            .apply_edit_ops(
                info.tab_id,
                &[EditOp::Insert {
                    row: 0,
                    utf16: 0,
                    text: "Y".to_string(),
                }],
            )
            .expect("插入失败");
        let external = dir.path().join("external.tmp");
        std::fs::write(&external, b"external").expect("外部写失败");
        std::fs::rename(&external, &path).expect("外部替换失败");

        let conflict = state.save_edit(info.tab_id, None, false, false);
        assert!(matches!(
            conflict,
            Err(AppStateError::Save(SaveError::Conflict))
        ));
        state
            .save_edit(info.tab_id, None, false, true)
            .expect("强制保存失败");
        assert_eq!(std::fs::read(&path).expect("读回失败"), b"YZabc");
    }

    /// 另存为：标签重定向到新路径并保持可编辑（脏态清零）。
    #[test]
    fn save_as_redirects_tab() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "a.txt", "abc");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        state
            .toggle_edit(info.tab_id, &settings)
            .expect("进入编辑失败");
        state
            .apply_edit_ops(
                info.tab_id,
                &[EditOp::Insert {
                    row: 0,
                    utf16: 0,
                    text: "Z".to_string(),
                }],
            )
            .expect("插入失败");

        let new_path = dir.path().join("b.txt");
        let outcome = state
            .save_edit_as(
                info.tab_id,
                &new_path,
                Some(FileEncoding::Utf8),
                false,
                &settings,
            )
            .expect("另存为失败");
        assert_eq!(outcome.encoding.label(), "UTF-8");
        let info_after = state.tab_info(info.tab_id).expect("标签缺失");
        assert!(info_after.path.ends_with("b.txt"));
        assert!(info_after.editing, "另存为后保持编辑模式");
        assert!(!info_after.dirty);
        assert_eq!(std::fs::read(&new_path).expect("读回失败"), b"Zabc");
        assert_eq!(
            state.rows(info.tab_id, 0, 1).expect("取行失败").rows[0].text,
            "Zabc"
        );
        // 后续编辑针对新文档
        state
            .apply_edit_ops(
                info.tab_id,
                &[EditOp::Insert {
                    row: 0,
                    utf16: 0,
                    text: "!".to_string(),
                }],
            )
            .expect("再次编辑失败");
        assert!(state.tab_info(info.tab_id).expect("标签缺失").dirty);
    }

    /// 重载：丢弃未保存修改并回到磁盘内容。
    #[test]
    fn reload_discards_edits() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "a.txt", "abc");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        state
            .toggle_edit(info.tab_id, &settings)
            .expect("进入编辑失败");
        state
            .apply_edit_ops(
                info.tab_id,
                &[EditOp::Insert {
                    row: 0,
                    utf16: 0,
                    text: "Z".to_string(),
                }],
            )
            .expect("插入失败");

        let info_after = state.reload_tab(info.tab_id, &settings).expect("重载失败");
        assert!(!info_after.editing);
        assert!(!info_after.dirty);
        assert_eq!(
            state.rows(info.tab_id, 0, 1).expect("取行失败").rows[0].text,
            "abc"
        );
    }

    /// 脏态阻止编码切换。
    #[test]
    fn set_encoding_blocked_when_dirty() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "a.txt", "中文\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        state
            .toggle_edit(info.tab_id, &settings)
            .expect("进入编辑失败");
        state
            .apply_edit_ops(
                info.tab_id,
                &[EditOp::Insert {
                    row: 0,
                    utf16: 0,
                    text: "X".to_string(),
                }],
            )
            .expect("插入失败");
        assert!(matches!(
            state.set_encoding(info.tab_id, Some(FileEncoding::Gb18030)),
            Err(AppStateError::DirtyEdit(_))
        ));
    }

    /// 超长行可进入编辑（显示分段）；读取不受影响。
    #[test]
    fn toggle_edit_allows_long_line() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let long_line = "x".repeat(70 * 1024);
        let path = write_file(dir.path(), "long.txt", &long_line);
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        let info = state
            .toggle_edit(info.tab_id, &settings)
            .expect("超长行应可进入编辑");
        assert!(info.editing);
        assert!(info.rows_total > 1, "超长行应产生显示分段");
        let payload = state.rows(info.tab_id, 0, 1).expect("取行失败");
        assert!(!payload.rows[0].text.is_empty());
        assert!(payload.rows[0].logical_row.is_some());
    }
}
