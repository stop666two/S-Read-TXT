//! 片段（Piece）模型与换行单元统计。
//!
//! 片表模型（设计 §5.1）：
//! - `Piece` = 对一个不可变字节源的引用：`Original`（mmap 原文）或 `Added`（新增缓冲）；
//! - 删除不复制字节：原文引用继续有效，新增缓冲只增不删（由撤销历史上限约束增长）。
//!
//! 换行单元语义（与 `line_index` 一致）：
//! - 单元 = `\n` | `\r\n` | `\r`（UTF-16 为对应的双字节对，含 CRLF 双单元）；
//! - 单元的计数依赖"紧邻左侧是否为 `\r`"：跨片段拆分的 `\r`|`\n` 只计一次。

use crate::textfile::encoding::FileEncoding;
use crate::textfile::line_index::{find_newline, read_utf16_unit};

/// 片段来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceSource {
    /// 原文（只读 mmap）
    Original,
    /// 新增缓冲（UTF-8）
    Added,
}

/// 文档片段：`[off, off + len)` 的字节范围引用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    /// 字节源
    pub source: PieceSource,
    /// 源内起始偏移
    pub off: u64,
    /// 长度（字节；UTF-16 下为偶数）
    pub len: u64,
}

impl Piece {
    /// 源内结束偏移（不含）。
    pub fn end(&self) -> u64 {
        self.off + self.len
    }

    /// 是否为零长度（编辑引擎不产生零长度片段）。
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// 片段换行元数据；`units` 的计数依赖其左邻居的 `ends_with_cr` 上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PieceMeta {
    /// 换行单元数（计入跨片 CRLF 抑制规则）
    pub units: u64,
    /// 片段是否以 `\r`（UTF-16：0x000D）结尾
    pub ends_with_cr: bool,
}

/// 判断 `[start, end)` 是否以 `\r`（UTF-16：0x000D）结尾；空范围返回 `false`。
pub fn ends_with_cr(bytes: &[u8], encoding: FileEncoding, start: u64, end: u64) -> bool {
    if end <= start {
        return false;
    }
    match encoding {
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
            if end - start < 2 {
                return false;
            }
            read_utf16_unit(bytes, (end - 2) as usize, encoding == FileEncoding::Utf16Le) == 0x000D
        }
        _ => bytes[(end - 1) as usize] == b'\r',
    }
}

/// 判断 `[start, end)` 是否以 `\n`（UTF-16：0x000A）开头；空范围返回 `false`。
pub fn starts_with_lf(bytes: &[u8], encoding: FileEncoding, start: u64, end: u64) -> bool {
    if end <= start {
        return false;
    }
    match encoding {
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
            if end - start < 2 {
                return false;
            }
            read_utf16_unit(bytes, start as usize, encoding == FileEncoding::Utf16Le) == 0x000A
        }
        _ => bytes[start as usize] == b'\n',
    }
}

/// 换行符（`\n`/`\r`）在给定编码下的字节宽度：UTF-16 为 2，其余为 1。
pub fn newline_width(encoding: FileEncoding) -> u64 {
    match encoding {
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => 2,
        _ => 1,
    }
}

