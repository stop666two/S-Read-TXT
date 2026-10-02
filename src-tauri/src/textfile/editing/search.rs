//! 查找与替换（普通文本；大小写敏感开关；不做正则）——设计文档 §5.3。
//!
//! 实现要点：
//! - **流式扫描**：按 1MB 文件字节分块解码，块间保留 `(查询长度×4 + 8)` 字节
//!   的「接续区」（carry），可命中跨块与跨片段的匹配；内存与文件大小无关；
//! - **坐标**：扫描同时维护「逻辑行 + 行内 UTF-16」游标（CRLF 视为单一换行
//!   单元），命中直接产出与前端编辑坐标一致的显示坐标；
//! - **大小写不敏感** = 基于 `char::to_lowercase` 简单折叠的逐字符比对；
//! - **全部替换** = 单次 [`EditDoc::apply_edits`]（单个撤销步）；
//!   命中数超过 [`REPLACE_ALL_LIMIT`] 时拒绝执行（防止构造超大操作批次）；
//! - 以 CRLF 的 `\n` 起始的命中会向前扩展包含 `\r`，保证替换坐标可精确解析。

use serde::Serialize;

use super::edit_doc::{EditApplied, EditDoc, EditError, EditOp};
use super::piece::PieceSource;

/// 扫描块大小（文件字节）。
const SEARCH_CHUNK_BYTES: usize = 1 << 20;

/// 「全部替换」命中数上限（超出报 [`EditError::TooManyMatches`]）。
pub const REPLACE_ALL_LIMIT: usize = 200_000;

/// 一次查找命中（逻辑行坐标：行号 + 行内 UTF-16 偏移；与前端编辑坐标一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindHit {
    /// 起始行号（逻辑行）
    pub start_row: u64,
    /// 起始行内 UTF-16 偏移
    pub start_utf16: u64,
    /// 结束行号（逻辑行）
    pub end_row: u64,
    /// 结束行内 UTF-16 偏移
    pub end_utf16: u64,
}

/// 「替换下一次」的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceNextOutcome {
    /// 本次替换的编辑结果
    pub applied: EditApplied,
    /// 替换后从光标起继续查找的下一次命中
    pub next: Option<FindHit>,
}

/// 「全部替换」的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceAllOutcome {
    /// 实际替换次数
    pub replaced: usize,
    /// 编辑结果（无命中时为 `None`，文档保持不变）
    pub applied: Option<EditApplied>,
}

impl EditDoc {
    /// 从文档位置 `from`（默认文档开头）起向后查找 `query` 的下一次出现。
    ///
    /// - `case_sensitive`：大小写敏感开关；
    /// - 不环绕（前端负责到文件末尾后从头重试）；
    /// - 返回逻辑行坐标（超长行的显示分段映射由前端完成）。
    pub fn find(
        &self,
        query: &str,
        case_sensitive: bool,
        from: Option<(u64, u64)>,
    ) -> Result<Option<FindHit>, EditError> {
        let mut hit = None;
        self.scan(query, case_sensitive, from, |found| {
            hit = Some(found);
            false
        })?;
        Ok(hit)
    }

    /// 查找并从 `from` 起替换下一次出现；返回替换结果与后续命中（便于连续替换）。
    ///
    /// 无命中时返回 `Ok(None)`（文档不变）。
    pub fn replace_next(
        &mut self,
        query: &str,
        case_sensitive: bool,
        from: Option<(u64, u64)>,
        replacement: &str,
    ) -> Result<Option<ReplaceNextOutcome>, EditError> {
        let Some(hit) = self.find(query, case_sensitive, from)? else {
            return Ok(None);
        };
        let op = EditOp::Replace {
            start_row: hit.start_row,
            start_utf16: hit.start_utf16,
            end_row: hit.end_row,
            end_utf16: hit.end_utf16,
            text: replacement.to_string(),
        };
        let applied = self.apply_edits(&[op])?;
        let next = self.find(
            query,
            case_sensitive,
            Some((applied.caret_row, applied.caret_utf16)),
        )?;
        Ok(Some(ReplaceNextOutcome { applied, next }))
    }

