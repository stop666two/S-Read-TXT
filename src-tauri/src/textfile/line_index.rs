//! 稀疏行索引：显示行 ↔ 字节偏移映射。
//!
//! 行模型（显示行）：
//! - 行由换行族（`\n`、`\r\n`、`\r`）分隔；文件以换行结尾时不产生额外空行；
//!   空文件视为 1 个空行；
//! - 单行字节长度超过 [`MAX_ROW_BYTES`]（8KB）时按字符边界切分成多个显示行
//!   （无换行超长文件防护，防止主/次进程卡死）；
//! - 检查点：每 [`CHECKPOINT_INTERVAL`]（512）行记录该行的起始字节偏移；
//!   行定位 = 检查点二分 + 至多 511 行线性重扫（不做逐行存储）。
//!
//! 字符边界安全：
//! - WHATWG 传统多字节编码中 0x0A/0x0D 不会出现在多字节序列内部，
//!   以换行字节切分天然落在字符边界；
//! - 8KB 分块点按编码回退到字符边界（UTF-8 续字节回退、UTF-16 对齐与
//!   代理项保护、GB18030/Big5/Shift_JIS/EUC-KR 走前向宽度走查）；
//! - UTF-16 换行按双字节对扫描（`0A 00` / `00 0A`，含 CRLF 双单元）。

use memchr::memchr2;

use crate::textfile::encoding::FileEncoding;

/// 检查点间隔（行）
pub const CHECKPOINT_INTERVAL: u64 = 512;
/// 单显示行最大字节数（超长行分块阈值）
pub const MAX_ROW_BYTES: u64 = 8 * 1024;

/// 行索引（检查点 + 总行数 + 总字节数）。
#[derive(Debug, Clone)]
pub struct RowIndex {
    /// `checkpoints[i]` = 第 `i * CHECKPOINT_INTERVAL` 行的起始字节偏移
    checkpoints: Vec<u64>,
    /// 总显示行数（≥1）
    rows_total: u64,
    /// 文件总字节数
    byte_len: u64,
    /// 构建时的编码（行结构依赖编码）
    encoding: FileEncoding,
}

impl RowIndex {
    /// 构建索引（单次线性扫描；100MB 量级为几十毫秒，后续可移入后台线程）。
    pub fn build(bytes: &[u8], encoding: FileEncoding) -> Self {
        let byte_len = bytes.len() as u64;
        let mut checkpoints = vec![0u64];
        let mut rows_total: u64 = 1;
        if byte_len > 0 {
            let mut row: u64 = 0;
            let mut start: u64 = 0;
            loop {
                let (_, next) = scan_row(bytes, encoding, start);
                match next {
                    Some(next_start) => {
                        row += 1;
                        if row % CHECKPOINT_INTERVAL == 0 {
                            checkpoints.push(next_start);
                        }
                        start = next_start;
                    }
                    None => break,
                }
            }
            rows_total = row + 1;
        }
        Self {
            checkpoints,
            rows_total,
            byte_len,
            encoding,
        }
    }

    /// 总显示行数（≥1）。
    pub fn rows_total(&self) -> u64 {
        self.rows_total
    }

    /// 文件总字节数。
    pub fn byte_len(&self) -> u64 {
        self.byte_len
    }

    /// 构建时的编码。
    pub fn encoding(&self) -> FileEncoding {
        self.encoding
    }

    /// 第 `row` 行的起始字节偏移（`row` 须小于 `rows_total`）。
    pub fn row_start(&self, bytes: &[u8], row: u64) -> u64 {
        debug_assert!(row < self.rows_total, "行号越界");
        let checkpoint_index = (row / CHECKPOINT_INTERVAL) as usize;
        let mut current_row = checkpoint_index as u64 * CHECKPOINT_INTERVAL;
        let mut start = self.checkpoints[checkpoint_index];
        while current_row < row {
            let (_, next) = scan_row(bytes, self.encoding, start);
            match next {
                Some(next_start) => {
                    start = next_start;
                    current_row += 1;
                }
                // 防御性退出：正常索引下不可达（row 已校验）
                None => break,
            }
        }
        start
    }

