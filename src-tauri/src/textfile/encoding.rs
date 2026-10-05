//! 编码检测与解码（WHATWG 编码族子集）。
//!
//! 支持范围（对外承诺，随界面编码菜单提供）：
//! UTF-8 / UTF-16LE / UTF-16BE / GB18030（兼容 GBK/GB2312）/ Big5 /
//! Shift_JIS / EUC-KR / windows-1252。
//!
//! 检测顺序：
//! 1. BOM（UTF-8 / UTF-16LE / UTF-16BE）；
//! 2. 全文件 UTF-8 校验（纯 ASCII 也归为 UTF-8；std 的校验为 SIMD 级速度）；
//! 3. chardetng 采样检测（前 2MB）；
//! 4. 无法映射到支持列表的检测结果 → windows-1252 兜底（可手动切换纠正）。
//!
//! 实现要点：
//! - 检测结果按 **WHATWG 规范名**映射（名称稳定、与库实现解耦；不依赖
//!   `Encoding` 对象的身份与比较语义——实测对象比较在本组合下未命中 GB18030）；
//! - 注意 WHATWG 中 GB18030 编码的规范名称是 `GBK`；
//! - `encoding_rs::Encoding::encode` 是 Web 表单语义：请求 UTF-16 时会被改写为
//!   UTF-8。因此本模块只做**解码**；保存路径使用 encoding_rs 的
//!   `Encoder` 流式接口编码输出。
//!
//! 解码：`encoding_rs::Encoding::decode` 按窗口解码并自动剥离 BOM；
//! 行边界在 WHATWG 多字节编码中必定落在字符边界（0x0A/0x0D 不可能出现在
//! 多字节序列内部），UTF-16 由行索引按双字节对扫描，详见 `line_index`。

use encoding_rs::Encoding;

/// UTF-8 BOM
const BOM_UTF8: [u8; 3] = [0xEF, 0xBB, 0xBF];
/// UTF-16LE BOM
const BOM_UTF16LE: [u8; 2] = [0xFF, 0xFE];
/// UTF-16BE BOM
const BOM_UTF16BE: [u8; 2] = [0xFE, 0xFF];
/// 采样检测的最大字节数（2MB：平衡检测质量与耗时）
const MAX_SAMPLE_BYTES: usize = 2 * 1024 * 1024;

/// 支持的编码（`label()` 即对外名称，序列化到会话/历史）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEncoding {
    /// UTF-8（含纯 ASCII）
    Utf8,
    /// UTF-16 小端
    Utf16Le,
    /// UTF-16 大端
    Utf16Be,
    /// GB18030（兼容 GBK/GB2312）
    Gb18030,
    /// Big5（繁体中文）
    Big5,
    /// Shift_JIS（日文）
    ShiftJis,
    /// EUC-KR（韩文）
    EucKr,
    /// windows-1252（西欧单字节）
    Windows1252,
}

impl FileEncoding {
    /// 界面编码菜单的展示顺序（常见优先）。
    pub const ALL: [FileEncoding; 8] = [
        FileEncoding::Utf8,
        FileEncoding::Gb18030,
        FileEncoding::Utf16Le,
        FileEncoding::Utf16Be,
        FileEncoding::Big5,
        FileEncoding::ShiftJis,
        FileEncoding::EucKr,
        FileEncoding::Windows1252,
    ];

    /// 对外名称（会话/历史中持久化的值；与 `docs/configuration.md` 一致）。
    pub fn label(self) -> &'static str {
        match self {
            FileEncoding::Utf8 => "UTF-8",
            FileEncoding::Utf16Le => "UTF-16LE",
            FileEncoding::Utf16Be => "UTF-16BE",
            FileEncoding::Gb18030 => "GB18030",
            FileEncoding::Big5 => "Big5",
            FileEncoding::ShiftJis => "Shift_JIS",
            FileEncoding::EucKr => "EUC-KR",
            FileEncoding::Windows1252 => "windows-1252",
        }
    }

    /// 由名称解析（宽容：忽略大小写与分隔符，接受 GBK/CP1252/SJIS 等常见别名）。
    pub fn from_label(label: &str) -> Option<Self> {
        let normalized: String = label
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect();
        match normalized.as_str() {
            "utf8" => Some(FileEncoding::Utf8),
            "utf16le" => Some(FileEncoding::Utf16Le),
            "utf16be" => Some(FileEncoding::Utf16Be),
            "gb18030" | "gbk" | "gb2312" => Some(FileEncoding::Gb18030),
            "big5" => Some(FileEncoding::Big5),
            "shiftjis" | "sjis" | "cp932" => Some(FileEncoding::ShiftJis),
            "euckr" | "cp949" => Some(FileEncoding::EucKr),
            "windows1252" | "cp1252" | "ansi" => Some(FileEncoding::Windows1252),
            _ => None,
        }
    }

    /// 对应的 `encoding_rs` 编码器（本模块用于解码；保存编码见模块说明）。
    pub fn encoding(self) -> &'static Encoding {
        match self {
            FileEncoding::Utf8 => encoding_rs::UTF_8,
            FileEncoding::Utf16Le => encoding_rs::UTF_16LE,
            FileEncoding::Utf16Be => encoding_rs::UTF_16BE,
            FileEncoding::Gb18030 => encoding_rs::GB18030,
            FileEncoding::Big5 => encoding_rs::BIG5,
            FileEncoding::ShiftJis => encoding_rs::SHIFT_JIS,
            FileEncoding::EucKr => encoding_rs::EUC_KR,
            FileEncoding::Windows1252 => encoding_rs::WINDOWS_1252,
        }
    }

    /// 新建文件时写入的 BOM（仅 UTF-16 需要；其余返回空）。
    ///
    /// 用途：P3-2 新建文件（无内容文件靠 BOM 保证自动检测可识别编码）。
    pub fn bom(self) -> &'static [u8] {
        match self {
            FileEncoding::Utf16Le => &BOM_UTF16LE,
            FileEncoding::Utf16Be => &BOM_UTF16BE,
            _ => &[],
        }
    }
}

