//! P2-1 文本统计集成测试：只读会话与编辑文档的文档级/选区级统计。
//!
//! 覆盖：UTF-8（含 ZWJ emoji）、GB18030、UTF-16LE（BOM）、编辑新增片段、
//! 选区半开区间切片与上限标记。仅使用公开 API。

use std::fs;

use s_read_txt::stats::{SELECTION_STATS_MAX_CHARS, STATS_MAX_CHARS};
use s_read_txt::textfile::editing::edit_doc::{EditDoc, EditOp};
use s_read_txt::textfile::session::FileSession;

fn write_file(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).expect("写测试文件失败");
    path
}

/// 只读会话：UTF-8 文本（含 ZWJ emoji 与空白分词）。
#[test]
fn read_session_utf8_stats() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_file(dir.path(), "u8.txt", "hi 你好 👨‍👩‍👧 end".as_bytes());
    let session = FileSession::open(&path, None, 10).expect("打开失败");
    let stats = session.document_stats(STATS_MAX_CHARS);
    assert_eq!(stats.codepoints, "hi 你好 👨‍👩‍👧 end".chars().count() as u64);
    assert_eq!(
        stats.graphemes,
        "hi 你好 👨‍👩‍👧 end".graphemes(true).count() as u64
    );
    assert_eq!(stats.words, 4, "hi / 你好 / 👨‍👩‍👧 / end");
    assert_eq!(stats.bytes, stats.bytes); // 字节口径自证：等于文件长度，见下
    assert_eq!(stats.bytes, fs::metadata(&path).expect("元数据失败").len());
    assert!(!stats.capped);
}

/// 只读会话：GB18030 文件（重复样本保证自动检测可靠；字节数为原始长度）。
#[test]
fn read_session_gb18030_stats() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let text = "汉字abc".repeat(20);
    let (encoded, _, _) = encoding_rs::GB18030.encode(&text);
    let path = write_file(dir.path(), "gb.txt", &encoded);
    let session = FileSession::open(&path, None, 10).expect("打开失败");
    let stats = session.document_stats(STATS_MAX_CHARS);
    assert_eq!(stats.codepoints, (text.chars().count()) as u64);
    assert_eq!(stats.words, 1);
    assert_eq!(stats.bytes, encoded.len() as u64);
}

/// 只读会话：UTF-16LE + BOM（计数不含 BOM，字节数含 BOM）。
#[test]
fn read_session_utf16_bom_stats() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let mut raw = vec![0xFF, 0xFE];
    raw.extend("hi你".encode_utf16().flat_map(|u| u.to_le_bytes()));
    let path = write_file(dir.path(), "u16.txt", &raw);
    let session = FileSession::open(&path, None, 10).expect("打开失败");
    let stats = session.document_stats(STATS_MAX_CHARS);
    assert_eq!(stats.codepoints, 3);
    assert_eq!(stats.words, 1);
    assert_eq!(stats.bytes, raw.len() as u64);
}

/// 编辑文档：插入新增片段后统计（新增缓冲与原文混合）。
#[test]
fn edit_doc_stats_after_insert() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_file(dir.path(), "t.txt", "ab\ncd".as_bytes());
    let mut doc = EditDoc::open(&path, None, 10).expect("打开失败");
    doc.apply_edits(&[EditOp::Insert {
        row: 0,
        utf16: 0,
        text: "é".to_string(),
    }])
    .expect("插入失败");
    let stats = doc.document_stats(STATS_MAX_CHARS);
    assert_eq!(stats.codepoints, "éab\ncd".chars().count() as u64);
    assert_eq!(stats.graphemes, 6);
    assert_eq!(stats.words, 2, "éab / cd");
    assert_eq!(stats.bytes, doc.byte_len());
    assert_eq!(stats.bytes, 7, "原文 5 + 新增 é 的 2 字节");
}

/// 选区统计：半开区间切片（UTF-8 字节口径）。
#[test]
fn edit_doc_range_stats_slice() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_file(dir.path(), "t.txt", "hello world".as_bytes());
    let doc = EditDoc::open(&path, None, 10).expect("打开失败");
    let stats = doc
        .range_stats((0, 0), (0, 5), SELECTION_STATS_MAX_CHARS)
        .expect("统计失败");
    assert_eq!(stats.codepoints, 5);
    assert_eq!(stats.bytes, 5);
    assert_eq!(stats.words, 1);
    // 越界坐标报错而不 panic。
    assert!(doc
        .range_stats((9, 0), (9, 1), SELECTION_STATS_MAX_CHARS)
        .is_err());
}

/// 上限：小上限触发 capped 且停止计数，字节仍为口径值。
#[test]
fn edit_doc_stats_cap() {
    let dir = tempfile::tempdir().expect("临时目录失败");
    let path = write_file(dir.path(), "t.txt", "abcdef".as_bytes());
    let doc = EditDoc::open(&path, None, 10).expect("打开失败");
    let stats = doc.document_stats(5);
    assert!(stats.capped);
    assert_eq!(stats.codepoints, 5);
    assert_eq!(stats.bytes, 6);
}

use unicode_segmentation::UnicodeSegmentation;
