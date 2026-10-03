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
use crate::textfile::editing::batch::{
    BatchError, BatchNumberingConfig, BatchNumberingOutcome, BatchPreview,
};
use crate::textfile::editing::edit_doc::{EditApplied, EditDoc, EditError, EditOp};
use crate::textfile::editing::line_ops::{LineOpConfig, LineOpError, LineOpOutcome, LineOpPreview};
use crate::textfile::editing::save::{
    save_doc, snapshot_of, DiskSnapshot, SaveError, SaveOptions, SaveOutcome,
};
use crate::textfile::editing::search::{
    FindHit, ReplaceAllOutcome, ReplaceNextOutcome, ReplacePreview, SearchMode,
};
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
    /// 批量插入/序号错误（透传）
    #[error(transparent)]
    Batch(#[from] BatchError),
    /// 行操作错误（透传）
    #[error(transparent)]
    LineOp(#[from] LineOpError),
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
    /// 文件超过只读阈值，不允许进入编辑模式
    #[error("文件大小 {size_bytes} 字节超过只读阈值 {limit_mb} MB")]
    EditTooLarge { size_bytes: u64, limit_mb: u32 },
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
    /// 是否只读（文件超过只读阈值：可浏览、不可进入编辑）
    pub read_only: bool,
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
    /// 只读标记（文件超过只读阈值：打开/重载时按当时设置计算）
    read_only: bool,
    /// 打开/上次保存时的磁盘快照（外部修改冲突检测基准）
    edit_baseline: Option<DiskSnapshot>,
}

/// 应用运行状态。
pub struct AppState {
    tabs: BTreeMap<u64, Tab>,
    /// 标签展示顺序（tab_id 列表；拖拽排序修改）。
    /// 与 BTreeMap 解耦以支持任意展示顺序；新标签追加到末尾。
    order: Vec<u64>,
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
            order: Vec::new(),
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
        let session = FileSession::open(&canonical, None, settings.hard_limit_mb)?;
        let read_only = session.byte_len() > read_threshold_bytes(settings);
        let id = self.next_tab_id;
        self.next_tab_id += 1;
        let tab = Tab {
            id,
            session,
            edit: None,
            editing: false,
            read_only,
            edit_baseline: None,
        };
        let info = tab_info(&tab);
        self.tabs.insert(id, tab);
        self.order.push(id);
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
        if tab.read_only {
            return Err(AppStateError::EditTooLarge {
                size_bytes: tab.session.byte_len(),
                limit_mb: settings.max_file_size_mb,
            });
        }
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

    /// 在编辑文档中查找下一个命中（标准/正则两种模式；不环绕）。
    ///
    /// `from` 为显示行 UTF-16 坐标（`None` = 从文档开头）。
    pub fn find_in_tab(
        &self,
        tab_id: u64,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        from: Option<(u64, u64)>,
    ) -> Result<Option<FindHit>, AppStateError> {
        let doc = self.edit_doc(tab_id)?;
        Ok(doc.find(query, case_sensitive, mode, from)?)
    }

    /// 替换一个命中（从 `from` 起）并返回应用结果与「新落点起的下一个命中」。
    ///
    /// 未命中时返回 `None`（不产生编辑、不改变脏态）；
    /// 正则模式的 `replacement` 支持 `$1`/`${name}` 捕获展开。
    pub fn replace_next_in_tab(
        &mut self,
        tab_id: u64,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        from: Option<(u64, u64)>,
        replacement: &str,
    ) -> Result<Option<ReplaceNextOutcome>, AppStateError> {
        let doc = self.edit_doc_mut(tab_id)?;
        Ok(doc.replace_next(query, case_sensitive, mode, from, replacement)?)
    }

    /// 全部替换（单次编辑 = 单个撤销步；命中数超过上限时报 `TooManyMatches`）。
    ///
    /// 说明：IPC 层的「全部替换」走「预览 → 二次确认 → 执行」流程
    /// （`preview_replace_all_in_tab` + `replace_matches_in_tab`）；本方法保留为
    /// 直接入口（单测与将来可能的“不再询问”偏好使用）。
    pub fn replace_all_in_tab(
        &mut self,
        tab_id: u64,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        replacement: &str,
    ) -> Result<ReplaceAllOutcome, AppStateError> {
        let doc = self.edit_doc_mut(tab_id)?;
        Ok(doc.replace_all(query, case_sensitive, mode, replacement)?)
    }

    /// 生成「全部替换」预览（命中总数 + 前 `max_items` 条的前后文本）。
    pub fn preview_replace_all_in_tab(
        &self,
        tab_id: u64,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        replacement: &str,
        max_items: usize,
    ) -> Result<ReplacePreview, AppStateError> {
        let doc = self.edit_doc(tab_id)?;
        Ok(doc.preview_replace_all(query, case_sensitive, mode, replacement, max_items)?)
    }

    /// 执行「全部替换」：`indices = None` 全部；`Some` 仅替换列出的命中序号
    /// （预览弹窗中剔除个别项后使用）。`expect_state_id` 校验预览后文档未变化。
    pub fn replace_matches_in_tab(
        &mut self,
        tab_id: u64,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        replacement: &str,
        indices: Option<&[usize]>,
        expect_state_id: u64,
    ) -> Result<ReplaceAllOutcome, AppStateError> {
        let doc = self.edit_doc_mut(tab_id)?;
        Ok(doc.replace_matches(
            query,
            case_sensitive,
            mode,
            replacement,
            indices,
            expect_state_id,
        )?)
    }

    /// 显示行窗口内的命中（文档高亮用；扫描越过窗口即停止）。
    pub fn match_window_in_tab(
        &self,
        tab_id: u64,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        start_row: u64,
        count: u64,
    ) -> Result<Vec<FindHit>, AppStateError> {
        let doc = self.edit_doc(tab_id)?;
        Ok(doc.match_window(query, case_sensitive, mode, start_row, count)?)
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
        let session = FileSession::open(&canonical, Some(saved_encoding), settings.hard_limit_mb)?;
        let read_only = session.byte_len() > read_threshold_bytes(settings);
        let reopened = if read_only {
            None
        } else {
            match EditDoc::open(&canonical, Some(saved_encoding), settings.max_file_size_mb) {
                Ok(doc) => Some(doc),
                Err(err) => {
                    log::warn!(
                        target: "sread::edit",
                        "另存为后重开编辑文档失败（退出编辑模式）：{err}"
                    );
                    None
                }
            }
        };
        tab.edit_baseline = snapshot_of(&canonical)?;
        tab.editing = reopened.is_some();
        tab.edit = reopened;
        tab.session = session;
        tab.read_only = read_only;
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
        tab.session = FileSession::open(&path, override_encoding, settings.hard_limit_mb)?;
        tab.read_only = tab.session.byte_len() > read_threshold_bytes(settings);
        tab.edit = None;
        tab.editing = false;
        tab.edit_baseline = None;
        Ok(tab_info(tab))
    }

    /// 预览批量序号（P1-1；仅编辑标签，格式超限/范围非法在此阶段报错）。
    pub fn preview_batch_numbering(
        &self,
        tab_id: u64,
        config: &BatchNumberingConfig,
    ) -> Result<BatchPreview, AppStateError> {
        let doc = self.edit_doc(tab_id)?;
        Ok(doc.preview_batch_numbering(config)?)
    }

    /// 执行批量序号（单次编辑 = 单撤销步；仅编辑标签）。
    pub fn apply_batch_numbering(
        &mut self,
        tab_id: u64,
        config: &BatchNumberingConfig,
    ) -> Result<BatchNumberingOutcome, AppStateError> {
        let doc = self.edit_doc_mut(tab_id)?;
        Ok(doc.apply_batch_numbering(config)?)
    }

    /// 预览行操作（P1-2；仅编辑标签）。
    pub fn preview_line_op(
        &self,
        tab_id: u64,
        config: &LineOpConfig,
    ) -> Result<LineOpPreview, AppStateError> {
        let doc = self.edit_doc(tab_id)?;
        Ok(doc.preview_line_op(config)?)
    }

    /// 执行行操作（单次编辑 = 单撤销步；仅编辑标签）。
    pub fn apply_line_op(
        &mut self,
        tab_id: u64,
        config: &LineOpConfig,
    ) -> Result<LineOpOutcome, AppStateError> {
        let doc = self.edit_doc_mut(tab_id)?;
        Ok(doc.apply_line_op(config)?)
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

    /// 关闭标签；若关闭的是活动标签，则活动标签回落为**展示顺序**的东侧相邻
    /// （无东侧时回落西侧）。
    ///
    /// 返回：`true` 表示确实关闭了一个标签。
    pub fn close(&mut self, tab_id: u64) -> bool {
        let position = self.order.iter().position(|id| *id == tab_id);
        let removed = self.tabs.remove(&tab_id).is_some();
        if let Some(position) = position {
            self.order.remove(position);
        }
        if removed && self.active_tab == Some(tab_id) {
            // 移除后 position 恰指向原「东侧相邻」；无东侧则取西侧
            let east = position.and_then(|index| self.order.get(index).copied());
            self.active_tab = east.or_else(|| {
                position
                    .and_then(|index| index.checked_sub(1))
                    .and_then(|index| self.order.get(index).copied())
            });
        }
        removed
    }

    /// 当前活动标签 id。
    pub fn active_tab(&self) -> Option<u64> {
        self.active_tab
    }

    /// 设置活动标签（前端点击/快捷键选择同步到后端；标签不存在报错）。
    ///
    /// 说明：后端活动标签用于关闭回落与会话语义，必须与前端选择保持一致。
    pub fn set_active_tab(&mut self, tab_id: u64) -> Result<(), AppStateError> {
        if self.tabs.contains_key(&tab_id) {
            self.active_tab = Some(tab_id);
            Ok(())
        } else {
            Err(AppStateError::TabNotFound(tab_id))
        }
    }

    /// 单个标签信息（不存在返回 None）。
    pub fn tab_info(&self, tab_id: u64) -> Option<TabInfo> {
        self.tabs.get(&tab_id).map(tab_info)
    }

    /// 调整标签展示顺序（拖拽排序）。
    ///
    /// 参数：`tab_id` 被移动的标签；`to_index` 目标下标（0 起，按「移除后再插入」语义；
    /// 越界时收敛到末尾）。
    /// 错误：标签不存在（`TabNotFound`）。
    pub fn reorder(&mut self, tab_id: u64, to_index: usize) -> Result<(), AppStateError> {
        let from = self
            .order
            .iter()
            .position(|id| *id == tab_id)
            .ok_or(AppStateError::TabNotFound(tab_id))?;
        let value = self.order.remove(from);
        let target = to_index.min(self.order.len());
        self.order.insert(target, value);
        Ok(())
    }

    /// 全部标签信息（按展示顺序）。
    pub fn tabs_info(&self) -> Vec<TabInfo> {
        self.order
            .iter()
            .filter_map(|id| self.tabs.get(id))
            .map(tab_info)
            .collect()
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
        read_only: tab.read_only,
        rows_total: source.rows_total(),
        byte_len: source.byte_len(),
    }
}

/// 只读阈值字节数（超过 → 只读打开）。
fn read_threshold_bytes(settings: &AppSettings) -> u64 {
    u64::from(settings.max_file_size_mb) * 1024 * 1024
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

    /// 测试用批量序号配置（阿拉伯数字 / 全文 / 行首）。
    fn test_batch_config() -> BatchNumberingConfig {
        use crate::textfile::editing::batch::{BatchScope, InsertPosition, NumberFormat};
        BatchNumberingConfig {
            format: NumberFormat::Arabic,
            start: 1,
            step: 1,
            zero_pad_width: 2,
            separator: ".".to_string(),
            suffix: String::new(),
            position: InsertPosition::LineStart,
            scope: BatchScope::All,
            skip_empty: true,
            template: None,
            preview_lines: 10,
        }
    }

    /// 批量序号：未进入编辑 → `NotEditing` 守卫（预览与执行一致）。
    #[test]
    fn batch_numbering_requires_editing() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "batch-a.txt", "aa\nbb\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        let config = test_batch_config();
        assert!(matches!(
            state.preview_batch_numbering(info.tab_id, &config),
            Err(AppStateError::NotEditing(_))
        ));
        assert!(matches!(
            state.apply_batch_numbering(info.tab_id, &config),
            Err(AppStateError::NotEditing(_))
        ));
    }

    /// 批量序号：进入编辑后预览/执行/单撤销经状态层贯通。
    #[test]
    fn batch_numbering_through_state_and_undo() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "batch-b.txt", "aa\nbb\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        state
            .toggle_edit(info.tab_id, &settings)
            .expect("进入编辑失败");
        let config = test_batch_config();
        let preview = state
            .preview_batch_numbering(info.tab_id, &config)
            .expect("预览失败");
        assert_eq!(preview.total_rows, 2);
        assert_eq!(preview.items[0].insert_text, "1.");
        let outcome = state
            .apply_batch_numbering(info.tab_id, &config)
            .expect("执行失败");
        assert_eq!(outcome.affected, 2);
        assert!(outcome.applied.dirty);
        assert_eq!(
            state
                .edit_doc(info.tab_id)
                .expect("文档存在")
                .row_text(0)
                .as_deref(),
            Some("1.aa")
        );
        state.undo_edit(info.tab_id).expect("撤销失败");
        assert_eq!(
            state
                .edit_doc(info.tab_id)
                .expect("文档存在")
                .row_text(0)
                .as_deref(),
            Some("aa")
        );
    }

    /// 测试用行操作配置（其余字段取默认值）。
    fn test_line_op_config(op: crate::textfile::editing::line_ops::LineOp) -> LineOpConfig {
        LineOpConfig {
            op,
            ..LineOpConfig::default()
        }
    }

    /// 行操作：未编辑守卫 + 编辑后排序经状态层贯通与单撤销。
    #[test]
    fn line_op_through_state_and_undo() {
        use crate::textfile::editing::line_ops::LineOp;
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "lines-a.txt", "b\na\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info, _) = state.open_file(&path, &settings).expect("打开失败");
        let config = test_line_op_config(LineOp::Sort);
        assert!(matches!(
            state.preview_line_op(info.tab_id, &config),
            Err(AppStateError::NotEditing(_))
        ));
        state
            .toggle_edit(info.tab_id, &settings)
            .expect("进入编辑失败");
        let preview = state
            .preview_line_op(info.tab_id, &config)
            .expect("预览失败");
        assert_eq!(preview.affected_rows, 2);
        let outcome = state.apply_line_op(info.tab_id, &config).expect("执行失败");
        assert_eq!(outcome.affected, 2);
        assert!(outcome.applied.as_ref().expect("应有应用结果").dirty);
        assert_eq!(
            state
                .edit_doc(info.tab_id)
                .expect("文档存在")
                .row_text(0)
                .as_deref(),
            Some("a")
        );
        state.undo_edit(info.tab_id).expect("撤销失败");
        assert_eq!(
            state
                .edit_doc(info.tab_id)
                .expect("文档存在")
                .row_text(0)
                .as_deref(),
            Some("b")
        );
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

    /// 拖拽排序：移动后展示顺序按新下标生效；越界收敛到末尾；未知标签报错。
    #[test]
    fn reorder_moves_tab_in_display_order() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let a = write_file(dir.path(), "a.txt", "a\n");
        let b = write_file(dir.path(), "b.txt", "b\n");
        let c = write_file(dir.path(), "c.txt", "c\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info_a, _) = state.open_file(&a, &settings).expect("打开 a 失败");
        let (info_b, _) = state.open_file(&b, &settings).expect("打开 b 失败");
        let (info_c, _) = state.open_file(&c, &settings).expect("打开 c 失败");
        assert_eq!(
            state
                .tabs_info()
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["a.txt", "b.txt", "c.txt"]
        );

        // c 移到最前
        state.reorder(info_c.tab_id, 0).expect("排序失败");
        assert_eq!(
            state
                .tabs_info()
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["c.txt", "a.txt", "b.txt"]
        );

        // a 移到末尾（越界收敛）
        state.reorder(info_a.tab_id, 99).expect("排序失败");
        assert_eq!(
            state
                .tabs_info()
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["c.txt", "b.txt", "a.txt"]
        );

        // b 移到中间
        state.reorder(info_b.tab_id, 1).expect("排序失败");
        assert_eq!(
            state
                .tabs_info()
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            ["c.txt", "b.txt", "a.txt"]
        );

        assert!(matches!(
            state.reorder(9999, 0),
            Err(AppStateError::TabNotFound(9999))
        ));
    }

    /// 排序后关闭活动标签：回落按展示顺序取东侧相邻（而非 id 顺序）。
    #[test]
    fn close_after_reorder_falls_back_in_display_order() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let a = write_file(dir.path(), "a.txt", "a\n");
        let b = write_file(dir.path(), "b.txt", "b\n");
        let c = write_file(dir.path(), "c.txt", "c\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info_a, _) = state.open_file(&a, &settings).expect("打开 a 失败");
        state.open_file(&b, &settings).expect("打开 b 失败");
        let (info_c, _) = state.open_file(&c, &settings).expect("打开 c 失败");
        // 展示顺序：c, a, b；活动 = c
        state.reorder(info_c.tab_id, 0).expect("排序失败");
        assert!(state.close(info_c.tab_id));
        // 按展示顺序东侧相邻 = a（若按 id 顺序会错误回落到 b）
        assert_eq!(state.active_tab(), Some(info_a.tab_id));
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

    /// 活动标签：前端选择可同步；选择不存在的标签报错。
    #[test]
    fn set_active_tab_roundtrip() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path_a = write_file(dir.path(), "a.txt", "a\n");
        let path_b = write_file(dir.path(), "b.txt", "b\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info_a, _) = state.open_file(&path_a, &settings).expect("打开失败");
        let (info_b, _) = state.open_file(&path_b, &settings).expect("打开失败");
        assert_eq!(state.active_tab(), Some(info_b.tab_id));
        state.set_active_tab(info_a.tab_id).expect("同步失败");
        assert_eq!(state.active_tab(), Some(info_a.tab_id));
        assert!(state.set_active_tab(9999).is_err(), "不存在的标签应报错");
    }

    /// 关闭非活动标签不改变当前活动标签。
    #[test]
    fn close_non_active_keeps_active() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path_a = write_file(dir.path(), "a.txt", "a\n");
        let path_b = write_file(dir.path(), "b.txt", "b\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info_a, _) = state.open_file(&path_a, &settings).expect("打开失败");
        let (info_b, _) = state.open_file(&path_b, &settings).expect("打开失败");
        state.set_active_tab(info_b.tab_id).expect("同步失败");
        assert!(state.close(info_a.tab_id));
        assert_eq!(state.active_tab(), Some(info_b.tab_id));
    }

    /// 关闭活动标签：回落东侧相邻；无东侧时回落西侧。
    #[test]
    fn close_active_falls_back_to_east_neighbor() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path_a = write_file(dir.path(), "a.txt", "a\n");
        let path_b = write_file(dir.path(), "b.txt", "b\n");
        let path_c = write_file(dir.path(), "c.txt", "c\n");
        let mut state = AppState::new();
        let settings = AppSettings::default();
        let (info_a, _) = state.open_file(&path_a, &settings).expect("打开失败");
        let (info_b, _) = state.open_file(&path_b, &settings).expect("打开失败");
        let (info_c, _) = state.open_file(&path_c, &settings).expect("打开失败");

        state.set_active_tab(info_b.tab_id).expect("同步失败");
        assert!(state.close(info_b.tab_id));
        assert_eq!(
            state.active_tab(),
            Some(info_c.tab_id),
            "中间标签关闭应回落东侧"
        );

        state.set_active_tab(info_c.tab_id).expect("同步失败");
        assert!(state.close(info_c.tab_id));
        assert_eq!(
            state.active_tab(),
            Some(info_a.tab_id),
            "末位标签关闭应回落西侧"
        );
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

    /// 双阈值：超过只读阈值 → 只读标记 + 进入编辑被拒；超过硬上限 → 打开被拒。
    #[test]
    fn dual_threshold_read_only_and_hard_limit() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        // 2 KB 文件 + 只读阈值 0 不可设 → 用 1 MB 阈值与 2 MB 内容构造「超阈值」
        let content = "a".repeat(2 * 1024 * 1024);
        let path = write_file(dir.path(), "big.txt", &content);
        let mut state = AppState::new();
        let mut settings = AppSettings::default();
        settings.max_file_size_mb = 1; // 只读阈值 1MB
        settings.hard_limit_mb = 4; // 硬上限 4MB

        let (info, _) = state.open_file(&path, &settings).expect("应当只读打开");
        assert!(info.read_only, "2MB > 1MB 阈值应标记只读");
        assert!(matches!(
            state.toggle_edit(info.tab_id, &settings),
            Err(AppStateError::EditTooLarge { .. })
        ));

        // 超过硬上限 → 拒绝打开（全新状态，避免复用已开标签跳过校验）
        let mut strict = AppSettings::default();
        strict.max_file_size_mb = 1;
        strict.hard_limit_mb = 1;
        let mut fresh = AppState::new();
        let err = fresh
            .open_file(&path, &strict)
            .expect_err("超过硬上限应被拒绝");
        assert!(matches!(
            err,
            AppStateError::TextFile(TextFileError::TooLarge { .. })
        ));

        // 提高阈值后重载 → 恢复可编辑
        let mut relaxed = AppSettings::default();
        relaxed.max_file_size_mb = 4;
        relaxed.hard_limit_mb = 8;
        let info = state.reload_tab(info.tab_id, &relaxed).expect("重载失败");
        assert!(!info.read_only, "阈值放宽后应可编辑");
    }
}
