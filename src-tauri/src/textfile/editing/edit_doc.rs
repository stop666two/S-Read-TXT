//! 文档视图：片表上的行列映射与文本读取（编辑引擎核心）。
//!
//! 语义约定（与 `line_index` 的读模式行语义逐项对齐）：
//! - 行 = 换行单元之间的字节段；换行单元（`\n`|`\r\n`|`\r`）不属于任何行的文本；
//! - 文档以换行单元结尾时不产生末尾空行（`rows = 单元数 + 1 - 结尾换行`）；
//! - 空文档 = 1 行（空文本）。
//!
//! 位置模型：
//! - [`DocPos`] = 片段下标 + 片段内偏移（`off ∈ [0, len]`）；
//!   偏移等于片段长度时规范化为下一片段起点，文档末尾为 `(pieces.len(), 0)`；
//! - 对外（IPC 层）位置为 `(行号, UTF-16 偏移)`——UTF-16 偏移与前端 JS 字符串
//!   索引天然一致；行内偏移的字节换算在 3b 切片（编辑应用）中实现。
//!
//! 内存模型：原文片段引用 mmap（零复制）；新增片段引用只增缓冲；
//! 行数元数据 = 每片段一个 `u64`（不是每行一个）。

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::textfile::editing::fenwick::Fenwick;
use crate::textfile::editing::piece::{
    count_units, ends_with_cr, newline_width, starts_with_lf, Piece, PieceMeta, PieceSource,
};
use crate::textfile::editing::{EDIT_MAX_ROW_BYTES, UNDO_MAX_BYTES, UNDO_MAX_STEPS};
use crate::textfile::encoding::{detect, FileEncoding};
use crate::textfile::line_index::find_newline;
use crate::textfile::mmap::MappedFile;
use crate::textfile::session::TextFileError;
use crate::textfile::window::RowText;

/// 编辑引擎错误。
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    /// 打开/读取阶段的文件错误（不存在 / 超限 / IO），与只读路径共用语义
    #[error(transparent)]
    File(#[from] TextFileError),
    /// 存在超过 [`EDIT_MAX_ROW_BYTES`] 的逻辑行，拒绝进入编辑
    #[error("该文件包含超长行（{bytes} 字节），暂不支持编辑")]
    UnsupportedLongLine {
        /// 最长行字节数（诊断用）
        bytes: u64,
    },
    /// 行号越界
    #[error("行号越界：{row}")]
    RowOutOfRange {
        /// 请求的行号
        row: u64,
    },
    /// 行内字符（UTF-16）偏移越界
    #[error("字符位置越界：行 {row} 偏移 {utf16}")]
    Utf16OutOfRange {
        /// 行号
        row: u64,
        /// UTF-16 偏移
        utf16: u64,
    },
    /// 内部位置解析失败（防御性，正常流程不可达）
    #[error("内部位置无效")]
    InvalidPosition,
}

/// 编辑操作（位置坐标为「应用前」的文档状态）。
///
/// 批量约定（IPC 层与前端遵守）：
/// - 同一批次内不出现重叠或同位置的多个操作（前端合并相邻按键）；
/// - 应用顺序由引擎统一按位置降序处理，保证前序操作不移动后序位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    /// 在 `(row, utf16)` 处插入文本（UTF-8 存入新增缓冲）
    Insert {
        /// 行号
        row: u64,
        /// 行内 UTF-16 偏移（与 JS 字符串索引一致）
        utf16: u64,
        /// 插入文本
        text: String,
    },
    /// 删除 `[start, end)` 区间
    Delete {
        /// 起始行
        start_row: u64,
        /// 起始行内 UTF-16 偏移
        start_utf16: u64,
        /// 结束行
        end_row: u64,
        /// 结束行内 UTF-16 偏移
        end_utf16: u64,
    },
    /// 将 `[start, end)` 区间替换为文本
    Replace {
        /// 起始行
        start_row: u64,
        /// 起始行内 UTF-16 偏移
        start_utf16: u64,
        /// 结束行
        end_row: u64,
        /// 结束行内 UTF-16 偏移
        end_utf16: u64,
        /// 替换文本
        text: String,
    },
}

/// 一次编辑应用（或撤销/重做）后的结果（供前端局部刷新与状态栏）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditApplied {
    /// 状态版本号（每次变更都会变化，前端据此判断刷新）
    pub state_id: u64,
    /// 是否有未保存修改
    pub dirty: bool,
    /// 首个受影响行（应用前坐标；前端从该行起重取可视窗）
    pub touched_row: u64,
    /// 当前总行数
    pub rows_total: u64,
    /// 当前总字节数（不含 BOM）
    pub byte_len: u64,
}

/// 撤销/重做步骤：某个状态的完整片段列表快照（交换式撤销/重做）。
///
/// 设计取舍：片段列表经合并后通常在数十项量级，快照开销远小于逐字节
/// 复制被删除内容；且原文/新增缓冲都不可变，快照天然包含全部可恢复信息。
struct UndoStep {
    /// 该状态的片段列表
    pieces: Vec<Piece>,
    /// 对应的片段元数据
    metas: Vec<PieceMeta>,
    /// 该状态的结尾换行标记
    trailing_newline: bool,
    /// 该状态的状态版本号
    state_id: u64,
    /// 触发该状态变更的首个行号（撤销/重做时通知前端）
    touched_row: u64,
    /// 该步骤计入撤销预算的字节数（新增 + 删除 + 快照开销）
    cost_bytes: u64,
}

/// 文档位置：片段下标 + 片段内偏移。
///
/// - `piece == pieces.len()` 表示文档末尾（`off` 必为 0）；
/// - `off == pieces[piece].len` 的中间值由 [`EditDoc::normalize_pos`] 规范化。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DocPos {
    /// 片段下标（可等于片段数，表示末尾）
    piece: usize,
    /// 片段内偏移（字节）
    off: u64,
}