/// 检测文件编码（顺序见模块说明）。
pub fn detect(bytes: &[u8]) -> FileEncoding {
    if bytes.starts_with(&BOM_UTF8) {
        return FileEncoding::Utf8;
    }
    if bytes.starts_with(&BOM_UTF16LE) {
        return FileEncoding::Utf16Le;
    }
    if bytes.starts_with(&BOM_UTF16BE) {
        return FileEncoding::Utf16Be;
    }
    if std::str::from_utf8(bytes).is_ok() {
        return FileEncoding::Utf8;
    }
    detect_by_sampler(&bytes[..bytes.len().min(MAX_SAMPLE_BYTES)])
}

/// 采样（前 2MB）交给 chardetng，并把结果映射到支持列表。
fn detect_by_sampler(sample: &[u8]) -> FileEncoding {
    // ISO-2022-JP 不在支持列表：Deny 让日文内容落向 Shift_JIS（更常见的 TXT 编码）。
    // UTF-8 此前已被全文件校验排除（走到这里说明不是合法 UTF-8），故 Deny。
    let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    detector.feed(sample, true);
    let guessed = detector.guess(None, chardetng::Utf8Detection::Deny);
    map_detected_name(guessed.name())
}

/// WHATWG 规范名 → 支持列表（名称对照见 encoding_rs/Encoding Standard）。
///
/// 说明：GB18030 的规范名是 `GBK`（WHATWG 历史命名）；其余为常见规范名。
fn map_detected_name(name: &str) -> FileEncoding {
    match name {
        "UTF-8" => FileEncoding::Utf8,
        "UTF-16LE" => FileEncoding::Utf16Le,
        "UTF-16BE" => FileEncoding::Utf16Be,
        "GBK" => FileEncoding::Gb18030,
        "Big5" => FileEncoding::Big5,
        "Shift_JIS" => FileEncoding::ShiftJis,
        "EUC-KR" => FileEncoding::EucKr,
        _ => FileEncoding::Windows1252,
    }
}

