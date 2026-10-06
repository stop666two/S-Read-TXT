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
//!   索引天然一致；行内偏移的字节换算在编辑应用路径中实现。
//!
//! 内存模型：原文片段引用 mmap（零复制）；新增片段引用只增缓冲；
//! 行数元数据 = 每片段一个 `u64`（不是每行一个）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::textfile::editing::fenwick::Fenwick;
use crate::textfile::editing::piece::{
    count_units, ends_with_cr, newline_width, starts_with_lf, Piece, PieceMeta, PieceSource,
};
use crate::textfile::editing::{DISPLAY_SEGMENT_BYTES, UNDO_MAX_BYTES, UNDO_MAX_STEPS};
use crate::textfile::encoding::{detect, FileEncoding};
use crate::textfile::line_index::{find_newline, snap_row_boundary};
use crate::textfile::mmap::MappedFile;
use crate::textfile::session::TextFileError;
use crate::textfile::window::RowText;

/// 换行符转换的全文档预算（32MB；超出拒绝，避免整文档重建超限）。
pub const EOL_CONVERT_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// 编辑引擎错误。
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    /// 打开/读取路径的文件错误（不存在 / 超限 / IO），与只读路径共用语义
    #[error(transparent)]
    File(#[from] TextFileError),
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
    /// 替换全部时命中数量超过上限（避免构造超大操作批次与撤销帧）
    #[error("匹配过多：超过 {limit} 处，请使用更具体的查找内容")]
    TooManyMatches {
        /// 允许的最大命中数
        limit: usize,
    },
    /// 正则表达式无效（编译失败；`message` 为引擎给出的详细说明）
    #[error("正则表达式无效：{message}")]
    InvalidRegex {
        /// 引擎错误说明
        message: String,
    },
    /// 预览后文档发生变化（状态号不一致），需重新查找（防御性）
    #[error("文档已变化，请重新执行查找/替换")]
    StaleSearch,
    /// 正则扫描超时（已中断，内容未修改；超时可配 `app.regex.timeoutMs`）
    #[error("正则执行超时，已中断，未修改内容")]
    RegexTimeout,
    /// 换行符转换超限（全文档重建受内存预算保护）
    #[error("文件过大，无法执行换行符转换（{size_bytes} 字节，上限 {limit_mb} MB）")]
    EolConvertTooLarge {
        /// 实际大小（字节）
        size_bytes: u64,
        /// 允许上限（MB）
        limit_mb: u32,
    },
}

/// 换行符转换结果（`convert_eol`）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EolConvertOutcome {
    /// 归一化处理的换行处数（0 = 无需修改）
    pub replacements: u64,
    /// 应用结果（无操作时为 `None`）
    pub applied: Option<EditApplied>,
}

/// 编辑操作（位置坐标为「应用前」的文档状态）。
///
/// 批量约定（IPC 层与前端遵守）：
/// - 同一批次内不出现重叠或同位置的多个操作（前端合并相邻按键）；
/// - 应用顺序由引擎统一按位置降序处理，保证前序操作不移动后序位置。
///
/// IPC 序列化：外部标签为 `kind`（`insert` / `delete` / `replace`），
/// 字段为 camelCase（如 `startRow`），与前端 TypeScript 类型一一对应。
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
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
    /// 首个受影响的显示行（应用前坐标；前端从该行起重取可视窗）
    pub touched_row: u64,
    /// 当前显示行总数（超长逻辑行按 8KB 分段）
    pub rows_total: u64,
    /// 当前总字节数（不含 BOM）
    pub byte_len: u64,
    /// 应用后的光标显示行（段号；后端换算，避免长行分段在前端的近似误差）
    pub caret_row: u64,
    /// 应用后的光标段内 UTF-16 偏移
    pub caret_utf16: u64,
}

/// 超长逻辑行的显示分段表（与只读模式的行内 8KB 分块语义一致）。
///
/// 仅当逻辑行字节数超过 [`DISPLAY_SEGMENT_BYTES`] 时构建；段边界经字符对齐
/// （UTF-8 续字节回退 / UTF-16 代理项保护 / 传统多字节前向走查）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct LongRowSegments {
    /// 各段起始（相对行首的字节偏移；首项恒为 0，严格递增）
    starts: Vec<u64>,
    /// 各段起始对应的行内 UTF-16 偏移（与 `starts` 一一对应；首项恒为 0）
    utf16_bases: Vec<u64>,
}

impl LongRowSegments {
    /// 段数（≥1）。
    fn segments(&self) -> u64 {
        self.starts.len() as u64
    }

    /// 相对普通行的「额外段数」（段数 - 1）。
    fn extra(&self) -> u64 {
        self.segments().saturating_sub(1)
    }

    /// 第 `index` 段的 `(行内起始字节, 行内结束字节)`（末段到 `row_len`）。
    fn byte_span(&self, index: usize, row_len: u64) -> (u64, u64) {
        (
            self.starts[index],
            self.starts.get(index + 1).copied().unwrap_or(row_len),
        )
    }

    /// 表自身的撤销预算字节数（近似快照开销）。
    fn cost_bytes(&self) -> u64 {
        ((self.starts.len() + self.utf16_bases.len()) as u64) * 8
    }
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
    /// 该状态的超长行分段表（随状态快照交换，保证撤销后分段视图一致）
    long_rows: BTreeMap<u64, LongRowSegments>,
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
pub(crate) struct DocPos {
    /// 片段下标（可等于片段数，表示末尾）
    pub(crate) piece: usize,
    /// 片段内偏移（字节）
    pub(crate) off: u64,
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
    /// 超长逻辑行的显示分段表（键 = 逻辑行号；未超长的行不登记）
    long_rows: BTreeMap<u64, LongRowSegments>,
    /// 文档是否以换行单元结尾（行数换算用）
    trailing_newline: bool,
    /// 主导换行符风格（打开时检测；「换行符转换」成功后更新）
    eol: crate::textfile::eol::EolStyle,
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
    /// 行推进游标缓存（顺序取行避免每次从片段起点重扫；编辑后按状态版本失效）
    advance_cursor: std::cell::RefCell<Option<AdvanceCursor>>,
}

/// 行推进游标：记录「第 `row` 行起点位于 `(piece, off)`」。
///
/// 仅当 `state_id` 与当前文档一致时有效（编辑/撤销/重做都会使游标失效）。
#[derive(Clone, Copy)]
struct AdvanceCursor {
    state_id: u64,
    row: u64,
    piece: usize,
    off: u64,
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
        let eol = crate::textfile::eol::detect(mapped.bytes(), encoding);

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
            long_rows: BTreeMap::new(),
            trailing_newline: false,
            eol,
            state_id: 1,
            next_state_id: 2,
            saved_state_id: Some(1),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            undo_cost: 0,
            advance_cursor: std::cell::RefCell::new(None),
        };
        doc.rebuild_trees();
        doc.trailing_newline = doc.doc_ends_with_newline();
        doc.rebuild_long_rows_initial();
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

    /// 主导换行符风格（打开时检测；换行符转换成功后更新）。
    pub fn eol(&self) -> crate::textfile::eol::EolStyle {
        self.eol
    }

    /// 将全文档换行符统一为 `target`（单撤销步；≤ [`EOL_CONVERT_MAX_BYTES`]）。
    ///
    /// 实现路径：收集全文（片段解码）→ 归一化换行 → 一次全文档 `Replace`，
    /// 完全复用编辑机制（撤销/树/长行段表/状态号自动维护）。
    /// 返回归一化处理的换行处数（0 = 文本已是目标风格，无操作）。
    pub fn convert_eol(
        &mut self,
        target: crate::textfile::eol::EolTarget,
    ) -> Result<EolConvertOutcome, EditError> {
        self.convert_eol_with(target, EOL_CONVERT_MAX_BYTES)
    }

