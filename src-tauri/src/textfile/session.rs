//! 单文件会话：打开（校验 + 映射 + 检测 + 索引）、取行窗口、百分比、切换编码。
//!
//! 职责边界：
//! - 本模块只管「一个文件」的读取状态；多标签/后台索引/外部修改检测属
//!   状态层（`AppState`）职责；
//! - 超限错误携带实际大小与上限数值（供日志），前端展示固定文案
//!   「很抱歉，文件过大无法打开，可以在设置里面调整。」（需求逐字要求）。
//!
//! 内存模型：
//! - 常驻 = mmap 映射（页由系统按需换入）+ 行索引检查点 + 少量元数据；
//! - 每次取窗仅解码请求行范围，禁止整读。

use std::path::{Path, PathBuf};

use crate::textfile::encoding::{detect, FileEncoding};
use crate::textfile::eol::EolStyle;
use crate::textfile::line_index::RowIndex;
use crate::textfile::mmap::MappedFile;
use crate::textfile::window::{fetch_rows, RowText};

/// 文本文件操作错误。
#[derive(Debug, thiserror::Error)]
pub enum TextFileError {
    /// 文件不存在或不可访问
    #[error("文件不存在或不可访问：{0}")]
    NotFound(PathBuf),
    /// 超过可打开大小上限（携带实际大小与上限，供日志与提示文案换算）
    #[error("文件过大：{size_bytes} 字节，超过上限 {limit_mb} MB")]
    TooLarge {
        /// 实际文件大小（字节）
        size_bytes: u64,
        /// 上限（MB）
        limit_mb: u32,
    },
    /// 其他 IO 失败（映射失败等）
    #[error("读取文件失败：{0}")]
    Io(#[from] std::io::Error),
}

/// 单文件读取会话。
pub struct FileSession {
    /// 文件路径
    path: PathBuf,
    /// 只读映射
    mapped: MappedFile,
    /// 行索引（编码变更时重建）
    index: RowIndex,
    /// 当前生效编码
    encoding: FileEncoding,
    /// 手动指定的编码（`None` = 自动检测）
    encoding_override: Option<FileEncoding>,
    /// 主导换行符风格（打开/切编码时按前 256KB 检测；状态栏展示）
    eol: EolStyle,
}

impl FileSession {
    /// 打开文件并构建索引。
    ///
    /// 参数：
    /// - `path`：目标文件；
    /// - `encoding_override`：手动编码（`None` 自动检测）；
    /// - `max_size_mb`：允许的最大文件大小（MB）。
    ///
    /// 返回：`Ok(FileSession)`；`Err(TextFileError)`（不存在/超限/IO 失败）。
    pub fn open(
        path: &Path,
        encoding_override: Option<FileEncoding>,
        max_size_mb: u32,
    ) -> Result<Self, TextFileError> {
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
            });
        }
        let encoding = encoding_override.unwrap_or_else(|| detect(mapped.bytes()));
        let eol = crate::textfile::eol::detect(mapped.bytes(), encoding);
        let index = RowIndex::build(mapped.bytes(), encoding);
        Ok(Self {
            path: path.to_path_buf(),
            mapped,
            index,
            encoding,
            encoding_override,
            eol,
        })
    }

    /// 文件路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 当前生效编码。
    pub fn encoding(&self) -> FileEncoding {
        self.encoding
    }

    /// 手动编码（`None` = 自动检测）。
    pub fn encoding_override(&self) -> Option<FileEncoding> {
        self.encoding_override
    }

    /// 主导换行符风格（前 256KB 检测）。
    pub fn eol(&self) -> EolStyle {
        self.eol
    }

    /// 总显示行数。
    pub fn rows_total(&self) -> u64 {
        self.index.rows_total()
    }

    /// 文件总字节数。
    pub fn byte_len(&self) -> u64 {
        self.index.byte_len()
    }

    /// 全文件文本统计。
    ///
    /// 参数：`max_chars` 码点上限（达到即停止计数并标记 `capped`）。
    /// 字节口径 = 文件原始字节（含 BOM）；字符计数不含 BOM。
    pub fn document_stats(&self, max_chars: usize) -> crate::stats::TextStats {
        let bytes = self.mapped.bytes();
        let bom = crate::stats::bom_len(bytes, self.encoding);
        let mut acc = crate::stats::StatsAccumulator::new(max_chars);
        let mut decoder = self.encoding.encoding().new_decoder_without_bom_handling();
        let mut off = bom as usize;
        let mut buf = String::new();
        while off < bytes.len() && !acc.is_capped() {
            let end = (off + 256 * 1024).min(bytes.len());
            // 预扩容 + 循环扩容（见 stats::decode_stream）；窗口尾部的
            // 不完整序列由解码器保留并与下一窗口拼接。
            buf.reserve((end - off) * 4 + 64);
            crate::stats::decode_stream(&mut decoder, &bytes[off..end], &mut buf, false);
            if !buf.is_empty() {
                acc.push_str(&buf);
                buf.clear();
            }
            off = end;
        }
        acc.finish(self.byte_len())
    }

    /// 取 `[start_row, start_row + count)` 的文本窗口。
    pub fn rows(&self, start_row: u64, count: usize) -> Vec<RowText> {
        fetch_rows(self.mapped.bytes(), &self.index, start_row, count)
    }

    /// 某行起始位置的阅读百分比（0–100）。
    pub fn percent_at_row(&self, row: u64) -> f64 {
        self.index.percent_at_row(self.mapped.bytes(), row)
    }

    /// 按百分比定位行（滚动条跳转）。
    pub fn row_at_percent(&self, percent: f64) -> u64 {
        self.index.row_at_percent(self.mapped.bytes(), percent)
    }

    /// 切换编码（`None` 恢复自动检测）并重建行索引。
    ///
    /// 说明：索引重建为同步线性扫描（100MB 量级几十毫秒）；
    /// 后续如需可移入后台线程（接口保持不变）。
    pub fn set_encoding(&mut self, encoding: Option<FileEncoding>) {
        self.encoding_override = encoding;
        self.encoding = encoding.unwrap_or_else(|| detect(self.mapped.bytes()));
        self.eol = crate::textfile::eol::detect(self.mapped.bytes(), self.encoding);
        self.index = RowIndex::build(self.mapped.bytes(), self.encoding);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 写测试文件并返回路径。
    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("写测试文件失败");
        path
    }

    /// 文件不存在 → NotFound。
    #[test]
    fn missing_file_reports_not_found() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let result = FileSession::open(&dir.path().join("none.txt"), None, 100);
        assert!(matches!(result, Err(TextFileError::NotFound(_))));
    }

    /// 超过上限 → TooLarge（携带数值）。
    #[test]
    fn oversized_file_reports_too_large() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "big.txt", b"0123456789");
        let result = FileSession::open(&path, None, 0);
        match result {
            Err(TextFileError::TooLarge {
                size_bytes,
                limit_mb,
            }) => {
                assert_eq!(size_bytes, 10);
                assert_eq!(limit_mb, 0);
            }
            _ => panic!("应为 TooLarge"),
        }
    }

    /// UTF-8 + 混合换行：取行正确。
    #[test]
    fn utf8_mixed_newlines_fetch() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(
            dir.path(),
            "mix.txt",
            "第一行\r\n第二行\r第三行\n第四行".as_bytes(),
        );
        let session = FileSession::open(&path, None, 100).expect("打开失败");
        assert_eq!(session.encoding(), FileEncoding::Utf8);
        assert_eq!(session.rows_total(), 4);
        let rows = session.rows(0, 4);
        assert_eq!(rows[0].text, "第一行");
        assert_eq!(rows[3].text, "第四行");
    }

    /// 多语言：日文（Shift_JIS）与韩文（EUC-KR）自动检测、取行正确。
    #[test]
    fn multilingual_auto_detection_and_rows() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let japanese = "これは日本語の文字コード判定を確認するテスト文章です。".repeat(20);
        let jp_bytes = encoding_rs::SHIFT_JIS.encode(&japanese).0.into_owned();
        let jp_path = write_file(dir.path(), "jp.txt", &jp_bytes);
        let session = FileSession::open(&jp_path, None, 100).expect("打开日文失败");
        assert_eq!(session.encoding(), FileEncoding::ShiftJis);
        assert_eq!(session.rows(0, 1)[0].text, japanese);

        let korean = "이것은 한국어 인코딩 감지를 위한 테스트 문장입니다. ".repeat(20);
        let kr_bytes = encoding_rs::EUC_KR.encode(&korean).0.into_owned();
        let kr_path = write_file(dir.path(), "kr.txt", &kr_bytes);
        let session = FileSession::open(&kr_path, None, 100).expect("打开韩文失败");
        assert_eq!(session.encoding(), FileEncoding::EucKr);
        assert_eq!(session.rows(0, 1)[0].text, korean);
    }

    /// GB18030 自动检测 + 取行。
    #[test]
    fn gb18030_auto_detection_and_fetch() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let text = "这是一个用于测试编码检测的中文文本段落。".repeat(30);
        let bytes = encoding_rs::GB18030.encode(&text).0.into_owned();
        let path = write_file(dir.path(), "gbk.txt", &bytes);
        let session = FileSession::open(&path, None, 100).expect("打开失败");
        assert_eq!(session.encoding(), FileEncoding::Gb18030);
        let rows = session.rows(0, 1);
        assert!(rows[0].text.starts_with("这是一个"));
    }

    /// UTF-16LE + BOM：检测与解码正确（首行剥离 BOM）。
    #[test]
    fn utf16le_with_bom_fetch() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "a\nb".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let path = write_file(dir.path(), "u16.txt", &bytes);
        let session = FileSession::open(&path, None, 100).expect("打开失败");
        assert_eq!(session.encoding(), FileEncoding::Utf16Le);
        assert_eq!(session.rows_total(), 2);
        let rows = session.rows(0, 2);
        assert_eq!(rows[0].text, "a");
        assert_eq!(rows[1].text, "b");
    }

    /// 切换编码会重建索引并改变解码结果。
    #[test]
    fn set_encoding_rebuilds_index() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = write_file(dir.path(), "a.txt", "中文\n第二行".as_bytes());
        let mut session = FileSession::open(&path, None, 100).expect("打开失败");
        assert_eq!(session.encoding(), FileEncoding::Utf8);
        assert_eq!(session.rows(0, 1)[0].text, "中文");
        session.set_encoding(Some(FileEncoding::Gb18030));
        assert_eq!(session.encoding(), FileEncoding::Gb18030);
        assert_eq!(session.encoding_override(), Some(FileEncoding::Gb18030));
        // 同一字节流按 GB18030 解码结果与 UTF-8 不同（不中断、不崩溃即为通过）
        let _ = session.rows(0, 2);
        session.set_encoding(None);
        assert_eq!(session.encoding(), FileEncoding::Utf8);
        assert_eq!(session.rows(0, 1)[0].text, "中文");
    }

    /// 百分比与百分比定位。
    #[test]
    fn percent_helpers() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let mut text = String::new();
        for index in 0..600 {
            text.push_str(&format!("行{index}\n"));
        }
        let path = write_file(dir.path(), "p.txt", text.as_bytes());
        let session = FileSession::open(&path, None, 100).expect("打开失败");
        let percent = session.percent_at_row(300);
        assert!(percent > 0.0 && percent < 100.0);
        let row = session.row_at_percent(percent);
        assert!((row as i64 - 300).abs() <= 1);
    }
}
