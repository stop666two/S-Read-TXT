//! 文本统计：行/词/字符等文档级统计，供状态栏展示。
//!
//! 提供流式、有上限的字符统计：字素簇（UAX #29）/ 码点 / 字节 / 词数
//! （口径已确认：空白分词）。统计按块累积，跨块的组合字符与代理对安全；
//! 超过 `max_chars` 码点后停止计数并标记 `capped`（界面以「≥」展示）。
//!
//! 口径说明：
//! - 字节：文档统计 = 文件原始字节数（含 BOM）；选区统计 = 选中文本的
//!   UTF-8 编码字节数（两种语境分属不同接口，注释与文档一致）。
//! - 词：以 Unicode 空白字符为分隔符的连续非空白序列（`split_whitespace` 语义）。

use serde::Serialize;
use unicode_segmentation::UnicodeSegmentation;

use crate::textfile::encoding::FileEncoding;

/// 文档统计的码点上限（超过即 `capped`）。
pub const STATS_MAX_CHARS: usize = 50_000_000;
/// 选区统计的码点上限（远超正常选区规模，防御异常范围）。
pub const SELECTION_STATS_MAX_CHARS: usize = 1_000_000;

/// 块间携带的最大字节数（覆盖实际可见的组合序列）。
const CARRY_BYTES: usize = 64;

/// 文本统计结果（IPC 返回体；字段 camelCase）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextStats {
    /// 字素簇数（用户感知的「字符」）
    pub graphemes: u64,
    /// Unicode 码点数
    pub codepoints: u64,
    /// 字节数（口径见模块文档）
    pub bytes: u64,
    /// 词数（空白分词）
    pub words: u64,
    /// 是否因超过上限提前停止计数
    pub capped: bool,
}

/// 流式统计累加器。
///
/// 用法：对每个解码后的文本块调用 [`push_str`](Self::push_str)，
/// 最后调用 [`finish`](Self::finish) 并传入按口径统计的字节数。
pub struct StatsAccumulator {
    graphemes: u64,
    codepoints: u64,
    words: u64,
    capped: bool,
    max_chars: usize,
    prev_whitespace: bool,
    carry: String,
    carry_graphemes: u64,
}

impl StatsAccumulator {
    /// 新建累加器；`max_chars` 为码点上限（达到即停止计数）。
    pub fn new(max_chars: usize) -> Self {
        Self {
            graphemes: 0,
            codepoints: 0,
            words: 0,
            capped: false,
            max_chars,
            prev_whitespace: true,
            carry: String::new(),
            carry_graphemes: 0,
        }
    }

    /// 是否已因超限停止计数。
    pub fn is_capped(&self) -> bool {
        self.capped
    }

    /// 追加一块文本（必须为完整 UTF-8 文本块；跨块组合序列由内部携带处理）。
    pub fn push_str(&mut self, chunk: &str) {
        if self.capped || chunk.is_empty() {
            return;
        }
        // 纯 ASCII 快路径：字素=字节数（仅 CR×LF 合并为一个簇）。
        // 逐字符 unicode_segmentation 在调试构建下对百 MB 文本需分钟级 CPU
        // （实测 100MB 单行文件每次统计烧满一核约一分钟）；字节扫描将其
        // 压到秒级以内，且不触碰文件外的额外内存。
        if chunk.is_ascii() {
            self.push_ascii(chunk);
            return;
        }
        // 上限语义：达到上限立即停止（与非 ASCII 路径逐字符检查一致）
        if self.codepoints as usize >= self.max_chars {
            self.capped = true;
            return;
        }
        for ch in chunk.chars() {
            if self.codepoints as usize >= self.max_chars {
                self.capped = true;
                return;
            }
            self.codepoints += 1;
            let whitespace = ch.is_whitespace();
            if !whitespace && self.prev_whitespace {
                self.words += 1;
            }
            self.prev_whitespace = whitespace;
        }
        if self.carry.is_empty() {
            let total = chunk.graphemes(true).count() as u64;
            self.graphemes += total;
        } else {
            let combined = format!("{}{}", self.carry, chunk);
            let total = combined.graphemes(true).count() as u64;
            self.graphemes += total.saturating_sub(self.carry_graphemes);
            self.rebuild_carry(&combined);
            return;
        }
        self.rebuild_carry(chunk);
    }

