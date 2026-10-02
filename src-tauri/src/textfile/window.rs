//! 文本窗口：按显示行取回解码后的字符串（仅取可视窗口，禁止整读文件）。
//!
//! 调用约定（阶段 2 接线时遵守）：
//! - 每次取行数建议 ≤ 512（虚拟滚动可视区 + 预取）；
//! - 返回顺序与行号一致；越界起点返回空列表；
//! - 行文本已去除换行符；首行含 BOM 时由编码层自动剥离。

use crate::textfile::encoding::decode_range;
use crate::textfile::line_index::RowIndex;

/// 单行文本（显示行号 + 解码后的 UTF-8 内容）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowText {
    /// 显示行号（从 0 开始）
    pub row: u64,
    /// 解码后的行文本（不含换行符）
    pub text: String,
}

/// 取 `[start_row, start_row + count)` 的文本窗口。
pub fn fetch_rows(bytes: &[u8], index: &RowIndex, start_row: u64, count: usize) -> Vec<RowText> {
    index
        .row_slices(bytes, start_row, count)
        .into_iter()
        .map(|(row, start, end)| RowText {
            row,
            text: decode_range(&bytes[start as usize..end as usize], index.encoding()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textfile::encoding::FileEncoding;

    /// 基础取窗：行文本正确、顺序正确、越界安全。
    #[test]
    fn fetch_rows_basic() {
        let bytes = b"one\ntwo\nthree";
        let index = RowIndex::build(bytes, FileEncoding::Utf8);
        let rows = fetch_rows(bytes, &index, 0, 10);
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[0],
            RowText {
                row: 0,
                text: "one".into()
            }
        );
        assert_eq!(
            rows[2],
            RowText {
                row: 2,
                text: "three".into()
            }
        );
        assert!(fetch_rows(bytes, &index, 3, 5).is_empty());
    }

    /// 从中间行开始的窗口只包含请求范围。
    #[test]
    fn fetch_rows_from_offset() {
        let bytes = b"a\nb\nc\nd";
        let index = RowIndex::build(bytes, FileEncoding::Utf8);
        let rows = fetch_rows(bytes, &index, 1, 2);
        assert_eq!(
            rows,
            vec![
                RowText {
                    row: 1,
                    text: "b".into()
                },
                RowText {
                    row: 2,
                    text: "c".into()
                },
            ]
        );
    }

    /// 首行含 UTF-8 BOM 时解码自动剥离。
    #[test]
    fn first_row_strips_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("中文\nb".as_bytes());
        let index = RowIndex::build(&bytes, FileEncoding::Utf8);
        let rows = fetch_rows(bytes.as_slice(), &index, 0, 2);
        assert_eq!(rows[0].text, "中文");
        assert_eq!(rows[1].text, "b");
    }

    /// GB18030 行解码正确。
    #[test]
    fn gb18030_rows_decode() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0xD6, 0xD0, 0xCE, 0xC4]); // 中文
        bytes.push(b'\n');
        bytes.extend_from_slice(&[0xB2, 0xE2, 0xCA, 0xD4]); // 测试
        let index = RowIndex::build(&bytes, FileEncoding::Gb18030);
        let rows = fetch_rows(bytes.as_slice(), &index, 0, 2);
        assert_eq!(rows[0].text, "中文");
        assert_eq!(rows[1].text, "测试");
    }

    /// 超长行分块取窗后可无损拼接。
    #[test]
    fn long_line_chunks_join_losslessly() {
        let mut bytes = vec![b'x'; 20_000];
        bytes.push(b'\n');
        let index = RowIndex::build(&bytes, FileEncoding::Utf8);
        let rows = fetch_rows(bytes.as_slice(), &index, 0, index.rows_total() as usize);
        let joined: String = rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(joined.len(), 20_000);
        assert_eq!(rows.len(), 3);
    }
}