/// UTF-8 BOM 字节。
const BOM_UTF8: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// 文档视图。
pub struct EditDoc {
    /// 文件路径
    path: PathBuf,
    /// 原文只读映射（原文片段引用它）
    original: MappedFile,
    /// 原文编码（解码原文片段；新增片段固定 UTF-8）
    encoding: FileEncoding,
    /// BOM 长度（0/2/3；保存时原样保留，BOM 不属于任何片段）
    bom_len: u64,
    /// 片段列表（文档顺序）
    pieces: Vec<Piece>,
    /// 片段换行元数据（与 `pieces` 一一对应，计数含左邻上下文）
    metas: Vec<PieceMeta>,
    /// 新增文本缓冲（只增；UTF-8）
    added: Vec<u8>,
    /// 片段字节长度前缀和
    byte_tree: Fenwick,
    /// 片段换行单元数前缀和
    line_tree: Fenwick,
    /// 文档是否以换行单元结尾（行数换算用）
    trailing_newline: bool,
    /// 状态版本号（每次变更递增；前端刷新依据）
    state_id: u64,
    /// 状态版本号分配器（单调，不回收）
    next_state_id: u64,
    /// 最近一次保存对应的状态版本号（`None` = 从未保存）
    saved_state_id: Option<u64>,
    /// 撤销栈（快照）
    undo_stack: Vec<UndoStep>,
    /// 重做栈（快照）
    redo_stack: Vec<UndoStep>,
    /// 撤销栈当前预算占用（字节）
    undo_cost: u64,
}