    /// 全部替换为 `replacement`（单次编辑 = 单个撤销步）。
    ///
    /// 命中数上限 [`REPLACE_ALL_LIMIT`]；超出时报 `TooManyMatches` 且文档不变。
    pub fn replace_all(
        &mut self,
        query: &str,
        case_sensitive: bool,
        replacement: &str,
    ) -> Result<ReplaceAllOutcome, EditError> {
        let mut hits: Vec<FindHit> = Vec::new();
        let mut overflow = false;
        self.scan(query, case_sensitive, None, |found| {
            hits.push(found);
            if hits.len() > REPLACE_ALL_LIMIT {
                overflow = true;
                false
            } else {
                true
            }
        })?;
        if overflow {
            return Err(EditError::TooManyMatches {
                limit: REPLACE_ALL_LIMIT,
            });
        }
        if hits.is_empty() {
            return Ok(ReplaceAllOutcome {
                replaced: 0,
                applied: None,
            });
        }
        let ops: Vec<EditOp> = hits
            .iter()
            .map(|hit| EditOp::Replace {
                start_row: hit.start_row,
                start_utf16: hit.start_utf16,
                end_row: hit.end_row,
                end_utf16: hit.end_utf16,
                text: replacement.to_string(),
            })
            .collect();
        let applied = self.apply_edits(&ops)?;
        Ok(ReplaceAllOutcome {
            replaced: hits.len(),
            applied: Some(applied),
        })
    }

    /// 核心扫描：流式解码 + 游标推进，逐次回调命中；回调返回 `false` 时停止。
    fn scan(
        &self,
        query: &str,
        case_sensitive: bool,
        from: Option<(u64, u64)>,
        mut on_hit: impl FnMut(FindHit) -> bool,
    ) -> Result<(), EditError> {
        if query.is_empty() {
            return Ok(());
        }
        let (start_row, start_utf16) = from.unwrap_or((0, 0));
        let start_pos = self.resolve_pos(start_row, start_utf16)?;
        let global_start = self.global_offset(start_pos);
        let mut cursor = ScanCursor {
            row: start_row,
            utf16: start_utf16,
            prev_cr: self.char_before_is_cr(global_start),
        };
        let query_fold: Vec<char> = if case_sensitive {
            Vec::new()
        } else {
            query.chars().flat_map(|ch| ch.to_lowercase()).collect()
        };
        let mut carry = String::new();
        let pieces = self.pieces();
        for piece_index in start_pos.piece..pieces.len() {
            let piece = pieces[piece_index];
            let mut off = if piece_index == start_pos.piece {
                start_pos.off
            } else {
                0
            };
            if off >= piece.len {
                continue;
            }
            match piece.source {
                PieceSource::Added => {
                    let bytes = self.piece_bytes(&piece);
                    while off < piece.len {
                        let mut take = ((piece.len - off) as usize).min(SEARCH_CHUNK_BYTES);
                        let end = (off as usize) + take;
                        if end < piece.len as usize {
                            take = ceil_char_boundary_byte(bytes, end) - off as usize;
                        }
                        let chunk =
                            String::from_utf8_lossy(&bytes[off as usize..off as usize + take]);
                        let keep = self.consume_chunk(
                            chunk.as_ref(),
                            query,
                            &query_fold,
                            case_sensitive,
                            &mut carry,
                            &mut cursor,
                            &mut on_hit,
                        );
                        off += take as u64;
                        if !keep {
                            return Ok(());
                        }
                    }
                }
                PieceSource::Original => {
                    let bytes = self.piece_bytes(&piece);
                    let encoding = self.encoding_for(&piece);
                    let mut decoder = encoding.encoding().new_decoder_without_bom_handling();
                    while off < piece.len {
                        let take = ((piece.len - off) as usize).min(SEARCH_CHUNK_BYTES);
                        let mut buffer = String::with_capacity(take + 16);
                        let (_result, read, _replaced) = decoder.decode_to_string(
                            &bytes[off as usize..off as usize + take],
                            &mut buffer,
                            false,
                        );
                        if read == 0 {
                            // 仅可能出现在剩余不足一个字符时：用 last 冲洗缓冲
                            let (_r, _rd, _rp) = decoder.decode_to_string(&[], &mut buffer, true);
                        }
                        if !buffer.is_empty() {
                            let keep = self.consume_chunk(
                                &buffer,
                                query,
                                &query_fold,
                                case_sensitive,
                                &mut carry,
                                &mut cursor,
                                &mut on_hit,
                            );
                            if !keep {
                                return Ok(());
                            }
                        }
                        if read == 0 {
                            break;
                        }
                        off += read as u64;
                    }
                }
            }
        }
        Ok(())
    }