/// 统计 `[start, end)` 内的换行单元数。
///
/// 参数：
/// - `prev_cr`：紧邻左侧字节/单元是否为 `\r`（用于跨片段 CRLF 拆分抑制：
///   当范围以 `\n` 开头且左邻为 `\r` 时，该 `\n` 不计入本范围）；
/// - `encoding`：源文本编码（`Added` 片段传 `FileEncoding::Utf8`）。
///
/// 返回：`PieceMeta`（`ends_with_cr` 直接按字节判定，`\r` 位于范围末尾时为 `true`，
/// 即使其配对的 `\n` 在范围之外——这正是跨片片段所需的语义）。
pub fn count_units(
    bytes: &[u8],
    encoding: FileEncoding,
    start: u64,
    end: u64,
    prev_cr: bool,
) -> PieceMeta {
    let mut pos = start;
    let mut units = 0u64;
    if prev_cr && starts_with_lf(bytes, encoding, start, end) {
        // 前导 `\n` 属于左侧 `\r\n`，跳过且不计
        pos += newline_width(encoding);
    }
    while pos < end {
        match find_newline(bytes, encoding, pos) {
            Some((newline_start, newline_end)) if newline_start < end => {
                units += 1;
                // `\r` 在范围内且配对的 `\n` 在范围外时，收敛到 `end` 结束扫描
                pos = newline_end.min(end);
            }
            _ => break,
        }
    }
    PieceMeta {
        units,
        ends_with_cr: ends_with_cr(bytes, encoding, start, end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 换行族基本计数。
    #[test]
    fn counts_newline_families() {
        assert_eq!(
            count_units(b"abc", FileEncoding::Utf8, 0, 3, false).units,
            0
        );
        assert_eq!(
            count_units(b"a\nb", FileEncoding::Utf8, 0, 3, false).units,
            1
        );
        assert_eq!(
            count_units(b"a\rb", FileEncoding::Utf8, 0, 3, false).units,
            1
        );
        assert_eq!(
            count_units(b"a\r\nb", FileEncoding::Utf8, 0, 4, false).units,
            1
        );
        assert_eq!(
            count_units(b"\n\n", FileEncoding::Utf8, 0, 2, false).units,
            2
        );
        assert_eq!(
            count_units(b"a\n", FileEncoding::Utf8, 0, 2, false).units,
            1
        );
    }

    /// 跨片 CRLF：左片以 `\r` 结尾、右片以 `\n` 开头时只计一次。
    #[test]
    fn cross_piece_crlf_counts_once() {
        let left = count_units(b"a\r", FileEncoding::Utf8, 0, 2, false);
        assert_eq!(left.units, 1);
        assert!(left.ends_with_cr);
        let right = count_units(b"\nb", FileEncoding::Utf8, 0, 2, true);
        assert_eq!(right.units, 0);
        assert!(!right.ends_with_cr);
    }

    /// 左片以 `\r` 结尾但右片不以 `\n` 开头：不抑制。
    #[test]
    fn prev_cr_without_lf_not_suppressed() {
        let right = count_units(b"b\nc", FileEncoding::Utf8, 0, 3, true);
        assert_eq!(right.units, 1);
    }

    /// UTF-16 换行按双字节对计数，跨片 CRLF（0D 00 | 0A 00）同样抑制。
    #[test]
    fn utf16_units_and_cross_piece() {
        // "a\nb" LE: 61 00 0A 00 62 00
        let bytes = [0x61, 0x00, 0x0A, 0x00, 0x62, 0x00];
        assert_eq!(
            count_units(&bytes, FileEncoding::Utf16Le, 0, 6, false).units,
            1
        );
        // 左片 = 0D 00，右片 = 0A 00 61 00
        let left = count_units(&[0x0D, 0x00], FileEncoding::Utf16Le, 0, 2, false);
        assert_eq!(left.units, 1);
        assert!(left.ends_with_cr);
        let right = count_units(&[0x0A, 0x00, 0x61, 0x00], FileEncoding::Utf16Le, 0, 4, true);
        assert_eq!(right.units, 0);
    }

    /// `ends_with_cr` 与 `starts_with_lf` 的边界。
    #[test]
    fn boundary_helpers() {
        assert!(!ends_with_cr(b"abc", FileEncoding::Utf8, 0, 3));
        assert!(ends_with_cr(b"ab\r", FileEncoding::Utf8, 0, 3));
        assert!(!ends_with_cr(b"", FileEncoding::Utf8, 0, 0));
        assert!(starts_with_lf(b"\nx", FileEncoding::Utf8, 0, 2));
        assert!(!starts_with_lf(b"\rx", FileEncoding::Utf8, 0, 2));
        // UTF-16 单字节残片不算
        assert!(!ends_with_cr(&[0x0D, 0x00], FileEncoding::Utf16Le, 1, 2));
    }

    /// 范围末尾的 `\r`（配对 `\n` 在范围外）：计 1 单元且 ends_with_cr。
    #[test]
    fn trailing_cr_with_lf_outside_range() {
        let meta = count_units(b"a\r\nb", FileEncoding::Utf8, 0, 2, false);
        assert_eq!(meta.units, 1);
        assert!(meta.ends_with_cr);
    }
}