    /// ASCII 块快速计数：码点/词/字素按字节推进，携带仅保留末字符。
    /// ASCII 的字素规则只有一处特例——CR 后紧跟的 LF 合并为一个簇；
    /// 跨块的 CR|LF 由携带的末字符状态衔接。
    fn push_ascii(&mut self, chunk: &str) {
        let bytes = chunk.as_bytes();
        let remaining = self.max_chars - self.codepoints as usize;
        if remaining == 0 {
            self.capped = true;
            return;
        }
        let limit = remaining.min(bytes.len());
        let mut prev_cr = self.carry.ends_with('\r');
        let mut graphemes = 0u64;
        for &byte in &bytes[..limit] {
            let whitespace = matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r');
            if !whitespace && self.prev_whitespace {
                self.words += 1;
            }
            self.prev_whitespace = whitespace;
            if byte == b'\n' && prev_cr {
                // CRLF：LF 与前一 CR 同属一个字素簇，不重复计数
            } else {
                graphemes += 1;
            }
            prev_cr = byte == b'\r';
        }
        self.codepoints += limit as u64;
        self.graphemes += graphemes;
        if limit < bytes.len() {
            // 达到上限：与非 ASCII 路径一致，停止后续处理且不再更新携带
            self.capped = true;
            return;
        }
        let last = &chunk[chunk.len() - 1..];
        self.carry = last.to_string();
        self.carry_graphemes = 1;
    }

    /// 重建块间携带（保留末尾至多 `CARRY_BYTES` 字节，且落在字符边界）。
    fn rebuild_carry(&mut self, text: &str) {
        let mut start = text.len().saturating_sub(CARRY_BYTES);
        while start < text.len() && !text.is_char_boundary(start) {
            start += 1;
        }
        self.carry = text[start..].to_string();
        self.carry_graphemes = self.carry.graphemes(true).count() as u64;
    }

    /// 产出结果；`total_bytes` 为按口径统计的字节数。
    pub fn finish(self, total_bytes: u64) -> TextStats {
        TextStats {
            graphemes: self.graphemes,
            codepoints: self.codepoints,
            bytes: total_bytes,
            words: self.words,
            capped: self.capped,
        }
    }
}

/// 完全解码 `src` 并追加到 `dst`：按 `CoderResult` 循环扩容，绝不丢字节。
///
/// 背景：`encoding_rs` 的 `decode_to_string` 只写入目标已分配容量，容量不足时
/// 返回 `OutputFull` 且仅消费部分输入；必须扩容并从 `read` 处续喂。
/// `last=false` 时输入末尾的不完整多字节序列保留在解码器内部，由后续块补齐。
pub(crate) fn decode_stream(
    decoder: &mut encoding_rs::Decoder,
    src: &[u8],
    dst: &mut String,
    last: bool,
) {
    use encoding_rs::CoderResult;
    let mut offset = 0;
    loop {
        let (result, read, _replaced) = decoder.decode_to_string(&src[offset..], dst, last);
        offset += read;
        match result {
            CoderResult::InputEmpty => break,
            CoderResult::OutputFull => dst.reserve(4096),
        }
    }
}