    /// 取 `[start_row, start_row + count)` 的 `(行号, 起始字节, 结束字节)` 列表。
    pub fn row_slices(&self, bytes: &[u8], start_row: u64, count: usize) -> Vec<(u64, u64, u64)> {
        if start_row >= self.rows_total || count == 0 {
            return Vec::new();
        }
        let mut slices = Vec::with_capacity(count.min(1024));
        let mut row = start_row;
        let mut start = self.row_start(bytes, row);
        while slices.len() < count && row < self.rows_total {
            let (end, next) = scan_row(bytes, self.encoding, start);
            slices.push((row, start, end));
            match next {
                Some(next_start) => {
                    row += 1;
                    start = next_start;
                }
                None => break,
            }
        }
        slices
    }

    /// 某行起始位置的阅读百分比（0–100；空文件为 0）。
    pub fn percent_at_row(&self, bytes: &[u8], row: u64) -> f64 {
        if self.byte_len == 0 {
            return 0.0;
        }
        let start = self.row_start(bytes, row);
        (start as f64) * 100.0 / (self.byte_len as f64)
    }

    /// 按百分比查找目标行（滚动条跳转；越界收敛到首/尾行）。
    pub fn row_at_percent(&self, bytes: &[u8], percent: f64) -> u64 {
        let clamped = percent.clamp(0.0, 100.0);
        let target = (self.byte_len as f64 * clamped / 100.0) as u64;
        let mut low = 0usize;
        let mut high = self.checkpoints.len();
        while low < high {
            let mid = (low + high) / 2;
            if self.checkpoints[mid] <= target {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        let checkpoint_index = low.saturating_sub(1);
        let mut row = checkpoint_index as u64 * CHECKPOINT_INTERVAL;
        let mut start = self.checkpoints[checkpoint_index];
        while row + 1 < self.rows_total {
            let (_, next) = scan_row(bytes, self.encoding, start);
            match next {
                Some(next_start) if next_start <= target => {
                    row += 1;
                    start = next_start;
                }
                _ => break,
            }
        }
        row
    }
}

/// 从 `start` 扫描一个显示行：返回 `(行结束字节, 下一行起点或 None)`。
///
/// 语义：
/// - `None` 表示已到文件末尾（结尾换行不生成新行）；
/// - 超过 [`MAX_ROW_BYTES`] 的行在字符边界处切块，`next == Some(end)`；
/// - `start` 必须位于字符边界（行起点由本模块保证）。
pub(crate) fn scan_row(bytes: &[u8], encoding: FileEncoding, start: u64) -> (u64, Option<u64>) {
    let len = bytes.len() as u64;
    debug_assert!(start <= len, "行起点越界");
    // 有界扫描：换行只影响「本行是否为超长块」的判定，因此仅需在
    // [start, start + MAX_ROW_BYTES + 2) 窗口内查找。无界查找会让无换行
    // 超长行退化为 O(n²)（每个 8KB 块都扫到文件尾，100MB 单行可达数百 GB 扫描量）。
    let window_end = (start + MAX_ROW_BYTES + 2).min(len);
    match find_newline_bounded(bytes, encoding, start, window_end) {
        Some((newline_start, newline_end)) => {
            if newline_start - start > MAX_ROW_BYTES {
                let end = snap_row_boundary(bytes, encoding, start, start + MAX_ROW_BYTES);
                (end, Some(end))
            } else if newline_end >= len {
                (newline_start, None)
            } else {
                (newline_start, Some(newline_end))
            }
        }
        None => {
            if len - start > MAX_ROW_BYTES {
                let end = snap_row_boundary(bytes, encoding, start, start + MAX_ROW_BYTES);
                (end, Some(end))
            } else {
                (len, None)
            }
        }
    }
}

/// 查找 `from` 之后最近的换行，返回 `(换行起始, 下一行起点)`。
///
/// 可见性：`pub(crate)` —— 编辑引擎（`textfile::editing`）复用同一换行语义，
/// 保证读/编辑两条路径的换行行为一致。
pub(crate) fn find_newline(bytes: &[u8], encoding: FileEncoding, from: u64) -> Option<(u64, u64)> {
    find_newline_bounded(bytes, encoding, from, bytes.len() as u64)
}

/// 受限窗口版：仅在 `[from, to)` 范围内查找换行**起点**（CRLF 的第二字节允许越窗读取）。
fn find_newline_bounded(
    bytes: &[u8],
    encoding: FileEncoding,
    from: u64,
    to: u64,
) -> Option<(u64, u64)> {
    if from >= to {
        return None;
    }
    match encoding {
        FileEncoding::Utf16Le => find_newline_utf16(bytes, from, true, to),
        FileEncoding::Utf16Be => find_newline_utf16(bytes, from, false, to),
        _ => find_newline_bytes(bytes, from, to),
    }
}

/// 单字节/传统多字节编码：`memchr2` 扫 `\n`/`\r`（`\r\n` 视为一个换行）。
fn find_newline_bytes(bytes: &[u8], from: u64, to: u64) -> Option<(u64, u64)> {
    let offset = memchr2(b'\n', b'\r', &bytes[from as usize..to as usize])?;
    let position = from as usize + offset;
    if bytes[position] == b'\r' && bytes.get(position + 1) == Some(&b'\n') {
        Some((position as u64, position as u64 + 2))
    } else {
        Some((position as u64, position as u64 + 1))
    }
}

/// UTF-16：按双字节对扫描 `0x000A`/`0x000D`（`0x000D 0x000A` 视为一个换行）。
fn find_newline_utf16(bytes: &[u8], from: u64, little_endian: bool, to: u64) -> Option<(u64, u64)> {
    debug_assert_eq!(from % 2, 0, "UTF-16 行起点必须双字节对齐");
    let limit = (to as usize).min(bytes.len());
    let mut position = from as usize;
    while position + 1 < limit {
        let unit = read_utf16_unit(bytes, position, little_endian);
        if unit == 0x000A || unit == 0x000D {
            let mut end = position as u64 + 2;
            if unit == 0x000D
                && position + 3 < bytes.len()
                && read_utf16_unit(bytes, position + 2, little_endian) == 0x000A
            {
                end += 2;
            }
            return Some((position as u64, end));
        }
        position += 2;
    }
    None
}

/// 读取指定位置的 UTF-16 代码单元（调用方保证 `position + 1 < len`）。
///
/// 可见性：`pub(crate)`（编辑引擎复用）。
pub(crate) fn read_utf16_unit(bytes: &[u8], position: usize, little_endian: bool) -> u16 {
    let pair = [bytes[position], bytes[position + 1]];
    if little_endian {
        u16::from_le_bytes(pair)
    } else {
        u16::from_be_bytes(pair)
    }
}

/// 将 8KB 分块候选点回退到字符边界（保证 `start < 返回值 ≤ candidate`）。
///
/// 可见性：`pub(crate)`（编辑引擎的显示分段复用同一对齐规则）。
pub(crate) fn snap_row_boundary(
    bytes: &[u8],
    encoding: FileEncoding,
    start: u64,
    candidate: u64,
) -> u64 {
    let len = bytes.len() as u64;
    let candidate = candidate.min(len);
    match encoding {
        FileEncoding::Utf8 => {
            // 回退到字符边界：position 处是续字节（10xxxxxx）时继续回退。
            // 注意：判定必须看 bytes[position] 本身，否则会落在字符中间（历史缺陷）。
            let mut position = candidate.min(len);
            while position > start
                && position < len
                && (bytes[position as usize] & 0b1100_0000) == 0b1000_0000
            {
                position -= 1;
            }
            position.max(start + 1)
        }
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
            let little_endian = encoding == FileEncoding::Utf16Le;
            let mut position = candidate & !1;
            // 不在高代理项（D800–DBFF）之后切分
            if position >= start + 2 {
                let unit = read_utf16_unit(bytes, (position - 2) as usize, little_endian);
                if (0xD800..=0xDBFF).contains(&unit) {
                    position -= 2;
                }
            }
            position.max(start + 1)
        }
        _ => snap_legacy(bytes, encoding, start, candidate),
    }
}

/// 传统多字节编码：从已知字符边界 `start` 前向走查，取 ≤ `candidate` 的最大边界。
fn snap_legacy(bytes: &[u8], encoding: FileEncoding, start: u64, candidate: u64) -> u64 {
    let mut position = start;
    while position < candidate {
        let width = legacy_char_width(bytes, encoding, position);
        if position + width > candidate {
            break;
        }
        position += width;
    }
    position
}

/// 传统多字节编码在 `position` 处的字符宽度（1–4；损坏字节按 1 处理以推进）。
fn legacy_char_width(bytes: &[u8], encoding: FileEncoding, position: u64) -> u64 {
    let remaining = bytes.len() as u64 - position;
    let first = bytes[position as usize];
    let width: u64 = match encoding {
        FileEncoding::Gb18030 => {
            if (0x81..=0xFE).contains(&first) && remaining >= 2 {
                let second = bytes[position as usize + 1];
                if (0x30..=0x39).contains(&second) && remaining >= 4 {
                    4
                } else {
                    2
                }
            } else {
                1
            }
        }
        FileEncoding::Big5 | FileEncoding::EucKr => {
            if (0x81..=0xFE).contains(&first) && remaining >= 2 {
                2
            } else {
                1
            }
        }
        FileEncoding::ShiftJis => {
            let is_lead = (0x81..=0x9F).contains(&first) || (0xE0..=0xFC).contains(&first);
            if is_lead && remaining >= 2 {
                2
            } else {
                1
            }
        }
        _ => 1,
    };
    width.min(remaining)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_of(bytes: &[u8]) -> RowIndex {
        RowIndex::build(bytes, FileEncoding::Utf8)
    }

    /// 16MB 单行：构建必须线性完成（防 O(n²) 回归；旧实现每个 8KB 块都扫到文件尾）。
    #[test]
    fn huge_single_line_build_is_linear() {
        let bytes = vec![b'a'; 16 * 1024 * 1024];
        let started = std::time::Instant::now();
        let index = RowIndex::build(&bytes, FileEncoding::Utf8);
        let elapsed = started.elapsed();
        assert_eq!(index.rows_total(), 2048);
        assert_eq!(index.row_start(&bytes, 2047), 2047 * 8192);
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "16MB 单行构建耗时 {elapsed:?}，疑似 O(n²) 回归"
        );
    }