impl EditDoc {
    /// 打开文件并构建初始单片段文档。
    ///
    /// 参数（与 `FileSession::open` 对齐）：
    /// - `path`：目标文件；
    /// - `encoding_override`：手动编码（`None` 自动检测）；
    /// - `max_size_mb`：允许的最大文件大小（MB）。
    ///
    /// 返回：`Ok(EditDoc)`；`Err(EditError)`（不存在 / 超限 / 超长行 / IO）。
    pub fn open(
        path: &Path,
        encoding_override: Option<FileEncoding>,
        max_size_mb: u32,
    ) -> Result<Self, EditError> {
        let mapped = MappedFile::open(path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                TextFileError::NotFound(path.to_path_buf())
            } else {
                TextFileError::Io(err)
            }
        })?;
        let limit_bytes = u64::from(max_size_mb) * 1024 * 1024;
        if mapped.len() > limit_bytes {
            return Err(TextFileError::TooLarge {
                size_bytes: mapped.len(),
                limit_mb: max_size_mb,
            }
            .into());
        }
        let encoding = encoding_override.unwrap_or_else(|| detect(mapped.bytes()));
        let bom_len = detect_bom_len(mapped.bytes(), encoding);
        ensure_editable(mapped.bytes(), encoding, bom_len)?;

        let mut pieces = Vec::new();
        let mut metas = Vec::new();
        if mapped.len() > bom_len {
            metas.push(count_units(
                mapped.bytes(),
                encoding,
                bom_len,
                mapped.len(),
                false,
            ));
            pieces.push(Piece {
                source: PieceSource::Original,
                off: bom_len,
                len: mapped.len() - bom_len,
            });
        }
        let mut doc = Self {
            path: path.to_path_buf(),
            original: mapped,
            encoding,
            bom_len,
            pieces,
            metas,
            added: Vec::new(),
            byte_tree: Fenwick::build_from(&[]),
            line_tree: Fenwick::build_from(&[]),
            trailing_newline: false,
            state_id: 1,
            next_state_id: 2,
            saved_state_id: Some(1),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            undo_cost: 0,
        };
        doc.rebuild_trees();
        doc.trailing_newline = doc.doc_ends_with_newline();
        Ok(doc)
    }

    /// 文件路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 当前生效编码（原文片段解码用；新增片段始终 UTF-8）。
    pub fn encoding(&self) -> FileEncoding {
        self.encoding
    }

    /// 保留的 BOM 长度（0/2/3；保存时原样写回）。
    pub fn bom_len(&self) -> u64 {
        self.bom_len
    }

    /// 总逻辑行数（≥1）。
    pub fn rows_total(&self) -> u64 {
        let units = self.line_tree.total();
        units + 1 - u64::from(self.trailing_newline)
    }

    /// 文档总字节数（不含 BOM；与保存后的文件大小相差 `bom_len`）。
    pub fn byte_len(&self) -> u64 {
        self.byte_tree.total()
    }

    /// 取 `[start_row, start_row + count)` 的行文本（跨片段拼接、按片段源解码）。
    ///
    /// 越界起点返回空列表；请求超出末尾时返回至末行为止。
    pub fn fetch_rows(&self, start_row: u64, count: usize) -> Vec<RowText> {
        let total = self.rows_total();
        let mut rows = Vec::new();
        let mut row = start_row;
        while rows.len() < count && row < total {
            let Ok(start) = self.row_start_pos(row) else {
                break;
            };
            let Ok(end) = self.row_end_pos(row) else {
                break;
            };
            rows.push(RowText {
                row,
                text: self.text_between(start, end),
            });
            row += 1;
        }
        rows
    }

    /// 单行文本（行号越界返回 `None`）。
    pub fn row_text(&self, row: u64) -> Option<String> {
        if row >= self.rows_total() {
            return None;
        }
        let start = self.row_start_pos(row).ok()?;
        let end = self.row_end_pos(row).ok()?;
        Some(self.text_between(start, end))
    }

    // ---- 内部：位置与映射 ----

    /// 文档末尾位置。
    fn doc_end_pos(&self) -> DocPos {
        DocPos {
            piece: self.pieces.len(),
            off: 0,
        }
    }

    /// 规范化位置：片段末尾 → 下一片段起点（保持文档末尾哨兵不变）。
    fn normalize_pos(&self, pos: DocPos) -> DocPos {
        let mut pos = pos;
        while pos.piece < self.pieces.len() && pos.off == self.pieces[pos.piece].len {
            pos = DocPos {
                piece: pos.piece + 1,
                off: 0,
            };
        }
        pos
    }

    /// 片段对应的字节源与编码。
    fn piece_bytes(&self, piece: &Piece) -> &[u8] {
        let from = piece.off as usize;
        let to = piece.end() as usize;
        match piece.source {
            PieceSource::Original => &self.original.bytes()[from..to],
            PieceSource::Added => &self.added[from..to],
        }
    }

    /// 解码片段局部范围（`[from, to)` 为片段内偏移）并追加到输出。
    fn append_decoded(&self, piece: &Piece, from: u64, to: u64, out: &mut String) {
        out.push_str(&self.decode_slice(piece, from, to));
    }

    /// 解码片段局部范围（不做 BOM 剥离：BOM 不属于任何片段，
    /// 且中段伪 BOM 序列不应被误剥离）。
    fn decode_slice(&self, piece: &Piece, from: u64, to: u64) -> String {
        let start = (piece.off + from) as usize;
        let end = (piece.off + to) as usize;
        match piece.source {
            PieceSource::Original => self
                .encoding
                .encoding()
                .decode_without_bom_handling(&self.original.bytes()[start..end])
                .0
                .into_owned(),
            PieceSource::Added => String::from_utf8_lossy(&self.added[start..end]).into_owned(),
        }
    }

    /// 文档是否以换行单元结尾。
    fn doc_ends_with_newline(&self) -> bool {
        let Some(piece) = self.pieces.last() else {
            return false;
        };
        let bytes = self.piece_bytes(piece);
        if bytes.is_empty() {
            return false;
        }
        match self.encoding_for(piece) {
            FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
                if bytes.len() < 2 {
                    return false;
                }
                let last = u16::from_le_bytes([bytes[bytes.len() - 2], bytes[bytes.len() - 1]]);
                let last = if self.encoding_for(piece) == FileEncoding::Utf16Le {
                    last
                } else {
                    u16::from_be_bytes([bytes[bytes.len() - 2], bytes[bytes.len() - 1]])
                };
                last == 0x000A || last == 0x000D
            }
            _ => matches!(bytes[bytes.len() - 1], b'\n' | b'\r'),
        }
    }

    /// 片段计数/扫描使用的编码（新增片段固定 UTF-8）。
    fn encoding_for(&self, piece: &Piece) -> FileEncoding {
        match piece.source {
            PieceSource::Original => self.encoding,
            PieceSource::Added => FileEncoding::Utf8,
        }
    }

    /// 片段 `piece_index` 的左侧上下文（前一片段是否以 `\r` 结尾）。
    fn prev_cr(&self, piece_index: usize) -> bool {
        if piece_index == 0 {
            false
        } else {
            self.metas[piece_index - 1].ends_with_cr
        }
    }

    /// 第 `unit` 个（0-based）换行单元的 `(起点, 终点)` 文档位置。
    ///
    /// 仅对 `unit < 单元总数` 有效（调用方保证）。
    fn unit_span(&self, unit: u64) -> Result<(DocPos, DocPos), EditError> {
        if unit >= self.line_tree.total() {
            return Err(EditError::InvalidPosition);
        }
        let piece_index = self.line_tree.lower_bound(unit);
        let prefix = self.line_tree.prefix(piece_index);
        let local = unit - prefix;
        let piece = self.pieces[piece_index];
        let bytes = self.piece_bytes(&piece);
        let (start_off, end_off) = scan_unit_span(
            bytes,
            self.encoding_for(&piece),
            local,
            self.prev_cr(piece_index),
        );
        // 跨片 `\r`+`\n`：单元在片段末尾以 `\r` 结束时，若下一片段以 `\n` 开头，
        // 该 `\n` 属于同一换行单元——行起点（单元终点）必须越过它
        let mut end_pos = DocPos {
            piece: piece_index,
            off: end_off,
        };
        if end_off == piece.len && ends_with_cr(bytes, self.encoding_for(&piece), 0, piece.len) {
            if let Some(next) = self.pieces.get(piece_index + 1) {
                let next_bytes = self.piece_bytes(next);
                let next_encoding = self.encoding_for(next);
                if starts_with_lf(next_bytes, next_encoding, 0, next.len) {
                    end_pos = DocPos {
                        piece: piece_index + 1,
                        off: newline_width(next_encoding),
                    };
                }
            }
        }
        Ok((
            DocPos {
                piece: piece_index,
                off: start_off,
            },
            self.normalize_pos(end_pos),
        ))
    }

    /// 第 `row` 行的起始位置（`row == 0` 为文档起点，其后为第 `row-1` 个单元终点）。
    fn row_start_pos(&self, row: u64) -> Result<DocPos, EditError> {
        if row >= self.rows_total() {
            return Err(EditError::RowOutOfRange { row });
        }
        if row == 0 {
            return Ok(DocPos { piece: 0, off: 0 });
        }
        let (_, end) = self.unit_span(row - 1)?;
        Ok(end)
    }

    /// 第 `row` 行的结束位置（不含换行单元）。
    ///
    /// 语义：行 `row` 的终点 = 下一个换行单元的起点；
    /// 若其后没有换行单元（末行且无结尾换行）则为文档末尾。
    fn row_end_pos(&self, row: u64) -> Result<DocPos, EditError> {
        let units = self.line_tree.total();
        if row < units {
            let (start, _) = self.unit_span(row)?;
            Ok(start)
        } else {
            Ok(self.doc_end_pos())
        }
    }

    /// 拼接 `[start, end)` 的文本（起点与终点均为文档位置；`start == end` 返回空串）。
    fn text_between(&self, start: DocPos, end: DocPos) -> String {
        let mut text = String::new();
        let mut piece_index = start.piece;
        let mut offset = start.off;
        while piece_index < end.piece || (piece_index == end.piece && offset < end.off) {
            let piece = self.pieces[piece_index];
            let stop = if piece_index == end.piece {
                end.off
            } else {
                piece.len
            };
            self.append_decoded(&piece, offset, stop, &mut text);
            piece_index += 1;
            offset = 0;
        }
        text
    }

    /// 重建两个前缀和树（片段列表变化后调用）。
    fn rebuild_trees(&mut self) {
        let byte_lens: Vec<u64> = self.pieces.iter().map(|piece| piece.len).collect();
        let unit_counts: Vec<u64> = self.metas.iter().map(|meta| meta.units).collect();
        self.byte_tree = Fenwick::build_from(&byte_lens);
        self.line_tree = Fenwick::build_from(&unit_counts);
    }

    // ---- 编辑应用与撤销/重做（阶段 3b）----

    /// 当前状态版本号（单调递增；前端刷新依据）。
    pub fn state_id(&self) -> u64 {
        self.state_id
    }

    /// 是否有未保存修改（与 `mark_saved` 配对）。
    pub fn is_dirty(&self) -> bool {
        self.saved_state_id != Some(self.state_id)
    }

    /// 标记「当前状态已保存」（保存链调用）。
    pub fn mark_saved(&mut self) {
        self.saved_state_id = Some(self.state_id);
    }

    /// 应用一批编辑操作（一个批次 = 一个撤销步骤）。
    ///
    /// 返回：`EditApplied`（新状态版本 / 是否脏 / 首个受影响行 / 总行数 / 总字节数）。
    /// 错误：`EditError`（行/字符越界等）。批次为原子操作：任一操作解析失败则整批不应用。
    pub fn apply_edits(&mut self, ops: &[EditOp]) -> Result<EditApplied, EditError> {
        if ops.is_empty() {
            return Ok(self.applied(0));
        }
        // 1) 解析所有操作（相对编辑前状态）：全局字节区间 + 替换文本 + 受影响行
        let mut resolved: Vec<(u64, u64, Vec<u8>, u64)> = Vec::with_capacity(ops.len());
        for op in ops {
            let (start, end, text, touched_row) = match op {
                EditOp::Insert { row, utf16, text } => {
                    let pos = self.resolve_pos(*row, *utf16)?;
                    let global = self.global_offset(pos);
                    (global, global, text.clone().into_bytes(), *row)
                }
                EditOp::Delete {
                    start_row,
                    start_utf16,
                    end_row,
                    end_utf16,
                } => {
                    let a = self.global_offset(self.resolve_pos(*start_row, *start_utf16)?);
                    let b = self.global_offset(self.resolve_pos(*end_row, *end_utf16)?);
                    if b < a {
                        return Err(EditError::InvalidPosition);
                    }
                    (a, b, Vec::new(), *start_row)
                }
                EditOp::Replace {
                    start_row,
                    start_utf16,
                    end_row,
                    end_utf16,
                    text,
                } => {
                    let a = self.global_offset(self.resolve_pos(*start_row, *start_utf16)?);
                    let b = self.global_offset(self.resolve_pos(*end_row, *end_utf16)?);
                    if b < a {
                        return Err(EditError::InvalidPosition);
                    }
                    (a, b, text.clone().into_bytes(), *start_row)
                }
            };
            resolved.push((start, end, text, touched_row));
        }
        let touched = resolved.iter().map(|item| item.3).min().unwrap_or(0);
        // 2) 快照当前状态（撤销步骤）
        let mut before = self.take_snapshot(touched);
        // 3) 按位置降序应用（后面的编辑不影响前面位置）
        let mut cost = 0u64;
        resolved.sort_by(|left, right| right.0.cmp(&left.0));
        for (start, end, text, _) in &resolved {
            cost += text.len() as u64 + (end - start);
            self.apply_range(*start, *end, text);
        }
        // 4) 合并、重建、版本推进、裁剪撤销预算
        self.coalesce();
        self.rebuild_trees();
        self.trailing_newline = self.doc_ends_with_newline();
        self.state_id = self.next_state_id;
        self.next_state_id += 1;
        self.redo_stack.clear();
        let snapshot_cost = (before.pieces.len() as u64)
            * ((std::mem::size_of::<Piece>() + std::mem::size_of::<PieceMeta>()) as u64);
        before.cost_bytes = cost + snapshot_cost;
        self.undo_cost += before.cost_bytes;
        self.undo_stack.push(before);
        self.trim_undo();
        Ok(self.applied(touched))
    }

    /// 撤销一步；无可撤销时返回 `None`。
    pub fn undo(&mut self) -> Option<EditApplied> {
        let step = self.undo_stack.pop()?;
        let touched = step.touched_row;
        self.undo_cost = self.undo_cost.saturating_sub(step.cost_bytes);
        let for_redo = self.swap_state(step);
        self.redo_stack.push(for_redo);
        self.rebuild_trees();
        Some(self.applied(touched))
    }

    /// 重做一步；无可重做时返回 `None`。
    pub fn redo(&mut self) -> Option<EditApplied> {
        let step = self.redo_stack.pop()?;
        let touched = step.touched_row;
        let for_undo = self.swap_state(step);
        self.undo_cost += for_undo.cost_bytes;
        self.undo_stack.push(for_undo);
        self.rebuild_trees();
        Some(self.applied(touched))
    }

    /// 片段列表公开只读访问（保存链使用）。
    pub(crate) fn pieces(&self) -> &[Piece] {
        &self.pieces
    }

    /// 新增缓冲只读访问（保存链使用）。
    pub(crate) fn added(&self) -> &[u8] {
        &self.added
    }

    /// 原文映射字节只读访问（保存链使用）。
    pub(crate) fn original_bytes(&self) -> &[u8] {
        self.original.bytes()
    }

    /// 构造对外的编辑结果。
    fn applied(&self, touched_row: u64) -> EditApplied {
        EditApplied {
            state_id: self.state_id,
            dirty: self.is_dirty(),
            touched_row,
            rows_total: self.rows_total(),
            byte_len: self.byte_len(),
        }
    }

    /// 截取当前状态的快照（撤销步骤载体）。
    fn take_snapshot(&self, touched_row: u64) -> UndoStep {
        UndoStep {
            pieces: self.pieces.clone(),
            metas: self.metas.clone(),
            trailing_newline: self.trailing_newline,
            state_id: self.state_id,
            touched_row,
            cost_bytes: 0,
        }
    }

    /// 交换当前状态与步骤快照，返回「交换出去的当前状态」构造的新步骤
    /// （撤销/重做共用；实现为状态互换）。
    fn swap_state(&mut self, step: UndoStep) -> UndoStep {
        let current = UndoStep {
            pieces: std::mem::take(&mut self.pieces),
            metas: std::mem::take(&mut self.metas),
            trailing_newline: self.trailing_newline,
            state_id: self.state_id,
            touched_row: step.touched_row,
            cost_bytes: step.cost_bytes,
        };
        self.pieces = step.pieces;
        self.metas = step.metas;
        self.trailing_newline = step.trailing_newline;
        self.state_id = step.state_id;
        current
    }

    /// 撤销预算裁剪（步数与字节双上限，先到先裁剪；从最旧步骤开始丢弃）。
    fn trim_undo(&mut self) {
        while self.undo_stack.len() > UNDO_MAX_STEPS || self.undo_cost > UNDO_MAX_BYTES {
            let dropped = self.undo_stack.remove(0);
            self.undo_cost = self.undo_cost.saturating_sub(dropped.cost_bytes);
        }
    }

    /// 文档位置 → 全局字节偏移（片段前缀和 + 片段内偏移）。
    fn global_offset(&self, pos: DocPos) -> u64 {
        self.byte_tree.prefix(pos.piece) + pos.off
    }

    /// 将全局字节偏移规范化为片段边界，返回边界处的片段下标。
    fn boundary_at(&mut self, global: u64) -> usize {
        if self.pieces.is_empty() {
            return 0;
        }
        if global >= self.byte_tree.total() {
            return self.pieces.len();
        }
        let index = self.byte_tree.lower_bound(global);
        let prefix = self.byte_tree.prefix(index);
        let local = global - prefix;
        if local == 0 {
            return index;
        }
        let piece = self.pieces[index];
        if local >= piece.len {
            return index + 1;
        }
        self.split_piece(index, local);
        // 拆分改变了片段列表：立即重建前缀和，保证同批次内后续边界定位正确
        // （全局偏移语义不因拆分改变，前缀和在变更点之前的取值保持不变）
        self.rebuild_trees();
        index + 1
    }

    /// 在片段内 `off`（0 < off < len）处拆分。
    ///
    /// 换行计数按**较小侧扫描**推导另一侧（大文件顶端编辑不产生整片段重扫）：
    /// `left + right = parent`（上下文一致），扫描较小侧即可。
    fn split_piece(&mut self, index: usize, off: u64) {
        let piece = self.pieces[index];
        let meta = self.metas[index];
        let bytes = self.piece_bytes(&piece);
        let encoding = self.encoding_for(&piece);
        let left_len = off;
        let right_len = piece.len - off;
        let (left_meta, right_meta) = if left_len <= right_len {
            let left_meta = count_units(bytes, encoding, 0, left_len, self.prev_cr(index));
            let right_meta = PieceMeta {
                units: meta.units - left_meta.units,
                ends_with_cr: meta.ends_with_cr,
            };
            (left_meta, right_meta)
        } else {
            let left_ends = ends_with_cr(bytes, encoding, 0, left_len);
            let right_meta = count_units(bytes, encoding, left_len, piece.len, left_ends);
            let left_meta = PieceMeta {
                units: meta.units - right_meta.units,
                ends_with_cr: left_ends,
            };
            (left_meta, right_meta)
        };
        self.pieces[index] = Piece {
            source: piece.source,
            off: piece.off,
            len: left_len,
        };
        self.pieces.insert(
            index + 1,
            Piece {
                source: piece.source,
                off: piece.off + left_len,
                len: right_len,
            },
        );
        self.metas[index] = left_meta;
        self.metas.insert(index + 1, right_meta);
    }

    /// 在全局区间 `[start, end)` 上应用一次替换（`text` 为空 = 纯删除）。
    fn apply_range(&mut self, start: u64, end: u64, text: &[u8]) {
        if text.is_empty() && start == end {
            return;
        }
        // 顺序要求：先定位起始边界（其拆分只会在其后插入片段，不影响已取得的 a），
        // 再定位结束边界（按全局偏移在新片段列表上重新定位）。
        let a = self.boundary_at(start);
        let b = self.boundary_at(end).max(a);
        // 旧右邻（原 `b` 处片段）在操作前的左上下文
        let old_flag = if b > 0 {
            self.metas[b - 1].ends_with_cr
        } else {
            false
        };
        if a < b {
            self.pieces.drain(a..b);
            self.metas.drain(a..b);
        }
        let mut next = a;
        if !text.is_empty() {
            let left_flag = if a > 0 {
                self.metas[a - 1].ends_with_cr
            } else {
                false
            };
            let off = self.added.len() as u64;
            self.added.extend_from_slice(text);
            let meta = count_units(text, FileEncoding::Utf8, 0, text.len() as u64, left_flag);
            self.pieces.insert(
                a,
                Piece {
                    source: PieceSource::Added,
                    off,
                    len: text.len() as u64,
                },
            );
            self.metas.insert(a, meta);
            next = a + 1;
        }
        // 右邻上下文修正：仅当左侧 `\r` 状态变化且右邻以 `\n` 开头时 ±1
        if next < self.pieces.len() {
            let new_flag = if next > 0 {
                self.metas[next - 1].ends_with_cr
            } else {
                false
            };
            if old_flag != new_flag {
                let piece = self.pieces[next];
                let bytes = self.piece_bytes(&piece);
                let encoding = self.encoding_for(&piece);
                if starts_with_lf(bytes, encoding, 0, piece.len) {
                    if new_flag {
                        self.metas[next].units -= 1;
                    } else {
                        self.metas[next].units += 1;
                    }
                }
            }
        }
        // 区间删除/插入改变了片段列表：立即重建前缀和（同批次后续操作按
        // 全局偏移重定位；变更点之前的前缀和保持不变）
        self.rebuild_trees();
    }

    /// 解析 `(行号, 行内 UTF-16 偏移)` → 文档位置。
    ///
    /// - `utf16` 落在代理对中间时吸附到字符起点（前端正常输入不会出现）；
    /// - 行尾（`utf16 == 行长`）返回行结束位置（换行单元之前）。
    fn resolve_pos(&self, row: u64, utf16: u64) -> Result<DocPos, EditError> {
        if row >= self.rows_total() {
            return Err(EditError::RowOutOfRange { row });
        }
        let start = self.row_start_pos(row)?;
        if utf16 == 0 {
            return Ok(start);
        }
        let end = self.row_end_pos(row)?;
        let mut remaining = utf16;
        let mut pos = start;
        while pos.piece < self.pieces.len() {
            if pos.piece == end.piece && pos.off >= end.off {
                break;
            }
            let piece = self.pieces[pos.piece];
            let stop = if pos.piece == end.piece {
                end.off
            } else {
                piece.len
            };
            let text = self.decode_slice(&piece, pos.off, stop);
            let encoding = self.encoding_for(&piece);
            let mut prefix = String::new();
            for ch in text.chars() {
                let width = ch.len_utf16() as u64;
                if remaining < width {
                    // 落在代理对中间：吸附到字符起点
                    let byte_off = pos.off + encoded_len(&prefix, encoding);
                    return Ok(self.normalize_pos(DocPos {
                        piece: pos.piece,
                        off: byte_off,
                    }));
                }
                remaining -= width;
                prefix.push(ch);
                if remaining == 0 {
                    let byte_off = pos.off + encoded_len(&prefix, encoding);
                    return Ok(self.normalize_pos(DocPos {
                        piece: pos.piece,
                        off: byte_off,
                    }));
                }
            }
            pos = DocPos {
                piece: pos.piece + 1,
                off: 0,
            };
        }
        Err(EditError::Utf16OutOfRange { row, utf16 })
    }

    /// 合并相邻可合并片段（同源且字节连续；不同源或间隔的不动）。
    ///
    /// 计数合并规则：`units` 直接相加（两者本就在同一上下文中相邻），
    /// 结尾标记取右片段（左片段因此不产生额外边界）。
    fn coalesce(&mut self) {
        let mut index = 0;
        while index + 1 < self.pieces.len() {
            let left = self.pieces[index];
            let right = self.pieces[index + 1];
            if left.source == right.source && left.end() == right.off {
                self.pieces[index].len = left.len + right.len;
                self.metas[index].units += self.metas[index + 1].units;
                self.metas[index].ends_with_cr = self.metas[index + 1].ends_with_cr;
                self.pieces.remove(index + 1);
                self.metas.remove(index + 1);
            } else {
                index += 1;
            }
        }
    }
}

