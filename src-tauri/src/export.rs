//! 文档导出与打印（P3-2）。
//!
//! 格式由目标扩展名决定：`txt`/`md`（保留原文换行）、`csv`（逐行单列、RFC 4180 引号）、
//! `json`（行字符串数组）、`html`（转义 + `<pre>`，UTF-8）。
//!
//! 全部流式写出并带输出上限（[`EXPORT_MAX_BYTES`]），避免大文件占用内存；
//! 打印走 [`print_html`]（HTML 上限 [`PRINT_MAX_BYTES`]，经 data URL 交给打印窗口）。

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::textfile::eol::EolStyle;
use crate::textfile::source::DocumentSource;

/// 导出输出上限（64MB）：超过报错（调用方可按需清理半成品文件）。
pub const EXPORT_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// 打印 HTML 上限（1MB）：经 data URL 传给打印窗口（Chromium URL 长度上限约 2MB，base64 后放大 4/3）。
pub const PRINT_MAX_BYTES: u64 = 1024 * 1024;
/// 每批抓取行数。
const ROW_BATCH: u64 = 4096;

/// 导出格式（由扩展名推断）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// 纯文本（保留原文换行风格）
    Txt,
    /// Markdown（纯文本直通）
    Markdown,
    /// 逐行单列 CSV
    Csv,
    /// 行字符串数组 JSON
    Json,
    /// 转义 HTML（`<pre>`）
    Html,
}

impl ExportFormat {
    /// 按路径扩展名解析格式；未知扩展名返回 `None`。
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "txt" => Some(Self::Txt),
            "md" | "markdown" => Some(Self::Markdown),
            "csv" => Some(Self::Csv),
            "json" => Some(Self::Json),
            "html" | "htm" => Some(Self::Html),
            _ => None,
        }
    }
}

/// 导出错误。
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// 目标扩展名不受支持
    #[error("不支持的导出格式（可选：txt / md / csv / json / html）")]
    Unsupported,
    /// 输出超过上限
    #[error("导出内容过大（超过 {limit_mb} MB）")]
    TooLarge {
        /// 上限（MB）
        limit_mb: u64,
    },
    /// 底层 IO 错误
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// 输出上限计数写入包装。
struct CappedWriter<W: Write> {
    inner: W,
    written: u64,
    cap: u64,
}

impl<W: Write> CappedWriter<W> {
    fn new(inner: W, cap: u64) -> Self {
        Self {
            inner,
            written: 0,
            cap,
        }
    }

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), ExportError> {
        if self.written + bytes.len() as u64 > self.cap {
            return Err(ExportError::TooLarge {
                limit_mb: self.cap / (1024 * 1024),
            });
        }
        self.inner.write_all(bytes)?;
        self.written += bytes.len() as u64;
        Ok(())
    }

    fn finish(mut self) -> Result<u64, ExportError> {
        self.inner.flush()?;
        Ok(self.written)
    }
}

fn newline_of(eol: EolStyle) -> &'static str {
    match eol {
        EolStyle::CrLf => "\r\n",
        EolStyle::Cr => "\r",
        _ => "\n",
    }
}

/// 追加一行 CSV 字段（必要时加引号，`"` 双写）。
fn push_csv_field(out: &mut String, field: &str) {
    let needs_quote = field.contains([',', '"', '\n', '\r']);
    if needs_quote {
        out.push('"');
        for ch in field.chars() {
            if ch == '"' {
                out.push('"');
            }
            out.push(ch);
        }
        out.push('"');
    } else {
        out.push_str(field);
    }
}

/// HTML 转义（`& < > " '`）。
fn push_html_escaped(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
}

/// 流式写出文档（`cap` 可注入便于测试）。
fn write_document(
    source: &dyn DocumentSource,
    format: ExportFormat,
    writer: &mut CappedWriter<impl Write>,
) -> Result<(), ExportError> {
    let total = source.rows_total();
    let nl = newline_of(source.eol());
    let title = source
        .path()
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".to_string());

    if format == ExportFormat::Html {
        let mut head = String::new();
        head.push_str("<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><title>");
        push_html_escaped(&mut head, &title);
        head.push_str("</title><style>body{font-family:system-ui,'Microsoft YaHei',sans-serif;margin:24px;}pre{white-space:pre-wrap;word-break:break-word;font-family:inherit;font-size:14px;line-height:1.7;}</style></head><body><pre>");
        writer.write_chunk(head.as_bytes())?;
    } else if format == ExportFormat::Json {
        writer.write_chunk("[\n".as_bytes())?;
    }

    let mut row = 0u64;
    while row < total {
        let rows = source.fetch_rows(row, ROW_BATCH.min(total - row) as usize);
        if rows.is_empty() {
            break;
        }
        for item in &rows {
            match format {
                ExportFormat::Txt | ExportFormat::Markdown => {
                    writer.write_chunk(item.text.as_bytes())?;
                }
                ExportFormat::Csv => {
                    let mut line = String::new();
                    push_csv_field(&mut line, &item.text);
                    writer.write_chunk(line.as_bytes())?;
                }
                ExportFormat::Json => {
                    let encoded =
                        serde_json::to_string(&item.text).unwrap_or_else(|_| "\"\"".into());
                    writer.write_chunk(format!("  {encoded}").as_bytes())?;
                }
                ExportFormat::Html => {
                    let mut line = String::new();
                    push_html_escaped(&mut line, &item.text);
                    writer.write_chunk(line.as_bytes())?;
                }
            }
            if item.row + 1 < total {
                if format == ExportFormat::Json {
                    writer.write_chunk(b",")?;
                }
                writer.write_chunk(nl.as_bytes())?;
            }
        }
        row += rows.len() as u64;
    }

    match format {
        ExportFormat::Json => writer.write_chunk("\n]\n".as_bytes())?,
        ExportFormat::Html => writer.write_chunk(b"</pre></body></html>\n")?,
        _ => {}
    }
    Ok(())
}