/// 按编码识别文件开头的 BOM 长度（用于跳过 BOM 计数）。
pub(crate) fn bom_len(bytes: &[u8], encoding: FileEncoding) -> u64 {
    match encoding {
        FileEncoding::Utf8 if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) => 3,
        FileEncoding::Utf16Le if bytes.starts_with(&[0xFF, 0xFE]) => 2,
        FileEncoding::Utf16Be if bytes.starts_with(&[0xFE, 0xFF]) => 2,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 纯 ASCII：字素 = 码点 = 字符数，词按空白分词。
    #[test]
    fn ascii_counts() {
        let mut acc = StatsAccumulator::new(STATS_MAX_CHARS);
        acc.push_str("hello world");
        let stats = acc.finish(11);
        assert_eq!(stats.graphemes, 11);
        assert_eq!(stats.codepoints, 11);
        assert_eq!(stats.words, 2);
        assert_eq!(stats.bytes, 11);
        assert!(!stats.capped);
    }

    /// CRLF：同块内 CR+LF 合并为 1 个字素簇，单独 LF/CR 各计 1。
    #[test]
    fn ascii_crlf_within_chunk() {
        let mut acc = StatsAccumulator::new(STATS_MAX_CHARS);
        acc.push_str("a\r\nb\n\r");
        let stats = acc.finish(6);
        assert_eq!(stats.graphemes, 5, "a + CRLF + b + LF + CR");
        assert_eq!(stats.codepoints, 6);
    }

    /// CRLF 跨块：CR 在块尾、LF 在下一块头，仍合并为 1 个字素簇。
    #[test]
    fn ascii_crlf_across_chunks() {
        let mut acc = StatsAccumulator::new(STATS_MAX_CHARS);
        acc.push_str("a\r");
        acc.push_str("\nb");
        let stats = acc.finish(4);
        assert_eq!(stats.graphemes, 3, "a + CRLF + b");
        assert_eq!(stats.codepoints, 4);
    }

    /// 大块 ASCII 快路径：百万字符计数正确。
    #[test]
    fn ascii_large_chunk() {
        let mut acc = StatsAccumulator::new(STATS_MAX_CHARS);
        let text = "x".repeat(1_000_000);
        acc.push_str(&text);
        let stats = acc.finish(1_000_000);
        assert_eq!(stats.graphemes, 1_000_000);
        assert_eq!(stats.codepoints, 1_000_000);
        assert_eq!(stats.words, 1);
    }

    /// ASCII 块达到码点上限：立即 capped，不再继续计数。
    #[test]
    fn ascii_chunk_caps_at_limit() {
        let mut acc = StatsAccumulator::new(5);
        acc.push_str("abcdef");
        let stats = acc.finish(6);
        assert!(stats.capped);
        assert_eq!(stats.codepoints, 5);
        assert_eq!(stats.graphemes, 5);
    }

    /// 跨块组合序列：块边界落在基字符与组合记号之间。
    #[test]
    fn combining_mark_split_across_chunks() {
        let mut acc = StatsAccumulator::new(STATS_MAX_CHARS);
        acc.push_str("e");
        acc.push_str("\u{0301}"); // 组合尖音符
        let stats = acc.finish(3);
        assert_eq!(stats.graphemes, 1, "e + 组合记号 = 1 个字素簇");
        assert_eq!(stats.codepoints, 2);
    }

    /// ZWJ 家庭 emoji 视为 1 个字素簇（多个码点）。
    #[test]
    fn zwj_emoji_is_one_grapheme() {
        let mut acc = StatsAccumulator::new(STATS_MAX_CHARS);
        acc.push_str("👨‍👩‍👧");
        let stats = acc.finish(18);
        assert_eq!(stats.graphemes, 1);
        assert_eq!(stats.codepoints, 5);
    }

    /// 中文与全角标点：码点按字符计。
    #[test]
    fn cjk_counts() {
        let mut acc = StatsAccumulator::new(STATS_MAX_CHARS);
        acc.push_str("你好，世界");
        let stats = acc.finish(15);
        assert_eq!(stats.graphemes, 5);
        assert_eq!(stats.codepoints, 5);
        assert_eq!(stats.words, 1, "无空白连续序列按 1 个词");
    }

    /// 多块词边界：块尾空白与下一块开头非空白不应把词拆成两个。
    #[test]
    fn word_boundary_across_chunks() {
        let mut acc = StatsAccumulator::new(STATS_MAX_CHARS);
        acc.push_str("alpha ");
        acc.push_str("beta");
        let stats = acc.finish(10);
        assert_eq!(stats.words, 2);
    }

    /// 超限：达到上限即停止并标记 capped。
    #[test]
    fn cap_stops_counting() {
        let mut acc = StatsAccumulator::new(5);
        acc.push_str("abcdef");
        let stats = acc.finish(6);
        assert!(stats.capped);
        assert_eq!(stats.codepoints, 5);
        assert_eq!(stats.bytes, 6, "字节为口径值，不受上限影响");
    }

    /// BOM 长度识别。
    #[test]
    fn bom_length_detection() {
        assert_eq!(bom_len(&[0xEF, 0xBB, 0xBF, b'a'], FileEncoding::Utf8), 3);
        assert_eq!(bom_len(&[0xFF, 0xFE, 0x61, 0x00], FileEncoding::Utf16Le), 2);
        assert_eq!(bom_len(&[0xFE, 0xFF, 0x00, 0x61], FileEncoding::Utf16Be), 2);
        assert_eq!(bom_len(b"plain", FileEncoding::Utf8), 0);
        assert_eq!(bom_len(&[0xEF, 0xBB, 0xBF], FileEncoding::Gb18030), 0);
    }
}