    /// 换行恰好落在 8KB 上限处：保持「行模式」（换行点不会被窗口边界误判为超长块）。
    #[test]
    fn newline_at_cap_boundary_keeps_row_semantics() {
        let mut bytes = vec![b'a'; 8192];
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(b"tail");
        let index = index_of(&bytes);
        assert_eq!(index.rows_total(), 2);
        assert_eq!(index.row_start(&bytes, 1), 8194);
    }

    /// 换行在上限之后 1 字节：先按超长块切分，下一轮仍能正确识别该换行。
    #[test]
    fn newline_beyond_cap_is_chunked_then_found() {
        let mut bytes = vec![b'a'; 8193];
        bytes.extend_from_slice(b"\nrest");
        let index = index_of(&bytes);
        assert_eq!(index.rows_total(), 3);
        assert_eq!(index.row_start(&bytes, 1), 8192);
        assert_eq!(index.row_start(&bytes, 2), 8194);
    }

    /// 行文本（测试辅助，UTF-8 宽容解码）。
    fn row_texts(bytes: &[u8], index: &RowIndex) -> Vec<String> {
        index
            .row_slices(bytes, 0, index.rows_total() as usize)
            .into_iter()
            .map(|(_, start, end)| {
                String::from_utf8_lossy(&bytes[start as usize..end as usize]).into_owned()
            })
            .collect()
    }