    /// 上限可注入版本（测试用；`max_bytes` 为全文档预算）。
    pub(crate) fn convert_eol_with(
        &mut self,
        target: crate::textfile::eol::EolTarget,
        max_bytes: u64,
    ) -> Result<EolConvertOutcome, EditError> {
        if self.byte_len() > max_bytes {
            return Err(EditError::EolConvertTooLarge {
                size_bytes: self.byte_len(),
                limit_mb: (max_bytes / (1024 * 1024)) as u32,
            });
        }
        let mut text = String::new();
        for piece in &self.pieces {
            // decode_slice 的 from/to 是片段内偏移；整片段=0..len（曾误传绝对偏移导致越界崩溃）。
            text.push_str(&self.decode_slice(piece, 0, piece.len));
        }
        // 先归一到 \n，再展开为目标风格；与原文相同则短路（含已是目标风格）。
        let unified = text.replace("\r\n", "\n").replace('\r', "\n");
        let converted = match target {
            crate::textfile::eol::EolTarget::Lf => unified.clone(),
            crate::textfile::eol::EolTarget::CrLf => unified.replace('\n', "\r\n"),
            crate::textfile::eol::EolTarget::Cr => unified.replace('\n', "\r"),
        };
        if converted == text {
            return Ok(EolConvertOutcome {
                replacements: 0,
                applied: None,
            });
        }
        let replacements = unified.matches('\n').count() as u64;
        let applied = self.replace_all_content_utf8(converted, 0);
        Ok(EolConvertOutcome {
            replacements,
            applied: Some(applied),
        })
    }

    /// 以 UTF-8 文本整体替换文档内容（单撤销步）。
    ///
    /// 共用路径（换行转换 / 快照恢复）：不走 EditOp 坐标层——全文档
    /// 「含末尾换行」的替换在行坐标模型中不可表达（末尾换行单元不属于
    /// 任何行范围；line_ops 在同类边界采用回退策略）。直接重建为单一新增
    /// 片段，并手工登记一个撤销步骤（预算与裁剪同 `apply_edits` 路径）。
    pub(crate) fn replace_all_content_utf8(
        &mut self,
        text: String,
        touched_row: u64,
    ) -> EditApplied {
        let old_len = self.byte_len();
        let old_pieces = self.pieces.len() as u64;
        let snapshot = self.take_snapshot(touched_row);
        let added_start = self.added.len() as u64;
        self.added.extend_from_slice(text.as_bytes());
        self.pieces = vec![Piece {
            source: PieceSource::Added,
            off: added_start,
            len: text.len() as u64,
        }];
        self.metas = vec![count_units(
            &self.added,
            FileEncoding::Utf8,
            added_start,
            added_start + text.len() as u64,
            false,
        )];
        self.redo_stack.clear();
        self.rebuild_trees();
        self.trailing_newline = self.doc_ends_with_newline();
        self.rebuild_long_rows_initial();
        self.state_id = self.next_state_id;
        self.next_state_id += 1;
        self.recompute_eol();
        let mut step = snapshot;
        step.cost_bytes = old_len
            .saturating_add(text.len() as u64)
            .saturating_add(16 * (old_pieces + 1));
        self.undo_cost += step.cost_bytes;
        self.undo_stack.push(step);
        self.trim_undo();
        self.applied(touched_row, (0, 0))
    }

    /// 快照恢复：以快照字节整体替换内容（单撤销步）。
    ///
    /// 快照由 `save::document_bytes` 以「当时的文档编码」生成；脏文档不允许
    /// 切换编码（`set_encoding` 在脏态被拒），因此按当前编码解码是安全的。
    pub fn restore_from_snapshot_bytes(&mut self, bytes: &[u8]) -> Result<EditApplied, EditError> {
        let (text, _, _) = self.encoding().encoding().decode(bytes);
        Ok(self.replace_all_content_utf8(text.into_owned(), 0))
    }

    /// 依当前片段构成重算 `eol`（撤销/重做后调用）：
    /// 仍含原文片段 → 以原文检测为准；全部为新增缓冲 → 以缓冲内容检测。
    /// （混合历史的精确风格以「原文/全新增」两端近似，展示语义足够。）
    fn recompute_eol(&mut self) {
        let has_original = self
            .pieces
            .iter()
            .any(|piece| matches!(piece.source, PieceSource::Original));
        self.eol = if has_original {
            crate::textfile::eol::detect(self.original.bytes(), self.encoding)
        } else {
            crate::textfile::eol::detect(&self.added, FileEncoding::Utf8)
        };
    }

    /// 文档是否以换行结尾（行操作「确保换行结尾」依据）。
    pub(crate) fn has_trailing_newline(&self) -> bool {
        self.trailing_newline
    }

