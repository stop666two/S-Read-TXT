//! `EditDoc` 的文本统计扩展。
//!
//! 独立成模块以避免 `edit_doc.rs` 继续膨胀；实现仅依赖既有
//! `pub(crate)` 内部访问器（片段表 / 新增缓冲 / 解码切片），不改变文档状态。

use crate::stats::{StatsAccumulator, TextStats};
use crate::textfile::editing::edit_doc::{EditDoc, EditError};
use crate::textfile::editing::piece::{Piece, PieceSource};

/// 原文片段的解码窗口（仅统计用；与查找扫描窗口独立）。
const STATS_WINDOW_BYTES: u64 = 256 * 1024;

impl EditDoc {
    /// 文档级统计：遍历全部片段流式计数；`max_chars` 上限后标记 `capped`。
    ///
    /// 字节口径 = `byte_len()`（原始文件字节 + BOM），与状态栏「大小」一致。
    pub fn document_stats(&self, max_chars: usize) -> TextStats {
        let mut acc = StatsAccumulator::new(max_chars);
        for piece in self.pieces() {
            push_piece(&mut acc, self, piece);
            if acc.is_capped() {
                break;
            }
        }
        acc.finish(self.byte_len())
    }

    /// 范围统计（逻辑行坐标，半开区间）；字节口径 = 文本的 UTF-8 字节数。
    ///
    /// 返回：`Err(EditError)`（坐标越界/非法）。
    pub fn range_stats(
        &self,
        from: (u64, u64),
        to: (u64, u64),
        max_chars: usize,
    ) -> Result<TextStats, EditError> {
        let start = self.global_offset(self.resolve_pos(from.0, from.1)?);
        let end = self.global_offset(self.resolve_pos(to.0, to.1)?);
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        let mut acc = StatsAccumulator::new(max_chars);
        let mut utf8_bytes: u64 = 0;
        let mut base: u64 = 0;
        for piece in self.pieces() {
            let piece_end = base + piece.len;
            if piece_end > start && base < end && !acc.is_capped() {
                let slice_from = start.max(base) - base;
                let slice_to = end.min(piece_end) - base;
                utf8_bytes += push_piece_range(&mut acc, self, piece, slice_from, slice_to);
            }
            base = piece_end;
            if base >= end || acc.is_capped() {
                break;
            }
        }
        Ok(acc.finish(utf8_bytes))
    }
}

/// 统计单片段全部内容（新增缓冲按 UTF-8 直读；原文按编码流式解码）。
fn push_piece(acc: &mut StatsAccumulator, doc: &EditDoc, piece: &Piece) -> u64 {
    push_piece_range(acc, doc, piece, 0, piece.len)
}

/// 统计单片段内 `[from, to)`（片段内字节偏移）；返回解码文本的 UTF-8 字节数。
fn push_piece_range(
    acc: &mut StatsAccumulator,
    doc: &EditDoc,
    piece: &Piece,
    from: u64,
    to: u64,
) -> u64 {
    let mut utf8_bytes: u64 = 0;
    match piece.source {
        PieceSource::Added => {
            let bytes = doc.added();
            let start = (piece.off + from) as usize;
            let end = (piece.off + to) as usize;
            // 新增缓冲恒为合法 UTF-8（引擎不变量）；异常时按空跳过而不 panic。
            if let Ok(text) = std::str::from_utf8(&bytes[start..end]) {
                acc.push_str(text);
                utf8_bytes += text.len() as u64;
            }
        }
        PieceSource::Original => {
            let enc = doc.encoding_for(piece).encoding();
            let mut decoder = enc.new_decoder_without_bom_handling();
            let src = doc.original_bytes();
            let mut off = piece.off + from;
            let piece_end = piece.off + to;
            let mut buf = String::new();
            while off < piece_end && !acc.is_capped() {
                let end = (off + STATS_WINDOW_BYTES).min(piece_end);
                // 预扩容 + 循环扩容（见 stats::decode_stream）。
                buf.reserve(((end - off) as usize) * 4 + 64);
                crate::stats::decode_stream(
                    &mut decoder,
                    &src[off as usize..end as usize],
                    &mut buf,
                    false,
                );
                if !buf.is_empty() {
                    acc.push_str(&buf);
                    utf8_bytes += buf.len() as u64;
                    buf.clear();
                }
                off = end;
            }
        }
    }
    utf8_bytes
}