    /// 处理一个已解码文本块：拼接接续区、搜索命中、推进游标、更新接续区。
    ///
    /// 返回 `false` 表示调用方应停止扫描（回调要求停止）。
    fn consume_chunk(
        &self,
        chunk: &str,
        query: &str,
        query_fold: &[char],
        case_sensitive: bool,
        carry: &mut String,
        cursor: &mut ScanCursor,
        on_hit: &mut impl FnMut(FindHit) -> bool,
    ) -> bool {
        if chunk.is_empty() {
            return true;
        }
        let carry_len = carry.len();
        let mut text = String::with_capacity(carry_len + chunk.len());
        text.push_str(carry);
        text.push_str(chunk);
        let text_bytes = text.as_bytes();
        let mut work = *cursor;
        let mut text_pos = 0usize;
        let mut search_from = 0usize;
        let mut keep_going = true;
        while let Some((match_start, match_end)) =
            next_match(&text, search_from, query, query_fold, case_sensitive)
        {
            let mut start = match_start;
            search_from = match_end;
            // 完全落在接续区内的命中：上一块已处理过
            if match_end <= carry_len {
                continue;
            }
            // 以 CRLF 的 `\n` 起始：向前扩展包含其 `\r`，保证编辑坐标可精确解析
            if start > 0 && text_bytes[start] == b'\n' && text_bytes[start - 1] == b'\r' {
                if start - 1 < text_pos {
                    continue; // 无法扩展（会与上一命中重叠）：防御性跳过
                }
                start -= 1;
            } else if start == 0 && text_bytes[0] == b'\n' && work.prev_cr {
                // `\r` 落在上一解码块：起始坐标无法表达，防御性跳过
                continue;
            }
            work.advance(&text[text_pos..start]);
            let start_row = work.row;
            let start_utf16 = work.utf16;
            work.advance(&text[start..match_end]);
            text_pos = match_end;
            let hit = FindHit {
                start_row,
                start_utf16,
                end_row: work.row,
                end_utf16: work.utf16,
            };
            if !on_hit(hit) {
                keep_going = false;
                break;
            }
        }
        // 游标推进到新接续区起点（与命中处理相互独立，保证坐标一致性）
        let keep_start = carry_start(&text, query, case_sensitive);
        cursor.advance(&text[..keep_start]);
        *carry = text[keep_start..].to_string();
        keep_going
    }
}

/// 扫描游标：把「已消费文本」累计为（逻辑行号, 行内 UTF-16 偏移）。
#[derive(Debug, Clone, Copy)]
struct ScanCursor {
    /// 当前行号（逻辑行）
    row: u64,
    /// 行内 UTF-16 偏移
    utf16: u64,
    /// 上一字符是否为 `\r`（CRLF 单元的换行计数去重）
    prev_cr: bool,
}