    /// 首选换行串（探测原文首个换行；UTF-16 按双字节单元识别）。
    ///
    /// 说明：编辑操作生成的新文本统一使用该换行，避免把 CRLF 文件改写成 LF；
    /// 无法探测时回退 `"\n"`。
    pub(crate) fn preferred_newline(&self) -> &'static str {
        const PROBE: usize = 4096;
        let Some(piece) = self.pieces().iter().find(|piece| !piece.is_empty()) else {
            return "\n";
        };
        let bytes = self.piece_bytes(piece);
        match self.encoding() {
            FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
                let little = matches!(self.encoding(), FileEncoding::Utf16Le);
                let limit = bytes.len().min(PROBE) & !1;
                let mut index = 0usize;
                while index + 1 < limit {
                    let unit = if little {
                        u16::from_le_bytes([bytes[index], bytes[index + 1]])
                    } else {
                        u16::from_be_bytes([bytes[index], bytes[index + 1]])
                    };
                    match unit {
                        0x000A => return "\n",
                        0x000D => {
                            let next = if index + 3 < bytes.len() {
                                Some(if little {
                                    u16::from_le_bytes([bytes[index + 2], bytes[index + 3]])
                                } else {
                                    u16::from_be_bytes([bytes[index + 2], bytes[index + 3]])
                                })
                            } else {
                                None
                            };
                            return if next == Some(0x000A) { "\r\n" } else { "\r" };
                        }
                        _ => {}
                    }
                    index += 2;
                }
                "\n"
            }
            _ => {
                let limit = bytes.len().min(PROBE);
                let mut index = 0usize;
                while index < limit {
                    match bytes[index] {
                        b'\n' => return "\n",
                        b'\r' => {
                            return if bytes.get(index + 1) == Some(&b'\n') {
                                "\r\n"
                            } else {
                                "\r"
                            };
                        }
                        _ => {}
                    }
                    index += 1;
                }
                "\n"
            }
        }
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

    /// 行首对应的阅读百分比（与只读路径语义一致：行首字节偏移 / 总字节数 × 100）。
    ///
    /// 参数：`row` 行号（越界时按末行处理）。
    /// 返回：0.0–100.0 的百分比；空文档返回 0.0。
    pub fn percent_at_row(&self, row: u64) -> f64 {
        let total = self.byte_len();
        if total == 0 {
            return 0.0;
        }
        let last_row = self.rows_total().saturating_sub(1);
        let offset = self
            .row_start_pos(row.min(last_row))
            .map(|pos| self.global_offset(pos))
            .unwrap_or(0);
        ((offset as f64 / total as f64) * 100.0).min(100.0)
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
                logical_row: None,
                base_utf16: None,
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

    // ---- 显示分段（超长逻辑行 8KB 虚拟分段；与只读行模型一致） ----

    /// 显示行总数（= 逻辑行 + 超长行额外分段数）。
    pub fn display_rows_total(&self) -> u64 {
        self.rows_total() + self.extra_segments()
    }

    /// 逻辑行贡献的额外分段总数。
    fn extra_segments(&self) -> u64 {
        self.long_rows.values().map(LongRowSegments::extra).sum()
    }

    /// `row` 之前（不含）的长行额外分段累计数。
    fn extras_before_row(&self, row: u64) -> u64 {
        self.long_rows
            .range(..row)
            .map(|(_, table)| table.extra())
            .sum()
    }

    /// 逻辑位置 → 显示位置：`(段序号, 段内 UTF-16 偏移)`。
    ///
    /// 普通行（未超长）映射为恒等；长行按分段表二分定位。
    pub fn seg_of_row_utf16(&self, row: u64, utf16: u64) -> (u64, u64) {
        let base_seg = row + self.extras_before_row(row);
        match self.long_rows.get(&row) {
            None => (base_seg, utf16),
            Some(table) => {
                let index = match table.utf16_bases.binary_search(&utf16) {
                    Ok(found) => found,
                    Err(insert) => insert.saturating_sub(1),
                };
                (base_seg + index as u64, utf16 - table.utf16_bases[index])
            }
        }
    }

    /// 显示位置 → 逻辑位置：`(逻辑行, 段首 UTF-16 偏移, 段在行内下标)`。
    ///
    /// 越界段返回 `None`。
    pub fn seg_to_row_utf16(&self, seg: u64) -> Option<(u64, u64, u64)> {
        if seg >= self.display_rows_total() {
            return None;
        }
        let mut extra_acc = 0u64;
        for (&row, table) in &self.long_rows {
            let first_seg = row + extra_acc;
            if seg < first_seg {
                return Some((seg - extra_acc, 0, 0));
            }
            let end_seg = first_seg + table.segments();
            if seg < end_seg {
                let index = (seg - first_seg) as usize;
                return Some((row, table.utf16_bases[index], index as u64));
            }
            extra_acc += table.extra();
        }
        Some((seg - extra_acc, 0, 0))
    }

    /// 取显示行的文本窗口（超长行按 8KB 分段；普通行与逻辑行等值）。
    ///
    /// 编辑视图的每个显示行携带 `logicalRow` / `baseUtf16`，供前端光标/选区
    /// 在逻辑行坐标上跨段映射；只读视图省略这两个字段。
    pub fn fetch_display_rows(&self, start_row: u64, count: usize) -> Vec<RowText> {
        let total = self.display_rows_total();
        let mut rows = Vec::new();
        let mut seg = start_row;
        while rows.len() < count && seg < total {
            let Some((row, base_utf16, index)) = self.seg_to_row_utf16(seg) else {
                break;
            };
            let Ok(row_start) = self.row_start_pos(row) else {
                break;
            };
            let row_start_global = self.global_offset(row_start);
            let text = match self.long_rows.get(&row) {
                None => {
                    let Ok(row_end) = self.row_end_pos(row) else {
                        break;
                    };
                    self.text_between(row_start, row_end)
                }
                Some(table) => {
                    let Ok(row_end) = self.row_end_pos(row) else {
                        break;
                    };
                    let row_len = self.global_offset(row_end) - row_start_global;
                    let (from, to) = table.byte_span(index as usize, row_len);
                    let start_pos = self.pos_from_global(row_start_global + from);
                    let end_pos = self.pos_from_global(row_start_global + to);
                    self.text_between(start_pos, end_pos)
                }
            };
            rows.push(RowText {
                row: seg,
                text,
                logical_row: Some(row),
                base_utf16: Some(base_utf16),
            });
            seg += 1;
        }
        rows
    }

    /// 显示行首对应的阅读百分比（0.0–100.0；段首字节语义）。
    pub fn percent_at_seg(&self, seg: u64) -> f64 {
        let total = self.byte_len();
        if total == 0 {
            return 0.0;
        }
        let last = self.display_rows_total().saturating_sub(1);
        let Some((row, _, index)) = self.seg_to_row_utf16(seg.min(last)) else {
            return 0.0;
        };
        let Ok(row_start) = self.row_start_pos(row) else {
            return 0.0;
        };
        let mut offset = self.global_offset(row_start);
        if let Some(table) = self.long_rows.get(&row) {
            offset += table.starts[index as usize];
        }
        ((offset as f64 / total as f64) * 100.0).min(100.0)
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
    pub(crate) fn piece_bytes(&self, piece: &Piece) -> &[u8] {
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
    pub(crate) fn decode_slice(&self, piece: &Piece, from: u64, to: u64) -> String {
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
    pub(crate) fn encoding_for(&self, piece: &Piece) -> FileEncoding {
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
    ///
    /// 顺序访问（读取可视窗口）经行推进游标做增量扫描；游标失效时回退全量定位。
    fn row_start_pos(&self, row: u64) -> Result<DocPos, EditError> {
        if row >= self.rows_total() {
            return Err(EditError::RowOutOfRange { row });
        }
        if row == 0 {
            let pos = DocPos { piece: 0, off: 0 };
            self.cache_cursor(0, pos);
            return Ok(pos);
        }
        if let Some(pos) = self.advance_from_cursor(row) {
            return Ok(pos);
        }
        let (_, end) = self.unit_span(row - 1)?;
        self.cache_cursor(row, end);
        Ok(end)
    }

    /// 更新行推进游标。
    fn cache_cursor(&self, row: u64, pos: DocPos) {
        *self.advance_cursor.borrow_mut() = Some(AdvanceCursor {
            state_id: self.state_id,
            row,
            piece: pos.piece,
            off: pos.off,
        });
    }

    /// 从游标增量推进到 `row` 行起点；`None` = 游标不可用（需回退全量扫描）。
    ///
    /// 语义与逐单元 `unit_span` 完全一致：换行单元可跨片段（片尾 `\r` + 片首 `\n`），
    /// 片段起始的孤立 `\n`（上一片段以 `\r` 结尾）不计为独立单元。
    fn advance_from_cursor(&self, row: u64) -> Option<DocPos> {
        let (cursor_row, mut piece, mut off) = {
            let cursor = (*self.advance_cursor.borrow())?;
            if cursor.state_id != self.state_id {
                return None;
            }
            if cursor.row == row {
                return Some(DocPos {
                    piece: cursor.piece,
                    off: cursor.off,
                });
            }
            if cursor.row > row {
                return None;
            }
            (cursor.row, cursor.piece, cursor.off)
        };
        let mut current = cursor_row;
        while current < row {
            // 从 (piece, off) 起找到当前行的换行单元
            let mut found: Option<(usize, u64, u64)> = None;
            let mut scan_piece = piece;
            let mut scan_off = off;
            while scan_piece < self.pieces.len() {
                let scan_ref = self.pieces[scan_piece];
                let bytes = self.piece_bytes(&scan_ref);
                let encoding = self.encoding_for(&scan_ref);
                let len = bytes.len() as u64;
                if scan_off == 0
                    && self.prev_cr(scan_piece)
                    && starts_with_lf(bytes, encoding, 0, len)
                {
                    // 跨片 CRLF：片段起始的 `\n` 属于上一片段结尾的 `\r` 单元
                    scan_off = newline_width(encoding);
                }
                if let Some((newline_start, newline_end)) = find_newline(bytes, encoding, scan_off)
                {
                    found = Some((scan_piece, newline_start, newline_end.min(len)));
                    break;
                }
                scan_piece += 1;
                scan_off = 0;
            }
            let (unit_piece, unit_start, unit_end) = found?;
            let unit_ref = self.pieces[unit_piece];
            let unit_bytes = self.piece_bytes(&unit_ref);
            let unit_encoding = self.encoding_for(&unit_ref);
            let next = if ends_with_cr(unit_bytes, unit_encoding, unit_start, unit_end)
                && unit_end >= unit_bytes.len() as u64
            {
                match self.pieces.get(unit_piece + 1) {
                    Some(next_ref) => {
                        let next_bytes = self.piece_bytes(next_ref);
                        let next_encoding = self.encoding_for(next_ref);
                        if starts_with_lf(next_bytes, next_encoding, 0, next_ref.len) {
                            DocPos {
                                piece: unit_piece + 1,
                                off: newline_width(next_encoding),
                            }
                        } else {
                            DocPos {
                                piece: unit_piece,
                                off: unit_end,
                            }
                        }
                    }
                    None => DocPos {
                        piece: unit_piece,
                        off: unit_end,
                    },
                }
            } else {
                DocPos {
                    piece: unit_piece,
                    off: unit_end,
                }
            };
            let pos = self.normalize_pos(next);
            current += 1;
            if current == row {
                self.cache_cursor(row, pos);
                return Some(pos);
            }
            piece = pos.piece;
            off = pos.off;
        }
        None
    }

    /// 第 `row` 行的结束位置（不含换行单元）。
    ///
    /// 语义：行 `row` 的终点 = 下一个换行单元的起点；
    /// 若其后没有换行单元（末行且无结尾换行）则为文档末尾。
    /// 游标正指向该行起点时做短扫描；否则回退 [`Self::unit_span`] 全量定位。
    fn row_end_pos(&self, row: u64) -> Result<DocPos, EditError> {
        let units = self.line_tree.total();
        if row < units {
            if let Some(pos) = self.scan_row_end_from_cursor(row) {
                return Ok(pos);
            }
            let (start, _) = self.unit_span(row)?;
            Ok(start)
        } else {
            Ok(self.doc_end_pos())
        }
    }

    /// 游标正指向 `row` 行起点时，短扫描定位该行文本终点（换行单元起点）。
    fn scan_row_end_from_cursor(&self, row: u64) -> Option<DocPos> {
        {
            let cursor = (*self.advance_cursor.borrow())?;
            if cursor.state_id != self.state_id || cursor.row != row {
                return None;
            }
        }
        let start = {
            let cursor = (*self.advance_cursor.borrow())?;
            DocPos {
                piece: cursor.piece,
                off: cursor.off,
            }
        };
        let mut scan_piece = start.piece;
        let mut scan_off = start.off;
        while scan_piece < self.pieces.len() {
            let piece = self.pieces[scan_piece];
            let bytes = self.piece_bytes(&piece);
            let encoding = self.encoding_for(&piece);
            if let Some((newline_start, _)) = find_newline(bytes, encoding, scan_off) {
                return Some(DocPos {
                    piece: scan_piece,
                    off: newline_start,
                });
            }
            scan_piece += 1;
            scan_off = 0;
        }
        Some(self.doc_end_pos())
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

    /// 全局字节偏移 → 文档位置（线性走片段；片段数经合并后为量级 O(1)–O(10²)）。
    fn pos_from_global(&self, global: u64) -> DocPos {
        let mut remaining = global;
        for (index, piece) in self.pieces.iter().enumerate() {
            if remaining <= piece.len {
                return DocPos {
                    piece: index,
                    off: remaining,
                };
            }
            remaining -= piece.len;
        }
        self.doc_end_pos()
    }

    /// 初次构建超长行分段表（打开时文档仅一个原始片段，直接顺序扫描避免逐行前缀定位）。
    fn rebuild_long_rows_initial(&mut self) {
        self.long_rows.clear();
        if self.byte_len() == 0 {
            return;
        }
        let mut long_rows: Vec<u64> = Vec::new();
        {
            let bytes = self.original.bytes();
            let file_end = bytes.len() as u64;
            let mut row: u64 = 0;
            let mut pos = self.bom_len;
            while pos < file_end {
                let (row_end, next) = match find_newline(bytes, self.encoding, pos) {
                    Some((newline_start, newline_end)) => {
                        (newline_start, newline_end.min(file_end))
                    }
                    None => (file_end, file_end),
                };
                if row_end - pos > DISPLAY_SEGMENT_BYTES {
                    long_rows.push(row);
                }
                row += 1;
                pos = next;
            }
        }
        for row in long_rows {
            if let Some(table) = self.build_long_row_table(row) {
                self.long_rows.insert(row, table);
            }
        }
    }

    /// 为逻辑行构建分段表（≤ [`DISPLAY_SEGMENT_BYTES`] 时返回 `None`；跨片段按片段编码对齐）。
    fn build_long_row_table(&self, row: u64) -> Option<LongRowSegments> {
        let start = self.row_start_pos(row).ok()?;
        let end = self.row_end_pos(row).ok()?;
        let row_start_global = self.global_offset(start);
        let row_end_global = self.global_offset(end);
        let row_len = row_end_global.checked_sub(row_start_global)?;
        if row_len <= DISPLAY_SEGMENT_BYTES {
            return None;
        }
        let mut starts = vec![0u64];
        let mut utf16_bases = vec![0u64];
        let mut utf16_base = 0u64;
        let mut cursor = start;
        let mut cursor_byte = row_start_global;
        while cursor_byte + DISPLAY_SEGMENT_BYTES < row_end_global {
            let target = cursor_byte + DISPLAY_SEGMENT_BYTES;
            let (boundary, boundary_byte) = self.snap_global_in_row(target, cursor, cursor_byte);
            if boundary_byte <= cursor_byte || boundary_byte > row_end_global {
                break;
            }
            let text = self.text_between(cursor, boundary);
            utf16_base += text.encode_utf16().count() as u64;
            starts.push(boundary_byte - row_start_global);
            utf16_bases.push(utf16_base);
            cursor = boundary;
            cursor_byte = boundary_byte;
        }
        Some(LongRowSegments {
            starts,
            utf16_bases,
        })
    }

    /// 在行内把全局字节偏移对齐到字符边界；返回 `(对齐后的位置, 全局偏移)`。
    ///
    /// 语义与只读模式的行内 8KB 分块一致（复用 [`snap_row_boundary`]，以当前段
    /// 起点为对齐基准）；目标正好落在片段末尾时对齐到片段边界。
    fn snap_global_in_row(&self, target: u64, cursor: DocPos, cursor_byte: u64) -> (DocPos, u64) {
        let mut piece_index = cursor.piece;
        let mut piece_local_start = cursor.off;
        let mut base = cursor_byte;
        while piece_index < self.pieces.len() {
            let piece = self.pieces[piece_index];
            let remaining_in_piece = piece.len - piece_local_start;
            if target <= base + remaining_in_piece {
                let local_target = piece_local_start + (target - base);
                if local_target >= piece.len {
                    return (
                        self.normalize_pos(DocPos {
                            piece: piece_index,
                            off: piece.len,
                        }),
                        base + remaining_in_piece,
                    );
                }
                let bytes = self.piece_bytes(&piece);
                let encoding = self.encoding_for(&piece);
                let snapped = snap_row_boundary(bytes, encoding, piece_local_start, local_target);
                // 防御：极端损坏编码下对齐函数可能不推进，此时采用原始候选点
                let snapped = if snapped <= piece_local_start {
                    local_target
                } else {
                    snapped
                };
                return (
                    DocPos {
                        piece: piece_index,
                        off: snapped,
                    },
                    base + (snapped - piece_local_start),
                );
            }
            base += remaining_in_piece;
            piece_index += 1;
            piece_local_start = 0;
        }
        (self.doc_end_pos(), base)
    }

    /// 维护超长行分段表（编辑后调用；增量更新受影响行 + 平移后续行号）。
    ///
    /// 参数（应用前后坐标系）：
    /// - `touched`：首个受影响逻辑行（编辑起点行号在应用前后不变）；
    /// - `old_end`：受影响区间在应用前的末行（插入 = 起点行；删除/替换 = 区间末行）；
    /// - `inserted_newlines`：本批插入文本中的换行总数；
    /// - `deleted_rows`：本批删除区间跨过的行数（Σ(end_row - start_row)）。
    fn update_long_rows(
        &mut self,
        touched: u64,
        old_end: u64,
        inserted_newlines: u64,
        deleted_rows: u64,
    ) {
        if !self.long_rows.is_empty() {
            // 旧受影响区间内的表全部失效（内容变化或行合并）
            let stale: Vec<u64> = self
                .long_rows
                .range(touched..=old_end)
                .map(|(row, _)| *row)
                .collect();
            for row in stale {
                self.long_rows.remove(&row);
            }
            // 区间之后的行号整体平移
            let delta = inserted_newlines as i64 - deleted_rows as i64;
            if delta != 0 {
                let shifted: Vec<(u64, LongRowSegments)> = self
                    .long_rows
                    .range(old_end + 1..)
                    .map(|(row, table)| (*row, table.clone()))
                    .collect();
                for (row, _) in &shifted {
                    self.long_rows.remove(row);
                }
                for (row, table) in shifted {
                    let new_row = (row as i64 + delta).max(0) as u64;
                    self.long_rows.insert(new_row, table);
                }
            }
        }
        // 复查新受影响区间（编辑范围 + 插入换行产生的新行）
        let new_end = touched + inserted_newlines;
        for row in touched..=new_end {
            if row >= self.rows_total() {
                break;
            }
            match self.build_long_row_table(row) {
                Some(table) => {
                    self.long_rows.insert(row, table);
                }
                None => {
                    self.long_rows.remove(&row);
                }
            }
        }
    }

    // ---- 编辑应用与撤销/重做 ----

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
        Ok(self
            .apply_edits_cancellable(ops, &|| false, &mut |_, _| {})?
            .expect("不取消时必然返回编辑结果"))
    }

    /// 可取消并上报进度的批量编辑（批量序号异步执行使用）。
    ///
    /// 返回 `Ok(None)` 表示已取消：文档状态完整回滚、不产生撤销步骤、不推进版本号。
    pub fn apply_edits_cancellable(
        &mut self,
        ops: &[EditOp],
        cancel: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Option<EditApplied>, EditError> {
        if ops.is_empty() {
            return Ok(Some(self.applied(0, (0, 0))));
        }
        // 1) 解析所有操作（相对编辑前状态）：批量单次扫描行位置（等价逐操作解析，
        //    但避免每个操作都从片段起点重扫——大文件批量编辑的关键路径）
        let Some(positions) = self.resolve_batch_positions_cancellable(ops, cancel)? else {
            return Ok(None);
        };
        // 超长行分段维护所需的区间统计（应用前坐标系）
        let mut resolved: Vec<(u64, u64, Vec<u8>, u64)> = Vec::with_capacity(ops.len());
        let mut old_end = 0u64;
        let mut deleted_rows = 0u64;
        let mut inserted_newlines = 0u64;
        for (index, op) in ops.iter().enumerate() {
            let (start, end) = positions[index];
            let (text, touched_row, span_rows, span_newlines) = match op {
                EditOp::Insert { row, text, .. } => (
                    text.clone().into_bytes(),
                    *row,
                    0,
                    count_newlines(text.as_bytes()),
                ),
                EditOp::Delete {
                    start_row, end_row, ..
                } => {
                    if end < start {
                        return Err(EditError::InvalidPosition);
                    }
                    (
                        Vec::new(),
                        *start_row,
                        end_row.saturating_sub(*start_row),
                        0,
                    )
                }
                EditOp::Replace {
                    start_row,
                    end_row,
                    text,
                    ..
                } => {
                    if end < start {
                        return Err(EditError::InvalidPosition);
                    }
                    (
                        text.clone().into_bytes(),
                        *start_row,
                        end_row.saturating_sub(*start_row),
                        count_newlines(text.as_bytes()),
                    )
                }
            };
            old_end = old_end.max(touched_row + span_rows);
            deleted_rows += span_rows;
            inserted_newlines += span_newlines;
            resolved.push((start, end, text, touched_row));
        }
        let touched = resolved.iter().map(|item| item.3).min().unwrap_or(0);
        // 2) 快照当前状态（撤销步骤）
        let mut before = self.take_snapshot(touched);
        // 3) 按位置降序应用（后面的编辑不影响前面位置）；逐步检查取消
        let mut cost = 0u64;
        resolved.sort_by(|left, right| right.0.cmp(&left.0));
        let total_ops = resolved.len() as u64;
        for (index, (start, end, text, _)) in resolved.iter().enumerate() {
            if cancel() {
                self.restore_snapshot(before);
                return Ok(None);
            }
            cost += text.len() as u64 + (end - start);
            self.apply_range(*start, *end, text);
            progress(index as u64 + 1, total_ops);
        }
        // 4) 合并、重建、版本推进、裁剪撤销预算
        self.coalesce();
        self.rebuild_trees();
        self.trailing_newline = self.doc_ends_with_newline();
        self.update_long_rows(touched, old_end, inserted_newlines, deleted_rows);
        self.state_id = self.next_state_id;
        self.next_state_id += 1;
        self.redo_stack.clear();
        let snapshot_cost = (before.pieces.len() as u64)
            * ((std::mem::size_of::<Piece>() + std::mem::size_of::<PieceMeta>()) as u64)
            + before
                .long_rows
                .values()
                .map(LongRowSegments::cost_bytes)
                .sum::<u64>();
        before.cost_bytes = cost + snapshot_cost;
        self.undo_cost += before.cost_bytes;
        self.undo_stack.push(before);
        self.trim_undo();
        // 光标落点：以批次最后一个操作为准（本应用单操作批次 = 精确；
        // 多操作批次按最后操作坐标，调用方需自行权衡）。
        let caret = match ops.last() {
            None => (touched, 0),
            Some(EditOp::Insert { row, utf16, text }) => advance_caret(*row, *utf16, text),
            Some(EditOp::Delete {
                start_row,
                start_utf16,
                ..
            }) => (*start_row, *start_utf16),
            Some(EditOp::Replace {
                start_row,
                start_utf16,
                text,
                ..
            }) => advance_caret(*start_row, *start_utf16, text),
        };
        progress(total_ops, total_ops);
        Ok(Some(self.applied(touched, caret)))
    }

    /// 取消时把文档恢复到操作前快照（不产生撤销步骤、不推进版本号）。
    fn restore_snapshot(&mut self, step: UndoStep) {
        self.pieces = step.pieces;
        self.metas = step.metas;
        self.long_rows = step.long_rows;
        self.trailing_newline = step.trailing_newline;
        self.state_id = step.state_id;
        self.rebuild_trees();
    }

    /// 撤销一步；无可撤销时返回 `None`。
    pub fn undo(&mut self) -> Option<EditApplied> {
        let step = self.undo_stack.pop()?;
        let touched = step.touched_row;
        self.undo_cost = self.undo_cost.saturating_sub(step.cost_bytes);
        let for_redo = self.swap_state(step);
        self.redo_stack.push(for_redo);
        self.rebuild_trees();
        self.recompute_eol();
        Some(self.applied(touched, (touched, 0)))
    }

    /// 重做一步；无可重做时返回 `None`。
    pub fn redo(&mut self) -> Option<EditApplied> {
        let step = self.redo_stack.pop()?;
        let touched = step.touched_row;
        let for_undo = self.swap_state(step);
        self.undo_cost += for_undo.cost_bytes;
        self.undo_stack.push(for_undo);
        self.rebuild_trees();
        self.recompute_eol();
        Some(self.applied(touched, (touched, 0)))
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

    /// 构造对外的编辑结果（行号已换算为显示行）。
    ///
    /// `caret` 为应用后的逻辑坐标 `(逻辑行, 行内 UTF-16)`，换算为显示段坐标；
    /// 行号越界时收敛到末行（保守防御，正常流程不会触发）。
    fn applied(&self, touched_row: u64, caret: (u64, u64)) -> EditApplied {
        let last_row = self.rows_total().saturating_sub(1);
        let touched_seg = self.seg_of_row_utf16(touched_row.min(last_row), 0).0;
        let (caret_seg, caret_utf16) = self.seg_of_row_utf16(caret.0.min(last_row), caret.1);
        EditApplied {
            state_id: self.state_id,
            dirty: self.is_dirty(),
            touched_row: touched_seg,
            rows_total: self.display_rows_total(),
            byte_len: self.byte_len(),
            caret_row: caret_seg,
            caret_utf16,
        }
    }

    /// 截取当前状态的快照（撤销步骤载体）。
    fn take_snapshot(&self, touched_row: u64) -> UndoStep {
        UndoStep {
            pieces: self.pieces.clone(),
            metas: self.metas.clone(),
            long_rows: self.long_rows.clone(),
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
            long_rows: std::mem::take(&mut self.long_rows),
            trailing_newline: self.trailing_newline,
            state_id: self.state_id,
            touched_row: step.touched_row,
            cost_bytes: step.cost_bytes,
        };
        self.pieces = step.pieces;
        self.metas = step.metas;
        self.long_rows = step.long_rows;
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
    pub(crate) fn global_offset(&self, pos: DocPos) -> u64 {
        self.byte_tree.prefix(pos.piece) + pos.off
    }

    /// 全局字节偏移的前一字符是否为 `\r`。
    ///
    /// 搜索游标跨块推进时使用：`\r` 与后续 `\n` 可能被解码块分割，
    /// 需要据此保持 CRLF「单一换行单元」的计数语义。
    pub(crate) fn char_before_is_cr(&self, global: u64) -> bool {
        if global == 0 || self.pieces.is_empty() {
            return false;
        }
        let last = global - 1;
        if last >= self.byte_tree.total() {
            return false;
        }
        let index = self.byte_tree.lower_bound(last);
        let Some(piece) = self.pieces.get(index) else {
            return false;
        };
        let local = (last - self.byte_tree.prefix(index)) as usize;
        let bytes = self.piece_bytes(piece);
        if local >= bytes.len() {
            return false;
        }
        match self.encoding_for(piece) {
            FileEncoding::Utf16Le => local >= 1 && bytes[local] == 0x00 && bytes[local - 1] == 0x0D,
            FileEncoding::Utf16Be => local >= 1 && bytes[local] == 0x0D && bytes[local - 1] == 0x00,
            _ => bytes[local] == 0x0D,
        }
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
    pub(crate) fn resolve_pos(&self, row: u64, utf16: u64) -> Result<DocPos, EditError> {
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

    /// 批量取指定行的文本（单次扫描定位行边界 + 逐行解码；用于批量序号等
    /// 大范围逐行检查——避免逐行 [`Self::row_start_pos`] 从片段起点重扫）。
    pub(crate) fn texts_for_rows(&self, rows: &[u64]) -> Vec<String> {
        if rows.is_empty() {
            return Vec::new();
        }
        let mut sorted = rows.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        let spans = self
            .scan_row_spans_cancellable(&sorted, &|| false)
            .expect("不取消时必然返回扫描结果");
        let mut lookup: Vec<(u64, String)> = Vec::with_capacity(sorted.len());
        for (index, &row) in sorted.iter().enumerate() {
            let (start, end) = spans[index];
            lookup.push((row, self.text_between(start, end)));
        }
        rows.iter()
            .map(|row| {
                lookup
                    .binary_search_by_key(row, |(key, _)| *key)
                    .ok()
                    .map(|index| lookup[index].1.clone())
                    .unwrap_or_default()
            })
            .collect()
    }

    /// 批量解析操作位置（可取消）：对全部请求行做单次顺序扫描（每次 apply 只扫一遍片段），
    /// 返回与 `ops` 顺序一致的全局字节区间 `(start, end)`；`Ok(None)` 表示已取消。
    ///
    /// 语义与逐操作 [`Self::resolve_pos`] 完全一致（含行越界与 UTF-16 越界报错、
    /// 代理对中间吸附、跨片段 CRLF 单元处理）。
    fn resolve_batch_positions_cancellable(
        &self,
        ops: &[EditOp],
        cancel: &dyn Fn() -> bool,
    ) -> Result<Option<Vec<(u64, u64)>>, EditError> {
        let mut requests: Vec<(u64, u64)> = Vec::with_capacity(ops.len() * 2);
        let mut slots: Vec<(usize, usize)> = Vec::with_capacity(ops.len());
        for op in ops {
            let (start_req, end_req) = match op {
                EditOp::Insert { row, utf16, .. } => ((*row, *utf16), (*row, *utf16)),
                EditOp::Delete {
                    start_row,
                    start_utf16,
                    end_row,
                    end_utf16,
                } => ((*start_row, *start_utf16), (*end_row, *end_utf16)),
                EditOp::Replace {
                    start_row,
                    start_utf16,
                    end_row,
                    end_utf16,
                    ..
                } => ((*start_row, *start_utf16), (*end_row, *end_utf16)),
            };
            slots.push((requests.len(), requests.len() + 1));
            requests.push(start_req);
            requests.push(end_req);
        }
        for (row, _) in &requests {
            if *row >= self.rows_total() {
                return Err(EditError::RowOutOfRange { row: *row });
            }
        }
        let mut rows: Vec<u64> = requests.iter().map(|(row, _)| *row).collect();
        rows.sort_unstable();
        rows.dedup();
        let Some(spans) = self.scan_row_spans_cancellable(&rows, cancel) else {
            return Ok(None);
        };

        let lookup = |row: u64| -> (DocPos, DocPos) {
            let index = rows.binary_search(&row).expect("请求行已在扫描集合中");
            spans[index]
        };
        let mut resolved_requests: Vec<u64> = Vec::with_capacity(requests.len());
        for (index, (row, utf16)) in requests.iter().enumerate() {
            if index % 512 == 0 && cancel() {
                return Ok(None);
            }
            let (start, end) = lookup(*row);
            let pos = if *utf16 == 0 {
                start
            } else {
                self.resolve_within(*row, *utf16, start, end)?
            };
            resolved_requests.push(self.global_offset(pos));
        }
        let mut result = Vec::with_capacity(ops.len());
        for (start_slot, end_slot) in slots {
            result.push((resolved_requests[start_slot], resolved_requests[end_slot]));
        }
        Ok(Some(result))
    }

    /// 扫描指定行的 `(起始, 结束)` 文档位置（按行号升序单次遍历片段；可取消）。
    ///
    /// `rows` 必须已升序去重；行 0 起始为文档起点，末行结束为文档末尾
    /// （无结尾换行时无对应单元）。
    fn scan_row_spans_cancellable(
        &self,
        rows: &[u64],
        cancel: &dyn Fn() -> bool,
    ) -> Option<Vec<(DocPos, DocPos)>> {
        let units_total = self.line_tree.total();
        // 需要扫描的单元 id：行 r 的起点 = 单元 r-1 的终点；行 r 的终点 = 单元 r 的起点
        let mut needed: Vec<u64> = Vec::with_capacity(rows.len() * 2);
        for &row in rows {
            if row > 0 {
                needed.push(row - 1);
            }
            if row < units_total {
                needed.push(row);
            }
        }
        needed.sort_unstable();
        needed.dedup();
        // 单次顺序扫描：逐片段枚举换行单元，只记录需要的单元 span
        let mut spans: Vec<(DocPos, DocPos)> = Vec::with_capacity(needed.len());
        let mut pointer = 0usize;
        if !needed.is_empty() {
            let mut unit_id: u64 = 0;
            'pieces: for (piece_index, piece) in self.pieces.iter().enumerate() {
                if cancel() {
                    return None;
                }
                if pointer >= needed.len() {
                    break;
                }
                let bytes = self.piece_bytes(piece);
                let encoding = self.encoding_for(piece);
                let len = bytes.len() as u64;
                let mut pos = 0u64;
                if self.prev_cr(piece_index) && starts_with_lf(bytes, encoding, 0, len) {
                    pos += newline_width(encoding);
                }
                while let Some((newline_start, newline_end)) = find_newline(bytes, encoding, pos) {
                    if pointer >= needed.len() {
                        break 'pieces;
                    }
                    let end_off = newline_end.min(len);
                    while pointer < needed.len() && needed[pointer] == unit_id {
                        // 跨片 `\r`+`\n`：单元在片段末尾以 `\r` 结束时，终点越过下一片开头的 `\n`
                        let mut end_pos = DocPos {
                            piece: piece_index,
                            off: end_off,
                        };
                        if end_off == len && ends_with_cr(bytes, encoding, 0, len) {
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
                        spans.push((
                            DocPos {
                                piece: piece_index,
                                off: newline_start,
                            },
                            self.normalize_pos(end_pos),
                        ));
                        pointer += 1;
                    }
                    unit_id += 1;
                    if end_off >= len {
                        break;
                    }
                    pos = end_off;
                }
            }
        }
        let span_of = |unit: u64| -> (DocPos, DocPos) {
            let index = needed.binary_search(&unit).expect("所需单元已在扫描集合中");
            spans[index]
        };
        let result: Vec<(DocPos, DocPos)> = rows
            .iter()
            .map(|&row| {
                let start = if row == 0 {
                    DocPos { piece: 0, off: 0 }
                } else {
                    span_of(row - 1).1
                };
                let end = if row < units_total {
                    span_of(row).0
                } else {
                    self.doc_end_pos()
                };
                (start, end)
            })
            .collect();
        Some(result)
    }

    /// 在已知行区间内解析 `(row, utf16)` → 文档位置（等价 [`Self::resolve_pos`] 的
    /// 行内推进，但复用批量扫描得到的行起点/终点，避免重复全片段重扫）。
    fn resolve_within(
        &self,
        row: u64,
        utf16: u64,
        start: DocPos,
        end: DocPos,
    ) -> Result<DocPos, EditError> {
        if utf16 == 0 {
            return Ok(start);
        }
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

/// 计算插入/替换后光标的逻辑坐标（行号 + 行内 UTF-16）。
///
/// 语义：坐标基于「应用前」的起始位置；无换行时同行为 `utf16 + 文本长度`，
/// 有换行时落到最后一个换行之后的新行，列 = 末行文本的 UTF-16 长度。
fn advance_caret(row: u64, utf16: u64, text: &str) -> (u64, u64) {
    let newlines = text.bytes().filter(|byte| *byte == b'\n').count() as u64;
    if newlines == 0 {
        return (row, utf16 + text.encode_utf16().count() as u64);
    }
    let tail = text.rsplit('\n').next().unwrap_or_default();
    (row + newlines, tail.encode_utf16().count() as u64)
}

/// 统计 UTF-8 字节中的换行单元数（插入文本的行数贡献；用于显示分段维护）。
fn count_newlines(bytes: &[u8]) -> u64 {
    let len = bytes.len() as u64;
    let mut count = 0u64;
    let mut pos = 0u64;
    while pos < len {
        match find_newline(bytes, FileEncoding::Utf8, pos) {
            Some((_, end)) => {
                count += 1;
                pos = end.min(len);
            }
            None => break,
        }
    }
    count
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

    /// 超长行可进入编辑：显示分段与只读模式逐段一致，拼接无损。
    #[test]
    fn long_row_display_segments_match_read_mode() {
        let mut text = String::new();
        while text.len() < 30_000 {
            text.push_str("中文abc");
        }
        let bytes = text.as_bytes();
        let (_dir, doc) = open_doc(bytes, None);
        let index = RowIndex::build(bytes, FileEncoding::Utf8);
        assert_eq!(doc.rows_total(), 1);
        assert_eq!(doc.display_rows_total(), index.rows_total());
        let expected = read_fetch_rows(bytes, &index, 0, index.rows_total() as usize);
        let actual = doc.fetch_display_rows(0, index.rows_total() as usize);
        assert_eq!(actual.len(), expected.len());
        let mut joined = String::new();
        for (seg, (got, want)) in actual.iter().zip(expected.iter()).enumerate() {
            assert_eq!(got.row, seg as u64);
            assert_eq!(got.text, want.text, "段 {seg} 文本不一致");
            assert_eq!(got.logical_row, Some(0), "段 {seg} 逻辑行归属错误");
            joined.push_str(&got.text);
        }
        if joined != text {
            let first = joined
                .bytes()
                .zip(text.bytes())
                .position(|(left, right)| left != right);
            panic!(
                "分段拼接与原文不一致：joined.len()={} text.len()={} first_diff={first:?}",
                joined.len(),
                text.len()
            );
        }
        // 映射往返：逻辑位置 → 段 → 逻辑位置
        let (seg, local) = doc.seg_of_row_utf16(0, 5);
        let (row, base, _) = doc.seg_to_row_utf16(seg).expect("段应存在");
        assert_eq!(row, 0);
        assert_eq!(base + local, 5);
    }

    /// 光标落点字段：普通行与超长行（显示段坐标）均精确。
    #[test]
    fn applied_caret_tracks_edits() {
        let dir = tempfile::tempdir().expect("临时目录失败");
        let path = dir.path().join("caret.txt");
        std::fs::write(&path, "abc\ndef").expect("写文件失败");
        let mut doc = EditDoc::open(&path, None, 100).expect("打开失败");
        // 普通行插入：光标 = 插入末尾
        let applied = doc
            .apply_edits(&[EditOp::Insert {
                row: 1,
                utf16: 1,
                text: "XY".to_string(),
            }])
            .expect("插入失败");
        assert_eq!((applied.caret_row, applied.caret_utf16), (1, 3));
        // 删除：光标 = 删除区间起点
        let applied = doc
            .apply_edits(&[EditOp::Delete {
                start_row: 0,
                start_utf16: 1,
                end_row: 0,
                end_utf16: 2,
            }])
            .expect("删除失败");
        assert_eq!((applied.caret_row, applied.caret_utf16), (0, 1));
        // 超长行：20 000 字节单行（ASCII：1 字节 = 1 UTF-16 单元）在 8192 字节
        // 段边界插入 → 光标落在第 2 段内部
        let long = "a".repeat(20_000);
        let path2 = dir.path().join("long-caret.txt");
        std::fs::write(&path2, long.as_bytes()).expect("写文件失败");
        let mut doc2 = EditDoc::open(&path2, None, 100).expect("打开失败");
        let applied = doc2
            .apply_edits(&[EditOp::Insert {
                row: 0,
                utf16: 8192,
                text: "ab".to_string(),
            }])
            .expect("插入失败");
        assert_eq!((applied.caret_row, applied.caret_utf16), (1, 2));
        // 长行中插入换行：光标落到新逻辑行的段起点
        let applied = doc2
            .apply_edits(&[EditOp::Insert {
                row: 0,
                utf16: 8192,
                text: "\n".to_string(),
            }])
            .expect("插入失败");
        assert_eq!((applied.caret_row, applied.caret_utf16), (1, 0));
    }

    /// 长行内编辑：分段表随编辑更新，撤销完整恢复。
    #[test]
    fn edit_inside_long_row_updates_segments() {
        let text = "x".repeat(20_000);
        let (_dir, mut doc) = open_doc(text.as_bytes(), None);
        let baseline = doc.display_rows_total();
        assert!(baseline >= 3, "20000 字节应产生多段：{baseline}");
        let applied = doc
            .apply_edits(&[EditOp::Insert {
                row: 0,
                utf16: 20_000,
                text: "尾巴".into(),
            }])
            .expect("长行内插入失败");
        assert_eq!(applied.rows_total, doc.display_rows_total());
        assert!(doc.row_text(0).expect("行存在").ends_with("尾巴"));
        doc.undo().expect("应可撤销");
        assert_eq!(doc.display_rows_total(), baseline, "撤销后分段数应还原");
        assert_eq!(doc.row_text(0).as_deref(), Some(text.as_str()));
    }

    /// 长行拆分（插入换行）后分段表消失；撤销后恢复。
    #[test]
    fn long_row_split_and_restore() {
        let text = "a".repeat(12_000);
        let (_dir, mut doc) = open_doc(text.as_bytes(), None);
        let baseline = doc.display_rows_total();
        assert!(baseline > 1);
        // 在 6000 处拆分：两行长度均 ≤ 8KB，均不应再有分段
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 6000,
            text: "\n".into(),
        }])
        .expect("插入换行失败");
        assert_eq!(doc.rows_total(), 2);
        assert_eq!(
            doc.display_rows_total(),
            doc.rows_total(),
            "拆分后两行都未超长，显示段应等于逻辑行"
        );
        assert_eq!(doc.seg_of_row_utf16(1, 0).0, 1);
        doc.undo().expect("应可撤销");
        assert_eq!(doc.display_rows_total(), baseline);
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

    /// 多语言：日文行中插入、撤销还原。
    #[test]
    fn japanese_insert_and_undo() {
        let (_dir, mut doc) = open_doc("日本語テキスト".as_bytes(), None);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 3,
            text: "の編集".into(),
        }])
        .expect("插入失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("日本語の編集テキスト"));
        doc.undo().expect("撤销失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("日本語テキスト"));
        assert!(!doc.is_dirty());
    }

    /// 多语言：西里尔字母删除区间。
    #[test]
    fn cyrillic_delete_range() {
        let (_dir, mut doc) = open_doc("Привет мир".as_bytes(), None);
        doc.apply_edits(&[EditOp::Delete {
            start_row: 0,
            start_utf16: 7,
            end_row: 0,
            end_utf16: 10,
        }])
        .expect("删除失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("Привет "));
    }

    /// 多语言：ZWJ 家庭 emoji 整段删除/撤销不损坏文本。
    #[test]
    fn emoji_zwj_sequence_edit_is_safe() {
        let (_dir, mut doc) = open_doc("A👨‍👩‍👧‍👦B".as_bytes(), None);
        // ZWJ 家庭序列 = 4 个代理对(8 单元) + 3 个 ZWJ(3 单元) = 11 个 UTF-16 单元
        doc.apply_edits(&[EditOp::Delete {
            start_row: 0,
            start_utf16: 1,
            end_row: 0,
            end_utf16: 12,
        }])
        .expect("删除失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("AB"));
        doc.undo().expect("撤销失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("A👨‍👩‍👧‍👦B"));
    }

    /// 多语言：组合附加符插入后保存 UTF-8 往返一致。
    #[test]
    fn combining_mark_insert_roundtrip() {
        use crate::textfile::editing::save::{save_doc, SaveOptions};
        let (dir, mut doc) = open_doc("eX".as_bytes(), None);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 1,
            text: "\u{0301}".into(),
        }])
        .expect("插入失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("e\u{0301}X"));
        let path = dir.path().join("edit.txt");
        save_doc(
            &mut doc,
            &path,
            &SaveOptions {
                target_encoding: FileEncoding::Utf8,
                make_backup: false,
                force: true,
                expected: None,
            },
        )
        .expect("保存失败");
        let saved = std::fs::read(&path).expect("读回失败");
        assert_eq!(String::from_utf8(saved).expect("非 UTF-8"), "e\u{0301}X");
    }

    /// 多语言：Shift_JIS 文档编辑后按原编码保存，字节解码回一致。
    #[test]
    fn shift_jis_edit_save_roundtrip() {
        use crate::textfile::editing::save::{save_doc, SaveOptions};
        let bytes = encoding_rs::SHIFT_JIS
            .encode("こんにちは、世界。日本語のテスト。")
            .0
            .into_owned();
        let (dir, mut doc) = open_doc(&bytes, Some(FileEncoding::ShiftJis));
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 5,
            text: "東京".into(),
        }])
        .expect("插入失败");
        let path = dir.path().join("edit.txt");
        let outcome = save_doc(
            &mut doc,
            &path,
            &SaveOptions {
                target_encoding: FileEncoding::ShiftJis,
                make_backup: false,
                force: true,
                expected: None,
            },
        )
        .expect("保存失败");
        assert_eq!(outcome.encoding, FileEncoding::ShiftJis);
        let saved = std::fs::read(&path).expect("读回失败");
        assert_eq!(
            encoding_rs::SHIFT_JIS.decode(&saved).0,
            "こんにちは東京、世界。日本語のテスト。"
        );
        assert!(!doc.is_dirty());
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
    fn convert_eol_roundtrip_and_undo() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("e.txt");
        std::fs::write(&path, "a\nb\nc\n").expect("写文件");
        let mut doc = EditDoc::open(&path, None, 10).expect("打开");
        assert_eq!(doc.eol(), crate::textfile::eol::EolStyle::Lf);
        let before = doc.byte_len();

        let out = doc
            .convert_eol(crate::textfile::eol::EolTarget::CrLf)
            .expect("转 CRLF");
        assert_eq!(out.replacements, 3);
        assert!(out.applied.is_some());
        assert!(doc.is_dirty());
        assert_eq!(doc.eol(), crate::textfile::eol::EolStyle::CrLf);
        assert_eq!(doc.byte_len(), before + 3);
        let rows: Vec<String> = (0..doc.rows_total())
            .filter_map(|row| doc.row_text(row))
            .collect();
        assert_eq!(rows, vec!["a", "b", "c"]);

        doc.undo().expect("撤销");
        assert!(!doc.is_dirty());
        assert_eq!(doc.eol(), crate::textfile::eol::EolStyle::Lf);
        assert_eq!(doc.byte_len(), before);

        let out = doc
            .convert_eol(crate::textfile::eol::EolTarget::Lf)
            .expect("已是 LF");
        assert_eq!(out.replacements, 0);
        assert!(out.applied.is_none());
    }

    /// 回归：已有编辑（片段偏移 >0）后转换换行符（曾因 decode_slice 传绝对偏移越界崩溃）。
    #[test]
    fn convert_eol_after_edits_with_multibyte() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("edited.txt");
        std::fs::write(&path, "第一行\r\n第二行\r\n").expect("写文件");
        let mut doc = EditDoc::open(&path, None, 10).expect("打开");
        // 替换行首 3 个 UTF-16 单元（「第一行」）→ 制造 off>0 的原文片段
        doc.apply_edits(&[EditOp::Replace {
            start_row: 0,
            start_utf16: 0,
            end_row: 0,
            end_utf16: 3,
            text: "X".to_string(),
        }])
        .expect("替换");
        let outcome = doc
            .convert_eol(crate::textfile::eol::EolTarget::Lf)
            .expect("转换");
        assert_eq!(outcome.replacements, 2);
        assert_eq!(doc.row_text(0).as_deref(), Some("X"));
        assert_eq!(doc.row_text(1).as_deref(), Some("第二行"));
        let len_after = doc.byte_len();
        doc.undo().expect("撤销");
        assert_eq!(doc.byte_len(), len_after + 2, "两个 \\r 回退");
        assert_eq!(doc.row_text(0).as_deref(), Some("X"));
    }

    /// 混合风格归一为 LF（CRLF 与孤立 CR 均处理），撤销后恢复 Mixed。
    #[test]
    fn convert_eol_mixed_to_lf_and_back() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("m.txt");
        std::fs::write(&path, "a\r\nb\rc\nd\r\n").expect("写文件");
        let mut doc = EditDoc::open(&path, None, 10).expect("打开");
        assert_eq!(doc.eol(), crate::textfile::eol::EolStyle::Mixed);

        let out = doc
            .convert_eol(crate::textfile::eol::EolTarget::Lf)
            .expect("归一 LF");
        assert_eq!(out.replacements, 4);
        assert_eq!(doc.eol(), crate::textfile::eol::EolStyle::Lf);
        let rows: Vec<String> = (0..doc.rows_total())
            .filter_map(|row| doc.row_text(row))
            .collect();
        assert_eq!(rows, vec!["a", "b", "c", "d"]);

        doc.undo().expect("撤销");
        assert_eq!(doc.eol(), crate::textfile::eol::EolStyle::Mixed);
    }

    /// 超出预算：拒绝且内容/脏标记不变。
    #[test]
    fn convert_eol_over_budget_is_rejected_without_change() {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("big.txt");
        std::fs::write(&path, "a\nb\nc\n").expect("写文件");
        let mut doc = EditDoc::open(&path, None, 10).expect("打开");
        let before = doc.byte_len();
        let err = doc
            .convert_eol_with(crate::textfile::eol::EolTarget::CrLf, 4)
            .expect_err("应超限");
        assert!(matches!(err, EditError::EolConvertTooLarge { .. }));
        assert_eq!(doc.byte_len(), before);
        assert!(!doc.is_dirty());
    }

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

    /// 大文件深行取行基准（手动运行）：
    /// `cargo test --lib benchmark_deep_fetch -- --ignored --nocapture`
    #[test]
    #[ignore = "性能基准，手动运行"]
    fn benchmark_deep_fetch() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("deep.txt");
        let line: &str =
            "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-=-=--==-=-=--==0\n";
        let rows_total: u64 = 1_300_000;
        {
            use std::io::Write as _;
            let mut file =
                std::io::BufWriter::new(std::fs::File::create(&path).expect("创建基准文件失败"));
            for _ in 0..rows_total {
                file.write_all(line.as_bytes()).expect("写基准文件失败");
            }
        }
        let open_start = std::time::Instant::now();
        let doc = EditDoc::open(&path, None, 100).expect("打开失败");
        println!(
            "EditDoc::open(100MB/1.3M 行)：{}ms",
            open_start.elapsed().as_millis()
        );
        for row in [0u64, 100_000, 600_000, 1_299_000] {
            let start = std::time::Instant::now();
            let rows = doc.fetch_rows(row, 40);
            println!(
                "fetch_rows(row={row}, 40)：{}ms（取到 {} 行）",
                start.elapsed().as_millis(),
                rows.len()
            );
        }
    }

    /// 应用阶段取消：完整回滚、无撤销步骤、不推进版本号、不产生脏标记。
    #[test]
    fn apply_cancellable_rolls_back_during_apply() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "cancel.txt", b"a\nb\nc\nd\n");
        let mut doc = EditDoc::open(&path, None, 100).expect("打开失败");
        let before_rows = doc.fetch_rows(0, doc.rows_total() as usize);
        let before_state = doc.state_id;
        let ops = [
            EditOp::Insert {
                row: 0,
                utf16: 0,
                text: "X".into(),
            },
            EditOp::Insert {
                row: 2,
                utf16: 0,
                text: "Y".into(),
            },
        ];
        let flag = std::cell::Cell::new(false);
        let cancel = || flag.get();
        let mut apply_calls = 0u32;
        let result = doc
            .apply_edits_cancellable(&ops, &cancel, &mut |done, _| {
                apply_calls += 1;
                if done >= 1 {
                    flag.set(true);
                }
            })
            .expect("不应报错");
        assert!(result.is_none(), "应返回取消（进度回调 {apply_calls} 次）");
        assert_eq!(doc.state_id, before_state, "取消不应推进版本号");
        assert_eq!(
            doc.fetch_rows(0, doc.rows_total() as usize),
            before_rows,
            "内容应完整回滚"
        );
        assert!(!doc.is_dirty(), "取消不应产生脏标记");
        assert!(doc.undo().is_none(), "取消不应产生撤销步骤");
    }

    /// 解析阶段取消（始终取消）：不修改文档且快速返回。
    #[test]
    fn apply_cancellable_cancels_before_apply() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "cancel2.txt", b"one\ntwo\nthree\n");
        let mut doc = EditDoc::open(&path, None, 100).expect("打开失败");
        let before_rows = doc.fetch_rows(0, doc.rows_total() as usize);
        let result = doc
            .apply_edits_cancellable(
                &[EditOp::Insert {
                    row: 1,
                    utf16: 0,
                    text: "Z".into(),
                }],
                &|| true,
                &mut |_, _| {},
            )
            .expect("不应报错");
        assert!(result.is_none());
        assert_eq!(doc.fetch_rows(0, doc.rows_total() as usize), before_rows);
        assert!(doc.undo().is_none());
    }

    /// 不取消时批次编辑与普通路径结果一致，且进度以 (len, len) 收尾。
    #[test]
    fn apply_cancellable_matches_apply_and_reports_progress() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path_a = write_file(dir.path(), "batch-a.txt", b"1\n2\n3\n4\n5\n");
        let path_b = write_file(dir.path(), "batch-b.txt", b"1\n2\n3\n4\n5\n");
        let mut doc_a = EditDoc::open(&path_a, None, 100).expect("打开失败");
        let mut doc_b = EditDoc::open(&path_b, None, 100).expect("打开失败");
        let ops = [
            EditOp::Insert {
                row: 0,
                utf16: 0,
                text: "A".into(),
            },
            EditOp::Insert {
                row: 4,
                utf16: 1,
                text: "B".into(),
            },
        ];
        let expected = doc_a.apply_edits(&ops).expect("普通路径失败");
        let mut last = (0u64, 0u64);
        let actual = doc_b
            .apply_edits_cancellable(&ops, &|| false, &mut |done, total| last = (done, total))
            .expect("可取消路径失败")
            .expect("不取消应返回结果");
        assert_eq!(last, (ops.len() as u64, ops.len() as u64));
        assert_eq!(actual.rows_total, expected.rows_total);
        assert_eq!(actual.touched_row, expected.touched_row);
        assert_eq!(
            doc_b.fetch_rows(0, doc_b.rows_total() as usize),
            doc_a.fetch_rows(0, doc_a.rows_total() as usize)
        );
    }
}