/// 字符串在指定编码下的字节长度（UTF-16 按代码单元 ×2；传统编码经编码器）。
fn encoded_len(text: &str, encoding: FileEncoding) -> u64 {
    match encoding {
        FileEncoding::Utf8 => text.len() as u64,
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
            (text.chars().map(|ch| ch.len_utf16() as u64).sum::<u64>()) * 2
        }
        _ => encoding.encoding().encode(text).0.len() as u64,
    }
}

/// 在片段内前进到第 `unit_index` 个（0-based）换行单元，返回 `(单元起点, 单元终点)`。
///
/// `prev_cr` 与 [`count_units`] 的上下文一致（抑制前导 `\n`）。
fn scan_unit_span(
    bytes: &[u8],
    encoding: FileEncoding,
    unit_index: u64,
    prev_cr: bool,
) -> (u64, u64) {
    let len = bytes.len() as u64;
    let mut pos = 0u64;
    if prev_cr && starts_with_lf(bytes, encoding, 0, len) {
        pos += newline_width(encoding);
    }
    let mut unit_start = pos;
    for _ in 0..=unit_index {
        match find_newline(bytes, encoding, pos) {
            Some((newline_start, newline_end)) => {
                unit_start = newline_start;
                pos = newline_end.min(len);
            }
            None => {
                unit_start = pos;
                break;
            }
        }
    }
    (unit_start, pos)
}