impl ScanCursor {
    /// 消费一段文本，推进游标（换行族 `\n` / `\r\n` / `\r` 语义与行索引一致）。
    fn advance(&mut self, text: &str) {
        for ch in text.chars() {
            match ch {
                '\r' => {
                    self.row += 1;
                    self.utf16 = 0;
                    self.prev_cr = true;
                }
                '\n' => {
                    if !self.prev_cr {
                        self.row += 1;
                        self.utf16 = 0;
                    }
                    self.prev_cr = false;
                }
                _ => {
                    self.utf16 += ch.len_utf16() as u64;
                    self.prev_cr = false;
                }
            }
        }
    }
}

/// 在 `text` 中从 `from` 起查找下一次匹配，返回 `[起, 止)` 字节区间。
fn next_match(
    text: &str,
    from: usize,
    query: &str,
    query_fold: &[char],
    case_sensitive: bool,
) -> Option<(usize, usize)> {
    if from > text.len() {
        return None;
    }
    if case_sensitive {
        text[from..]
            .find(query)
            .map(|index| (from + index, from + index + query.len()))
    } else {
        for (index, _ch) in text[from..].char_indices() {
            let at = from + index;
            if let Some(consumed) = ci_match(&text[at..], query_fold) {
                return Some((at, at + consumed));
            }
        }
        None
    }
}

/// 大小写不敏感比对：`hay` 以 `pos` 起始是否匹配折叠后的查询。
///
/// 返回匹配消耗的字节数（按完整字符计）；语义 = 在「小写折叠文本」上做子串匹配
/// （例如 `İ` 折叠为 `i` + 组合点，可匹配查询 `i` 并整体消耗该字符）。
fn ci_match(hay: &str, query_fold: &[char]) -> Option<usize> {
    let mut qi = 0usize;
    let mut consumed = 0usize;
    for ch in hay.chars() {
        for folded in ch.to_lowercase() {
            if qi >= query_fold.len() || folded != query_fold[qi] {
                return None;
            }
            qi += 1;
        }
        consumed += ch.len_utf8();
        if qi == query_fold.len() {
            return Some(consumed);
        }
    }
    None
}

/// 接续区起点：让跨块匹配可被检出，同时避免重复上报已处理的命中。
fn carry_start(text: &str, query: &str, case_sensitive: bool) -> usize {
    let max_span = if case_sensitive {
        query.len().saturating_sub(1)
    } else {
        query.len().saturating_mul(4) + 8
    };
    if max_span == 0 || max_span >= text.len() {
        return if max_span == 0 { text.len() } else { 0 };
    }
    ceil_char_boundary(text, text.len() - max_span)
}

