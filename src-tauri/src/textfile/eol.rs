//! 换行符风格检测（状态栏展示与「换行符转换」的基础）。
//!
//! 只扫描文件前 [`EOL_SCAN_BYTES`] 字节：对状态栏展示而言，前 256KB
//! 足以代表文件的主流风格；全文件统计在超大文件上代价不成比例。
//! UTF-16 按双字节单元识别（`\r\n` = 0x000D 0x000A 两个单元）。

use serde::Serialize;

use crate::textfile::encoding::FileEncoding;

/// 扫描窗口（字节）。
pub const EOL_SCAN_BYTES: usize = 256 * 1024;

/// 主导换行符风格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::upper_case_acronyms)]
pub enum EolStyle {
    /// `\n`（Unix/LF）
    Lf,
    /// `\r\n`（Windows/CRLF）
    CrLf,
    /// `\r`（经典 Mac/CR）
    Cr,
    /// 混合（扫描窗口内出现多种）
    Mixed,
    /// 未检测到换行（空文件/单行无行尾）
    Unknown,
}

impl EolStyle {
    /// 稳定的序列化短名（IPC 与设置文案共用）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "lf",
            Self::CrLf => "crlf",
            Self::Cr => "cr",
            Self::Mixed => "mixed",
            Self::Unknown => "unknown",
        }
    }
}

/// 在文件前 [`EOL_SCAN_BYTES`] 字节内统计换行风格并判定主导风格。
///
/// 判定规则：无换行 → `Unknown`；超过一种 → `Mixed`；否则为唯一风格。
pub fn detect(bytes: &[u8], encoding: FileEncoding) -> EolStyle {
    let mut lf: u64 = 0;
    let mut crlf: u64 = 0;
    let mut cr: u64 = 0;
    match encoding {
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
            let be = encoding == FileEncoding::Utf16Be;
            let end = bytes.len().min(EOL_SCAN_BYTES) & !1;
            let unit = |i: usize| -> u16 {
                if be {
                    u16::from_be_bytes([bytes[i], bytes[i + 1]])
                } else {
                    u16::from_le_bytes([bytes[i], bytes[i + 1]])
                }
            };
            let mut i = 0;
            while i + 1 < end {
                if unit(i) == 0x0D {
                    if i + 3 < end && unit(i + 2) == 0x0A {
                        crlf += 1;
                        i += 4;
                        continue;
                    }
                    cr += 1;
                } else if unit(i) == 0x0A {
                    lf += 1;
                }
                i += 2;
            }
        }
        _ => {
            let end = bytes.len().min(EOL_SCAN_BYTES);
            let mut i = 0;
            while i < end {
                match bytes[i] {
                    b'\r' => {
                        if i + 1 < end && bytes[i + 1] == b'\n' {
                            crlf += 1;
                            i += 2;
                            continue;
                        }
                        cr += 1;
                    }
                    b'\n' => lf += 1,
                    _ => {}
                }
                i += 1;
            }
        }
    }
    let kinds = u8::from(lf > 0) + u8::from(crlf > 0) + u8::from(cr > 0);
    if kinds == 0 {
        EolStyle::Unknown
    } else if kinds > 1 {
        EolStyle::Mixed
    } else if crlf > 0 {
        EolStyle::CrLf
    } else if cr > 0 {
        EolStyle::Cr
    } else {
        EolStyle::Lf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_single_styles() {
        assert_eq!(detect(b"a\nb\nc", FileEncoding::Utf8), EolStyle::Lf);
        assert_eq!(detect(b"a\r\nb\r\n", FileEncoding::Utf8), EolStyle::CrLf);
        assert_eq!(detect(b"a\rb\rc", FileEncoding::Utf8), EolStyle::Cr);
    }

    #[test]
    fn mixed_and_empty() {
        assert_eq!(detect(b"a\nb\r\nc", FileEncoding::Utf8), EolStyle::Mixed);
        assert_eq!(detect(b"", FileEncoding::Utf8), EolStyle::Unknown);
        assert_eq!(
            detect(b"no newline at all", FileEncoding::Utf8),
            EolStyle::Unknown
        );
    }

    #[test]
    fn utf16_pairs_are_recognized() {
        let mut le = Vec::new();
        for unit in "a\r\nb".encode_utf16() {
            le.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(detect(&le, FileEncoding::Utf16Le), EolStyle::CrLf);
        let mut be = Vec::new();
        for unit in "a\rb".encode_utf16() {
            be.extend_from_slice(&unit.to_be_bytes());
        }
        assert_eq!(detect(&be, FileEncoding::Utf16Be), EolStyle::Cr);
    }

    #[test]
    fn as_str_is_stable() {
        assert_eq!(EolStyle::Lf.as_str(), "lf");
        assert_eq!(EolStyle::CrLf.as_str(), "crlf");
        assert_eq!(EolStyle::Cr.as_str(), "cr");
        assert_eq!(EolStyle::Mixed.as_str(), "mixed");
        assert_eq!(EolStyle::Unknown.as_str(), "unknown");
    }
}