/// 探测 BOM 长度（仅当检测/指定编码与 BOM 匹配时）。
fn detect_bom_len(bytes: &[u8], encoding: FileEncoding) -> u64 {
    match encoding {
        FileEncoding::Utf8 if bytes.starts_with(&BOM_UTF8) => 3,
        FileEncoding::Utf16Le if bytes.starts_with(&[0xFF, 0xFE]) => 2,
        FileEncoding::Utf16Be if bytes.starts_with(&[0xFE, 0xFF]) => 2,
        _ => 0,
    }
}

/// 可编辑性守卫：任一逻辑行超过 [`EDIT_MAX_ROW_BYTES`] 即拒绝（提前退出）。
fn ensure_editable(bytes: &[u8], encoding: FileEncoding, start: u64) -> Result<(), EditError> {
    let len = bytes.len() as u64;
    let mut pos = start;
    while pos < len {
        match find_newline(bytes, encoding, pos) {
            Some((newline_start, next)) => {
                if newline_start - pos > EDIT_MAX_ROW_BYTES {
                    return Err(EditError::UnsupportedLongLine {
                        bytes: newline_start - pos,
                    });
                }
                pos = next.min(len);
            }
            None => {
                if len - pos > EDIT_MAX_ROW_BYTES {
                    return Err(EditError::UnsupportedLongLine { bytes: len - pos });
                }
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textfile::line_index::RowIndex;
    use crate::textfile::window::fetch_rows as read_fetch_rows;
    use std::path::PathBuf;

    /// 写测试文件并返回路径。
    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("写测试文件失败");
        path
    }

    /// UTF-16LE 编码（测试辅助：手工双字节，不能用 encoding_rs 的 Web 表单语义）。
    fn encode_utf16le(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect()
    }

    /// 与读模式的 RowIndex 做一致性断言（同一字节流、行模型相同）。
    fn assert_consistent_with_read_mode(bytes: &[u8], encoding: FileEncoding) {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "sample.txt", bytes);
        let doc = EditDoc::open(&path, Some(encoding), 100).expect("打开失败");
        let index = RowIndex::build(bytes, encoding);
        assert_eq!(
            doc.rows_total(),
            index.rows_total(),
            "行数不一致：doc={} index={}",
            doc.rows_total(),
            index.rows_total()
        );
        let expected = read_fetch_rows(bytes, &index, 0, index.rows_total() as usize);
        let actual = doc.fetch_rows(0, index.rows_total() as usize);
        assert_eq!(actual, expected, "行文本不一致");
    }

    /// 换行族混合 + 连续空行：与读模式逐行一致。
    #[test]
    fn newline_families_match_read_mode() {
        assert_consistent_with_read_mode(b"a\r\nb\rc\nd", FileEncoding::Utf8);
        assert_consistent_with_read_mode(b"\n\n", FileEncoding::Utf8);
        assert_consistent_with_read_mode(b"a\n\nb", FileEncoding::Utf8);
        assert_consistent_with_read_mode(b"a\n", FileEncoding::Utf8);
        assert_consistent_with_read_mode(b"a\r", FileEncoding::Utf8);
        assert_consistent_with_read_mode(b"", FileEncoding::Utf8);
        assert_consistent_with_read_mode("中文一\r\n中文二\n中文三".as_bytes(), FileEncoding::Utf8);
    }

    /// GB18030 一致性。
    #[test]
    fn gb18030_matches_read_mode() {
        let text = "第一行中文\r\n第二行中文\r第三行中文\n结束";
        let bytes = encoding_rs::GB18030.encode(text).0.into_owned();
        assert_consistent_with_read_mode(&bytes, FileEncoding::Gb18030);
    }

    /// UTF-16LE 一致性（含 CRLF 双单元）。
    #[test]
    fn utf16le_matches_read_mode() {
        let bytes = encode_utf16le("甲\r\n乙\r丙\n丁");
        assert_consistent_with_read_mode(&bytes, FileEncoding::Utf16Le);
    }

    /// BOM：不进入行文本，行映射与无 BOM 相同。
    #[test]
    fn bom_is_excluded_from_rows() {
        let mut bytes = BOM_UTF8.to_vec();
        bytes.extend_from_slice("首行\n次行".as_bytes());
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "bom.txt", &bytes);
        let doc = EditDoc::open(&path, None, 100).expect("打开失败");
        assert_eq!(doc.encoding(), FileEncoding::Utf8);
        assert_eq!(doc.rows_total(), 2);
        assert_eq!(doc.row_text(0).as_deref(), Some("首行"));
        assert_eq!(doc.row_text(1).as_deref(), Some("次行"));
        // 文档字节数不含 BOM
        assert_eq!(doc.byte_len(), "首行\n次行".len() as u64);
        assert_eq!(doc.bom_len(), 3);
    }

    /// 空文件：1 空行。
    #[test]
    fn empty_file_has_one_empty_row() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "empty.txt", b"");
        let doc = EditDoc::open(&path, None, 100).expect("打开失败");
        assert_eq!(doc.rows_total(), 1);
        assert_eq!(doc.fetch_rows(0, 5).len(), 1);
        assert_eq!(doc.row_text(0).as_deref(), Some(""));
        assert_eq!(doc.byte_len(), 0);
    }

    /// 超长行拒绝编辑（> 64KB），但错误携带字节数。
    #[test]
    fn long_line_is_rejected() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let mut bytes = vec![b'x'; (EDIT_MAX_ROW_BYTES + 1) as usize];
        bytes.push(b'\n');
        let path = write_file(dir.path(), "long.txt", &bytes);
        match EditDoc::open(&path, None, 100).err() {
            Some(EditError::UnsupportedLongLine { bytes: reported }) => {
                assert_eq!(reported, EDIT_MAX_ROW_BYTES + 1);
            }
            other => panic!("应拒绝编辑：{other:?}"),
        }
        // 64KB 整行可编辑
        let dir2 = tempfile::tempdir().expect("创建临时目录失败");
        let ok_bytes = vec![b'y'; EDIT_MAX_ROW_BYTES as usize];
        let ok_path = write_file(dir2.path(), "ok.txt", &ok_bytes);
        assert!(EditDoc::open(&ok_path, None, 100).is_ok());
    }

    /// 行定位越界与 fetch 越界行为。
    #[test]
    fn row_bounds() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "b.txt", b"a\nb");
        let doc = EditDoc::open(&path, None, 100).expect("打开失败");
        assert!(doc.fetch_rows(2, 3).is_empty());
        assert!(doc.fetch_rows(99, 3).is_empty());
        assert_eq!(doc.row_text(2), None);
        assert!(matches!(
            doc.row_start_pos(2),
            Err(EditError::RowOutOfRange { row: 2 })
        ));
    }

    /// 50MB 上限错误透传（与只读路径同语义）。
    #[test]
    fn too_large_is_rejected() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "big.txt", b"0123456789");
        match EditDoc::open(&path, None, 0).err() {
            Some(EditError::File(TextFileError::TooLarge {
                size_bytes,
                limit_mb,
            })) => {
                assert_eq!(size_bytes, 10);
                assert_eq!(limit_mb, 0);
            }
            other => panic!("应为 TooLarge：{other:?}"),
        }
    }

    /// 缺失文件 → NotFound。
    #[test]
    fn missing_file() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        match EditDoc::open(&dir.path().join("none.txt"), None, 100).err() {
            Some(EditError::File(TextFileError::NotFound(_))) => {}
            other => panic!("应为 NotFound：{other:?}"),
        }
    }

    /// `unit_span` 与行起止的边界关系（直接验证位置语义）。
    #[test]
    fn unit_span_edges() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "s.txt", b"a\nb\n");
        let doc = EditDoc::open(&path, None, 100).expect("打开失败");
        // 单元 0 = "\n" 位于 [1,2)
        let (start, end) = doc.unit_span(0).expect("单元 0 存在");
        assert_eq!((start.piece, start.off), (0, 1));
        assert_eq!((end.piece, end.off), (0, 2));
        // 行 1 起点 = 单元 0 终点
        let row1 = doc.row_start_pos(1).expect("行 1 存在");
        assert_eq!((row1.piece, row1.off), (0, 2));
        // 行 0 终点 = 单元 0 起点
        let row0_end = doc.row_end_pos(0).expect("行 0 存在");
        assert_eq!((row0_end.piece, row0_end.off), (0, 1));
        // 行 1 终点 = 单元 1 起点 = 3
        let row1_end = doc.row_end_pos(1).expect("行 1 存在");
        assert_eq!((row1_end.piece, row1_end.off), (0, 3));
    }

    /// 构造可编辑文档（测试辅助）。
    fn open_doc(bytes: &[u8], encoding: Option<FileEncoding>) -> (tempfile::TempDir, EditDoc) {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "edit.txt", bytes);
        let doc = EditDoc::open(&path, encoding, 100).expect("打开失败");
        (dir, doc)
    }

    /// 基础插入 + 撤销 + 重做 + 脏标记。
    #[test]
    fn insert_undo_redo_cycle() {
        let (_dir, mut doc) = open_doc(b"abcdef", None);
        assert!(!doc.is_dirty());
        let result = doc
            .apply_edits(&[EditOp::Insert {
                row: 0,
                utf16: 2,
                text: "XY".into(),
            }])
            .expect("插入失败");
        assert!(result.dirty);
        assert_eq!(result.touched_row, 0);
        assert_eq!(doc.row_text(0).as_deref(), Some("abXYcdef"));
        let undone = doc.undo().expect("应可撤销");
        assert!(!undone.dirty);
        assert_eq!(doc.row_text(0).as_deref(), Some("abcdef"));
        let redone = doc.redo().expect("应可重做");
        assert!(redone.dirty);
        assert_eq!(doc.row_text(0).as_deref(), Some("abXYcdef"));
        assert!(doc.redo().is_none());
    }

    /// 换行插入（行拆分）+ 跨行删除（行合并）。
    #[test]
    fn split_and_merge_rows() {
        let (_dir, mut doc) = open_doc(b"abcdef", None);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 3,
            text: "\n".into(),
        }])
        .expect("插入换行失败");
        assert_eq!(doc.rows_total(), 2);
        assert_eq!(doc.row_text(0).as_deref(), Some("abc"));
        assert_eq!(doc.row_text(1).as_deref(), Some("def"));
        doc.apply_edits(&[EditOp::Delete {
            start_row: 0,
            start_utf16: 2,
            end_row: 1,
            end_utf16: 1,
        }])
        .expect("跨行删除失败");
        assert_eq!(doc.rows_total(), 1);
        assert_eq!(doc.row_text(0).as_deref(), Some("abef"));
    }

    /// 批量操作 = 单撤销步（降序应用互不影响）。
    #[test]
    fn batch_is_single_undo_step() {
        let (_dir, mut doc) = open_doc(b"aaa\nbbb\nccc", None);
        doc.apply_edits(&[
            EditOp::Insert {
                row: 0,
                utf16: 1,
                text: "1".into(),
            },
            EditOp::Insert {
                row: 2,
                utf16: 1,
                text: "3".into(),
            },
        ])
        .expect("批量插入失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("a1aa"));
        assert_eq!(doc.row_text(2).as_deref(), Some("c3cc"));
        doc.undo().expect("应可撤销");
        assert_eq!(doc.row_text(0).as_deref(), Some("aaa"));
        assert_eq!(doc.row_text(2).as_deref(), Some("ccc"));
    }

    /// 替换操作。
    #[test]
    fn replace_range() {
        let (_dir, mut doc) = open_doc(b"hello world", None);
        doc.apply_edits(&[EditOp::Replace {
            start_row: 0,
            start_utf16: 6,
            end_row: 0,
            end_utf16: 11,
            text: "S-Read".into(),
        }])
        .expect("替换失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("hello S-Read"));
    }

    /// GB18030：行内 UTF-16→字节换算 + 混合片段解码。
    #[test]
    fn gb18030_insert_mixed_decode() {
        let text = "中文测试内容";
        let bytes = encoding_rs::GB18030.encode(text).0.into_owned();
        let (_dir, mut doc) = open_doc(&bytes, None);
        assert_eq!(doc.encoding(), FileEncoding::Gb18030);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 2,
            text: "-插入-".into(),
        }])
        .expect("插入失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("中文-插入-测试内容"));
        doc.undo().expect("应可撤销");
        assert_eq!(doc.row_text(0).as_deref(), Some("中文测试内容"));
    }

    /// UTF-16：混合片段 + 代理对偏移吸附。
    #[test]
    fn utf16_edit_and_surrogate_clamp() {
        let bytes = encode_utf16le("a😀b\n第二行");
        let (_dir, mut doc) = open_doc(&bytes, Some(FileEncoding::Utf16Le));
        assert_eq!(doc.row_text(0).as_deref(), Some("a😀b"));
        // UTF-16 偏移 2 落在代理对内部：吸附到字符起点（a 之后）
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 2,
            text: "X".into(),
        }])
        .expect("插入失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("aX😀b"));
        // 行尾偏移
        doc.apply_edits(&[EditOp::Insert {
            row: 1,
            utf16: 3,
            text: "!".into(),
        }])
        .expect("插入失败");
        assert_eq!(doc.row_text(1).as_deref(), Some("第二行!"));
    }

    /// 跨片 CRLF 安全：手工拆分 `\r`|`\n`，验证计数与拼合。
    #[test]
    fn cross_piece_crlf_safety() {
        let (_dir, mut doc) = open_doc(b"a\r\nb", None);
        // 直接删掉 "\n"（全局偏移 2..3），使 \r 与 b 之间形成片段边界
        doc.apply_range(2, 3, b"");
        doc.coalesce();
        doc.rebuild_trees();
        doc.trailing_newline = doc.doc_ends_with_newline();
        assert_eq!(doc.rows_total(), 2);
        assert_eq!(doc.row_text(0).as_deref(), Some("a"));
        assert_eq!(doc.row_text(1).as_deref(), Some("b"));
        // 重新插回 "\n"：跨片 CRLF 拼合，行数不变
        doc.apply_range(2, 2, b"\n");
        doc.coalesce();
        doc.rebuild_trees();
        doc.trailing_newline = doc.doc_ends_with_newline();
        assert_eq!(doc.rows_total(), 2);
        assert_eq!(doc.row_text(0).as_deref(), Some("a"));
        assert_eq!(doc.row_text(1).as_deref(), Some("b"));
    }

    /// 状态版本单调、脏标记与保存标记联动、撤销后新编辑清空重做。
    #[test]
    fn dirty_and_state_id() {
        let (_dir, mut doc) = open_doc(b"x", None);
        let initial = doc.state_id();
        assert!(!doc.is_dirty());
        let first = doc
            .apply_edits(&[EditOp::Insert {
                row: 0,
                utf16: 1,
                text: "y".into(),
            }])
            .expect("插入失败");
        assert!(first.state_id > initial);
        assert!(doc.is_dirty());
        doc.mark_saved();
        assert!(!doc.is_dirty());
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 0,
            text: "z".into(),
        }])
        .expect("插入失败");
        assert!(doc.is_dirty());
        doc.undo().expect("应可撤销");
        assert!(!doc.is_dirty());
        doc.undo().expect("应可撤销");
        assert_eq!(doc.row_text(0).as_deref(), Some("x"));
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 0,
            text: "q".into(),
        }])
        .expect("插入失败");
        assert!(doc.redo().is_none());
    }

    /// 撤销步数上限：超出后丢弃最旧步骤。
    #[test]
    fn undo_step_cap_trims_oldest() {
        let (_dir, mut doc) = open_doc(b"", None);
        for _ in 0..(UNDO_MAX_STEPS + 5) {
            doc.apply_edits(&[EditOp::Insert {
                row: 0,
                utf16: 0,
                text: "a".into(),
            }])
            .expect("插入失败");
        }
        assert_eq!(doc.undo_stack.len(), UNDO_MAX_STEPS);
        let mut undone = 0usize;
        while doc.undo().is_some() {
            undone += 1;
        }
        assert_eq!(undone, UNDO_MAX_STEPS);
    }

    /// 越界解析错误：批次原子（失败不改变状态）。
    #[test]
    fn resolve_errors() {
        let (_dir, mut doc) = open_doc(b"ab\ncd", None);
        assert!(matches!(
            doc.apply_edits(&[EditOp::Insert {
                row: 5,
                utf16: 0,
                text: String::new(),
            }])
            .err(),
            Some(EditError::RowOutOfRange { row: 5 })
        ));
        assert!(matches!(
            doc.apply_edits(&[EditOp::Insert {
                row: 0,
                utf16: 9,
                text: "x".into(),
            }])
            .err(),
            Some(EditError::Utf16OutOfRange { .. })
        ));
        assert_eq!(doc.row_text(0).as_deref(), Some("ab"));
        assert!(!doc.is_dirty());
    }
}