/// 返回 ≥ `index` 的最小字符边界（UTF-8 域）。
fn ceil_char_boundary(text: &str, index: usize) -> usize {
    let mut i = index.min(text.len());
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// 返回 ≥ `index` 的最小字符边界（字节切片级，供 UTF-8 新增片段分块）。
fn ceil_char_boundary_byte(bytes: &[u8], index: usize) -> usize {
    let mut i = index.min(bytes.len());
    while i < bytes.len() && (bytes[i] & 0xC0) == 0x80 {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textfile::encoding::FileEncoding;
    use std::path::PathBuf;

    /// 构造样本文件并打开编辑文档。
    fn open_doc(bytes: &[u8], encoding: Option<FileEncoding>) -> (tempfile::TempDir, EditDoc) {
        let dir = tempfile::tempdir().expect("临时目录失败");
        let path: PathBuf = dir.path().join("search-sample.txt");
        std::fs::write(&path, bytes).expect("写样本失败");
        let doc = EditDoc::open(&path, encoding, 100).expect("打开失败");
        (dir, doc)
    }

    /// 构造命中坐标的测试夹具。
    fn hit(sr: u64, su: u64, er: u64, eu: u64) -> FindHit {
        FindHit {
            start_row: sr,
            start_utf16: su,
            end_row: er,
            end_utf16: eu,
        }
    }

    /// 顺序查找下一个。
    fn find(doc: &EditDoc, query: &str, cs: bool, from: Option<(u64, u64)>) -> Option<FindHit> {
        doc.find(query, cs, from).expect("查找失败")
    }

    #[test]
    fn finds_next_occurrence_forward() {
        let (_dir, doc) = open_doc(b"hello world hello", None);
        assert_eq!(find(&doc, "hello", true, None), Some(hit(0, 0, 0, 5)));
        assert_eq!(
            find(&doc, "hello", true, Some((0, 6))),
            Some(hit(0, 12, 0, 17))
        );
        assert_eq!(find(&doc, "hello", true, Some((0, 17))), None);
        assert_eq!(find(&doc, "missing", true, None), None);
    }

    #[test]
    fn case_sensitive_toggle() {
        let (_dir, doc) = open_doc(b"Alpha alpha ALPHA", None);
        assert_eq!(find(&doc, "alpha", true, None), Some(hit(0, 6, 0, 11)));
        assert_eq!(find(&doc, "alpha", false, None), Some(hit(0, 0, 0, 5)));
        assert_eq!(
            find(&doc, "alpha", false, Some((0, 5))),
            Some(hit(0, 6, 0, 11))
        );
        assert_eq!(
            find(&doc, "alpha", false, Some((0, 11))),
            Some(hit(0, 12, 0, 17))
        );
        assert_eq!(find(&doc, "alpha", false, Some((0, 17))), None);
    }

    #[test]
    fn case_insensitive_unicode_fold() {
        let (_dir, doc) = open_doc("CAFÉ café".as_bytes(), None);
        assert_eq!(find(&doc, "café", true, None), Some(hit(0, 5, 0, 9)));
        assert_eq!(find(&doc, "café", false, None), Some(hit(0, 0, 0, 4)));
    }

    #[test]
    fn multiline_query_across_rows() {
        let (_dir, doc) = open_doc(b"aa\nbb\naa\nbb", None);
        assert_eq!(find(&doc, "aa\nbb", true, None), Some(hit(0, 0, 1, 2)));
        assert_eq!(
            find(&doc, "aa\nbb", true, Some((0, 1))),
            Some(hit(2, 0, 3, 2))
        );
    }

    #[test]
    fn crlf_query_extends_over_slash_r() {
        let (_dir, doc) = open_doc(b"a\r\nb", None);
        // 查询 "\nb"：命中从 CRLF 的 '\n' 起始 → 扩展包含 '\r'，坐标可精确解析
        assert_eq!(find(&doc, "\nb", true, None), Some(hit(0, 1, 1, 1)));
        // 普通命中：b 位于第二行行首
        assert_eq!(find(&doc, "b", true, None), Some(hit(1, 0, 1, 1)));
    }

    #[test]
    fn crosses_chunk_boundary() {
        let mut content = "x".repeat(SEARCH_CHUNK_BYTES - 2);
        content.push_str("ZZneedleZZ");
        let (_dir, doc) = open_doc(content.as_bytes(), None);
        let chunk = SEARCH_CHUNK_BYTES as u64;
        assert_eq!(
            find(&doc, "needle", true, None),
            Some(hit(0, chunk, 0, chunk + 6))
        );
    }

    #[test]
    fn case_insensitive_spanning_chunk_boundary() {
        let mut content = "x".repeat(SEARCH_CHUNK_BYTES - 3);
        content.push_str("John john");
        let (_dir, doc) = open_doc(content.as_bytes(), None);
        let chunk = SEARCH_CHUNK_BYTES as u64;
        assert_eq!(
            find(&doc, "john", false, None),
            Some(hit(0, chunk - 3, 0, chunk + 1))
        );
    }

    #[test]
    fn match_across_edit_pieces() {
        let (_dir, mut doc) = open_doc(b"AAA", None);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 0,
            text: "nee".to_string(),
        }])
        .expect("首次插入失败");
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 6,
            text: "dle".to_string(),
        }])
        .expect("二次插入失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("neeAAAdle"));
        assert_eq!(find(&doc, "eAAAd", true, None), Some(hit(0, 2, 0, 7)));
    }

    #[test]
    fn replace_next_advances() {
        let (_dir, mut doc) = open_doc(b"foo bar foo", None);
        let outcome = doc
            .replace_next("foo", true, None, "FOO")
            .expect("替换失败")
            .expect("应有命中");
        assert!(outcome.applied.dirty);
        assert_eq!(doc.row_text(0).as_deref(), Some("FOO bar foo"));
        assert_eq!(outcome.next, Some(hit(0, 8, 0, 11)));
        let second = doc
            .replace_next(
                "foo",
                true,
                outcome.next.map(|h| (h.start_row, h.start_utf16)),
                "FOO",
            )
            .expect("二次替换失败")
            .expect("应有第二次命中");
        assert_eq!(doc.row_text(0).as_deref(), Some("FOO bar FOO"));
        assert_eq!(second.next, None);
        assert!(doc
            .replace_next("foo", true, None, "X")
            .expect("无命中替换失败")
            .is_none());
    }

    #[test]
    fn replace_all_is_single_undo_step() {
        let (_dir, mut doc) = open_doc(b"aXaXa", None);
        let outcome = doc.replace_all("a", true, "b").expect("全部替换失败");
        assert_eq!(outcome.replaced, 3);
        assert!(outcome.applied.expect("应有编辑结果").dirty);
        assert_eq!(doc.row_text(0).as_deref(), Some("bXbXb"));
        doc.undo().expect("撤销失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("aXaXa"));
        assert!(!doc.is_dirty());
    }

    #[test]
    fn replace_all_no_match_keeps_document() {
        let (_dir, mut doc) = open_doc(b"abc", None);
        let outcome = doc.replace_all("z", true, "y").expect("全部替换失败");
        assert_eq!(outcome.replaced, 0);
        assert!(outcome.applied.is_none());
        assert!(!doc.is_dirty());
    }

    #[test]
    fn replace_all_overflow_is_rejected() {
        let content = "a".repeat(REPLACE_ALL_LIMIT + 1);
        let (_dir, mut doc) = open_doc(content.as_bytes(), None);
        let err = doc.replace_all("a", true, "b").expect_err("应拒绝");
        assert!(matches!(err, EditError::TooManyMatches { .. }));
        assert!(!doc.is_dirty());
        assert_eq!(doc.byte_len(), (REPLACE_ALL_LIMIT + 1) as u64);
    }

    #[test]
    fn utf16_file_search() {
        let text = "hello 世界 hello";
        let bytes: Vec<u8> = text
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect();
        let (_dir, doc) = open_doc(&bytes, Some(FileEncoding::Utf16Le));
        assert_eq!(find(&doc, "世界", true, None), Some(hit(0, 6, 0, 8)));
        assert_eq!(
            find(&doc, "hello", true, Some((0, 7))),
            Some(hit(0, 9, 0, 14))
        );
    }

    #[test]
    fn empty_query_is_noop() {
        let (_dir, mut doc) = open_doc(b"abc", None);
        assert_eq!(find(&doc, "", true, None), None);
        let outcome = doc.replace_all("", true, "x").expect("空查询失败");
        assert_eq!(outcome.replaced, 0);
        assert!(outcome.applied.is_none());
    }

    #[test]
    fn from_row_out_of_range_is_error() {
        let (_dir, doc) = open_doc(b"abc", None);
        let err = doc.find("a", true, Some((99, 0))).expect_err("应报越界");
        assert!(matches!(err, EditError::RowOutOfRange { .. }));
    }
}