/// 导出文档到 `target`；返回写入字节数。
pub fn export_document(
    source: &dyn DocumentSource,
    target: &Path,
    format: ExportFormat,
) -> Result<u64, ExportError> {
    export_document_capped(source, target, format, EXPORT_MAX_BYTES)
}

/// 带上限的导出（测试注入用）。
pub fn export_document_capped(
    source: &dyn DocumentSource,
    target: &Path,
    format: ExportFormat,
    cap: u64,
) -> Result<u64, ExportError> {
    let file = File::create(target)?;
    let mut writer = CappedWriter::new(BufWriter::new(file), cap);
    write_document(source, format, &mut writer)?;
    writer.finish()
}

/// 生成打印用 HTML（自动调起打印对话框；UTF-8 字符串）。
///
/// 错误：内容超过 [`PRINT_MAX_BYTES`] → `TooLarge`（建议改用导出）。
pub fn print_html(source: &dyn DocumentSource, auto_print: bool) -> Result<String, ExportError> {
    let mut buffer = Vec::new();
    let mut writer = CappedWriter::new(&mut buffer, PRINT_MAX_BYTES);
    let total = source.rows_total();
    let nl = newline_of(source.eol());
    let title = source
        .path()
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".to_string());

    let mut head = String::new();
    head.push_str("<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><title>");
    push_html_escaped(&mut head, &title);
    head.push_str(" - S-Read-TXT</title><style>body{margin:20mm 16mm;}pre{white-space:pre-wrap;word-break:break-word;font-family:system-ui,'Microsoft YaHei',sans-serif;font-size:12pt;line-height:1.8;}</style></head><body><pre>");
    writer.write_chunk(head.as_bytes())?;

    let mut row = 0u64;
    while row < total {
        let rows = source.fetch_rows(row, ROW_BATCH.min(total - row) as usize);
        if rows.is_empty() {
            break;
        }
        for item in &rows {
            let mut line = String::new();
            push_html_escaped(&mut line, &item.text);
            writer.write_chunk(line.as_bytes())?;
            if item.row + 1 < total {
                writer.write_chunk(nl.as_bytes())?;
            }
        }
        row += rows.len() as u64;
    }
    if auto_print {
        writer.write_chunk(
            b"</pre><script>window.addEventListener('load',function(){setTimeout(function(){window.print();},250);});</script></body></html>",
        )?;
    } else {
        // 自动化测试路径（SRT_PRINT_NO_AUTO）：不自动弹系统打印对话框
        writer.write_chunk(b"</pre></body></html>")?;
    }
    writer.finish()?;
    Ok(String::from_utf8(buffer).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textfile::session::FileSession;

    fn fixture(text: &str) -> (tempfile::TempDir, FileSession) {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("sample.txt");
        std::fs::write(&path, text.as_bytes()).expect("写样本");
        let session = FileSession::open(&path, None, 64).expect("打开样本");
        (dir, session)
    }

    #[test]
    fn format_from_extension() {
        assert_eq!(
            ExportFormat::from_path(Path::new("a.HTM")),
            Some(ExportFormat::Html)
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("a.markdown")),
            Some(ExportFormat::Markdown)
        );
        assert_eq!(ExportFormat::from_path(Path::new("a.pdf")), None);
    }

    #[test]
    fn csv_quotes_special_fields() {
        let (_dir, session) = fixture("plain\nwith,comma\nsay \"hi\"");
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.csv");
        export_document(&session, &target, ExportFormat::Csv).expect("导出");
        let text = std::fs::read_to_string(&target).unwrap();
        assert_eq!(text, "plain\n\"with,comma\"\n\"say \"\"hi\"\"\"");
    }

    #[test]
    fn json_is_line_array() {
        let (_dir, session) = fixture("a\\b\nc");
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.json");
        export_document(&session, &target, ExportFormat::Json).expect("导出");
        let text = std::fs::read_to_string(&target).unwrap();
        assert_eq!(text, "[\n  \"a\\\\b\",\n  \"c\"\n]\n");
    }

    #[test]
    fn html_escapes_markup() {
        let (_dir, session) = fixture("<b>&x</b>");
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.html");
        export_document(&session, &target, ExportFormat::Html).expect("导出");
        let text = std::fs::read_to_string(&target).unwrap();
        assert!(text.contains("&lt;b&gt;&amp;x&lt;/b&gt;"));
        assert!(text.contains("charset=\"utf-8\""));
    }

    #[test]
    fn txt_preserves_crlf() {
        let (_dir, session) = fixture("l1\r\nl2");
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.txt");
        export_document(&session, &target, ExportFormat::Txt).expect("导出");
        let text = std::fs::read_to_string(&target).unwrap();
        assert_eq!(text, "l1\r\nl2");
    }

    #[test]
    fn cap_rejects_large_output() {
        let (_dir, session) = fixture("aaaaaaaaaa\nbbbbbbbbbb");
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.txt");
        let err = export_document_capped(&session, &target, ExportFormat::Txt, 5).unwrap_err();
        assert!(matches!(err, ExportError::TooLarge { .. }));
    }

    #[test]
    fn print_html_auto_print_is_optional() {
        let (_dir, session) = fixture("print me");
        let html = print_html(&session, true).expect("打印 HTML");
        assert!(html.contains("window.print()"));
        assert!(html.contains("print me"));
        let html = print_html(&session, false).expect("打印 HTML（无自动弹窗）");
        assert!(!html.contains("window.print()"));
        assert!(html.contains("print me"));
    }
}