/// 按编码解码一段字节（自动剥离 BOM；无效序列替换为 U+FFFD）。
pub fn decode_range(bytes: &[u8], encoding: FileEncoding) -> String {
    encoding.encoding().decode(bytes).0.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编码为传统编码（非 UTF-16）的字节（测试辅助）。
    ///
    /// 注意：不能用 `Encoding::encode` 构造 UTF-16 测试数据——该方法按 Web 表单
    /// 语义会把 UTF-16 请求改写为 UTF-8（使用 `encode_utf16le` 辅助代替）。
    fn encode_legacy(text: &str, encoding: FileEncoding) -> Vec<u8> {
        debug_assert!(matches!(
            encoding,
            FileEncoding::Utf8
                | FileEncoding::Gb18030
                | FileEncoding::Big5
                | FileEncoding::ShiftJis
                | FileEncoding::EucKr
                | FileEncoding::Windows1252
        ));
        encoding.encoding().encode(text).0.into_owned()
    }

    /// UTF-16LE 字节（测试辅助）：手工按小端双字节构造。
    fn encode_utf16le(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect()
    }

    /// BOM 优先于其他检测。
    #[test]
    fn bom_is_detected_first() {
        let mut utf8 = BOM_UTF8.to_vec();
        utf8.extend_from_slice(b"abc");
        assert_eq!(detect(&utf8), FileEncoding::Utf8);

        let mut utf16le = BOM_UTF16LE.to_vec();
        utf16le.extend_from_slice(&[0x61, 0x00]);
        assert_eq!(detect(&utf16le), FileEncoding::Utf16Le);

        let mut utf16be = BOM_UTF16BE.to_vec();
        utf16be.extend_from_slice(&[0x00, 0x61]);
        assert_eq!(detect(&utf16be), FileEncoding::Utf16Be);
    }

    /// 纯 ASCII 与无 BOM 的 UTF-8 中文都判为 UTF-8。
    #[test]
    fn ascii_and_utf8_chinese_are_utf8() {
        assert_eq!(detect(b"hello world\n"), FileEncoding::Utf8);
        let chinese = "这是一个用于测试的中文段落。".as_bytes();
        assert_eq!(detect(chinese), FileEncoding::Utf8);
    }

    /// GB18030 采样检测。
    #[test]
    fn gb18030_is_detected() {
        let text = "这是一个用于测试编码检测的中文文本段落。".repeat(30);
        let bytes = encode_legacy(&text, FileEncoding::Gb18030);
        assert_eq!(detect(&bytes), FileEncoding::Gb18030);
    }

    /// Big5 采样检测。
    #[test]
    fn big5_is_detected() {
        let text = "這是一個測試編碼偵測的繁體中文文本段落。".repeat(30);
        let bytes = encode_legacy(&text, FileEncoding::Big5);
        assert_eq!(detect(&bytes), FileEncoding::Big5);
    }

    /// Shift_JIS 采样检测。
    #[test]
    fn shift_jis_is_detected() {
        let text = "これは日本語の文字コード判定を確認するためのテスト文章です。".repeat(30);
        let bytes = encode_legacy(&text, FileEncoding::ShiftJis);
        assert_eq!(detect(&bytes), FileEncoding::ShiftJis);
    }

    /// 西欧单字节文本检测为 windows-1252。
    #[test]
    fn windows1252_is_detected() {
        let text = "café naïve résumé déjà vu ".repeat(40);
        let bytes = encode_legacy(&text, FileEncoding::Windows1252);
        assert_eq!(detect(&bytes), FileEncoding::Windows1252);
    }

    /// 名称往返与常见别名解析。
    #[test]
    fn label_roundtrip_and_aliases() {
        for encoding in FileEncoding::ALL {
            assert_eq!(FileEncoding::from_label(encoding.label()), Some(encoding));
        }
        assert_eq!(FileEncoding::from_label("GBK"), Some(FileEncoding::Gb18030));
        assert_eq!(
            FileEncoding::from_label(" cp1252 "),
            Some(FileEncoding::Windows1252)
        );
        assert_eq!(FileEncoding::from_label("unknown-enc"), None);
    }

    /// 按编码解码（GB18030 / UTF-16LE）。
    #[test]
    fn decode_range_roundtrips() {
        let chinese = "中文解码测试";
        let gbk = encode_legacy(chinese, FileEncoding::Gb18030);
        assert_eq!(decode_range(&gbk, FileEncoding::Gb18030), chinese);
        let utf16 = encode_utf16le(chinese);
        assert_eq!(decode_range(&utf16, FileEncoding::Utf16Le), chinese);
    }

    /// WHATWG 规范名映射（含 GB18030 的规范名是 GBK 这一历史事实）。
    #[test]
    fn spec_name_mapping() {
        assert_eq!(map_detected_name("GBK"), FileEncoding::Gb18030);
        assert_eq!(map_detected_name("Big5"), FileEncoding::Big5);
        assert_eq!(map_detected_name("Shift_JIS"), FileEncoding::ShiftJis);
        assert_eq!(map_detected_name("EUC-KR"), FileEncoding::EucKr);
        assert_eq!(map_detected_name("UTF-16LE"), FileEncoding::Utf16Le);
        assert_eq!(map_detected_name("windows-1251"), FileEncoding::Windows1252);
    }

    /// EUC-KR 采样检测。
    #[test]
    fn euc_kr_is_detected() {
        let text = "이것은 한국어 인코딩 감지를 위한 테스트 문장입니다. ".repeat(30);
        let bytes = encode_legacy(&text, FileEncoding::EucKr);
        assert_eq!(detect(&bytes), FileEncoding::EucKr);
    }

    /// 多语言解码往返：每种语言按其代表性编码写入后解码回原文本。
    #[test]
    fn multilingual_decode_roundtrips() {
        let cases: &[(&str, FileEncoding)] = &[
            (
                "English text with punctuation, 12345.",
                FileEncoding::Windows1252,
            ),
            ("café naïve résumé — déjà vu", FileEncoding::Windows1252),
            (
                "日本語のテキスト、カタカナ、ひらがな。",
                FileEncoding::ShiftJis,
            ),
            ("한국어 텍스트와 문장 부호.", FileEncoding::EucKr),
            ("繁體中文與標點符號。", FileEncoding::Big5),
            ("Русский текст, кириллица.", FileEncoding::Utf8),
            ("نص عربي مع علامات الترقيم.", FileEncoding::Utf8),
            ("טקסט עברי לדוגמה.", FileEncoding::Utf8),
            ("Emoji: 👨‍👩‍👧‍👦 🎌 ✨", FileEncoding::Utf8),
        ];
        for (text, encoding) in cases {
            let bytes = encode_legacy(text, *encoding);
            let decoded = decode_range(&bytes, *encoding);
            assert_eq!(&decoded, text, "往返不一致：{encoding:?} {text}");
        }
    }
}