    /// 换行族与空文件的行数语义。
    #[test]
    fn row_counts_cover_newline_families() {
        assert_eq!(index_of(b"").rows_total(), 1);
        assert_eq!(index_of(b"a").rows_total(), 1);
        assert_eq!(index_of(b"a\n").rows_total(), 1);
        assert_eq!(index_of(b"\n").rows_total(), 1);
        assert_eq!(index_of(b"a\nb").rows_total(), 2);
        assert_eq!(index_of(b"a\r\nb").rows_total(), 2);
        assert_eq!(index_of(b"a\rb").rows_total(), 2);
        assert_eq!(index_of(b"\n\n").rows_total(), 2);
    }

    /// 行文本不包含任何换行字符。
    #[test]
    fn row_texts_strip_all_newline_variants() {
        let bytes = b"one\r\ntwo\rthree\nfour";
        let index = index_of(bytes);
        assert_eq!(
            row_texts(bytes, &index),
            vec!["one", "two", "three", "four"]
        );
    }

    /// 检查点保证直接行定位正确。
    #[test]
    fn checkpoints_allow_direct_row_lookup() {
        let mut bytes = Vec::new();
        for _ in 0..1200 {
            bytes.extend_from_slice(b"x\n");
        }
        let index = index_of(&bytes);
        assert_eq!(index.rows_total(), 1200);
        assert_eq!(index.row_start(&bytes, 0), 0);
        assert_eq!(index.row_start(&bytes, 512), 1024);
        assert_eq!(index.row_start(&bytes, 1000), 2000);
        assert_eq!(index.row_start(&bytes, 1199), 2398);
    }

