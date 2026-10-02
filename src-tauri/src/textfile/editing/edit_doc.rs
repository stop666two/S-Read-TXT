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

use crate::textfile::editing::fenwick::Fenwick;
use crate::textfile::editing::piece::{
    count_units, newline_width, starts_with_lf, Piece, PieceMeta, PieceSource,
};
use crate::textfile::editing::EDIT_MAX_ROW_BYTES;
use crate::textfile::encoding::{decode_range, detect, FileEncoding};
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
        let start = (piece.off + from) as usize;
        let end = (piece.off + to) as usize;
        match piece.source {
            PieceSource::Original => {
                out.push_str(&decode_range(&self.original.bytes()[start..end], self.encoding))
            }
            // 新增片段是 UTF-8；不用 decode_range 以避免把用户文本的前导
            // U+FEFF 当作 BOM 误剥离
            PieceSource::Added => out.push_str(&String::from_utf8_lossy(&self.added[start..end])),
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
        Ok((
            DocPos {
                piece: piece_index,
                off: start_off,
            },
            self.normalize_pos(DocPos {
                piece: piece_index,
                off: end_off,
            }),
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
}