    /// 超长行按 8KB 切块且逐块无重叠、拼接无损。
    #[test]
    fn long_line_is_chunked_without_loss() {
        let mut bytes = vec![b'x'; 20_000];
        bytes.push(b'\n');
        let index = index_of(&bytes);
        assert_eq!(index.rows_total(), 3);
        let slices = index.row_slices(&bytes, 0, 3);
        assert_eq!(slices[0], (0, 0, 8192));
        assert_eq!(slices[1], (1, 8192, 16384));
        assert_eq!(slices[2], (2, 16384, 20000));
        let joined: Vec<u8> = slices
            .iter()
            .flat_map(|(_, start, end)| bytes[*start as usize..*end as usize].to_vec())
            .collect();
        assert_eq!(joined.len(), 20_000);
        assert!(joined.iter().all(|byte| *byte == b'x'));
    }

    /// CRLF 恰好落在块阈值上时不产生空块。
    #[test]
    fn crlf_at_chunk_limit_stays_single_row() {
        let mut bytes = vec![b'x'; 8191];
        bytes.extend_from_slice(b"\r\n");
        let index = index_of(&bytes);
        assert_eq!(index.rows_total(), 1);
        assert_eq!(index.row_slices(&bytes, 0, 1)[0], (0, 0, 8191));
    }

    /// UTF-16 换行按代码单元扫描。
    #[test]
    fn utf16_newlines_are_scanned_as_units() {
        let mut bytes = Vec::new();
        for unit in "a\nb".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let index = RowIndex::build(&bytes, FileEncoding::Utf16Le);
        assert_eq!(index.rows_total(), 2);
        assert_eq!(index.row_start(&bytes, 1), 4);
    }

    /// 百分比与按百分比定位互为逆运算（一致性）。
    #[test]
    fn percent_and_row_lookup_roundtrip() {
        let mut bytes = Vec::new();
        for _ in 0..600 {
            bytes.extend_from_slice(b"x\n");
        }
        let index = index_of(&bytes);
        let percent = index.percent_at_row(&bytes, 300);
        assert!((percent - 50.0).abs() < 1e-9, "percent={percent}");
        assert_eq!(index.row_at_percent(&bytes, 50.0), 300);
        assert_eq!(index.row_at_percent(&bytes, 100.0), 599);
        assert_eq!(index.row_at_percent(&bytes, 0.0), 0);
    }

    /// GB18030 分块点不劈开双字节字符。
    #[test]
    fn gb18030_chunk_boundary_snaps_to_char_boundary() {
        let mut bytes = vec![b'x'; 8191];
        bytes.extend_from_slice(&[0x81, 0x40, 0x81, 0x40]);
        bytes.push(b'\n');
        let index = RowIndex::build(&bytes, FileEncoding::Gb18030);
        assert_eq!(index.rows_total(), 2);
        let slices = index.row_slices(&bytes, 0, 2);
        assert_eq!(slices[0], (0, 0, 8191));
        let first = &bytes[slices[0].1 as usize..slices[0].2 as usize];
        assert!(first.iter().all(|byte| *byte == b'x'));
    }
}
