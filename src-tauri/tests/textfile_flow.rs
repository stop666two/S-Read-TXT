//! 集成测试：textfile 读取引擎端到端（真实文件系统）。
//!
//! 与单元测试的区别：这里验证「真实文件写入 → 打开 → 取窗 → 编码切换 →
//! 百分比导航」完整链路，作为阶段 1 的集成验收。

use std::path::{Path, PathBuf};

use s_read_txt::textfile::encoding::FileEncoding;
use s_read_txt::textfile::session::FileSession;

/// 写测试文件并返回路径。
fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("写测试文件失败");
    path
}

/// 混合换行 + UTF-8：端到端取窗与百分比。
#[test]
fn utf8_mixed_newlines_end_to_end() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let text = "第一回 楔子\r\n第二回 风起\r第三回 云涌\n第四回 落幕\n";
    let path = write_file(dir.path(), "novel.txt", text.as_bytes());
    let session = FileSession::open(&path, None, 100).expect("打开失败");
    assert_eq!(session.encoding(), FileEncoding::Utf8);
    assert_eq!(session.rows_total(), 4);
    let rows = session.rows(0, 10);
    let joined: Vec<&str> = rows.iter().map(|row| row.text.as_str()).collect();
    assert_eq!(
        joined,
        vec!["第一回 楔子", "第二回 风起", "第三回 云涌", "第四回 落幕"]
    );
    // 起始位置百分比语义：首行为 0%；末行接近但不超过 100%（以行首定位）
    assert_eq!(session.percent_at_row(0), 0.0);
    let bottom = session.percent_at_row(session.rows_total() - 1);
    assert!(bottom > 70.0 && bottom < 100.0, "bottom={bottom}");
}

/// GB18030 自动检测；手动切 UTF-8 后内容变化，切回自动后恢复。
#[test]
fn gb18030_detection_and_encoding_switch() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let text = "这是一本中文小说。".repeat(200);
    let bytes = encoding_rs::GB18030.encode(&text).0.into_owned();
    let path = write_file(dir.path(), "gbk.txt", &bytes);
    let mut session = FileSession::open(&path, None, 100).expect("打开失败");
    assert_eq!(session.encoding(), FileEncoding::Gb18030);
    assert_eq!(session.rows(0, 1)[0].text, text);
    session.set_encoding(Some(FileEncoding::Utf8));
    assert_ne!(session.rows(0, 1)[0].text, text, "错误编码下不应还原原文");
    session.set_encoding(None);
    assert_eq!(session.rows(0, 1)[0].text, text);
}

/// 超长无换行文件：8KB 分块后无损拼接。
#[test]
fn long_line_chunking_end_to_end() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let bytes: Vec<u8> = (0..100_000u32).map(|i| b'a' + (i % 26) as u8).collect();
    let path = write_file(dir.path(), "long.txt", &bytes);
    let session = FileSession::open(&path, None, 100).expect("打开失败");
    assert_eq!(session.rows_total(), 13, "100000 / 8192 向上取整");
    let rows = session.rows(0, 20);
    let joined: String = rows.iter().map(|row| row.text.as_str()).collect();
    assert_eq!(joined.len(), 100_000);
    assert_eq!(joined.as_bytes(), bytes.as_slice());
}

/// 中等规模文件（约 200KB / 5000 行）：检查点导航与取窗一致性。
#[test]
fn checkpoint_navigation_on_larger_file() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let mut text = String::new();
    for index in 0..5000 {
        text.push_str(&format!("第{index:04}行：内容内容内容内容内容\n"));
    }
    let path = write_file(dir.path(), "big.txt", text.as_bytes());
    let session = FileSession::open(&path, None, 100).expect("打开失败");
    assert_eq!(session.rows_total(), 5000);
    let percent = session.percent_at_row(2500);
    let row = session.row_at_percent(percent);
    assert!((row as i64 - 2500).abs() <= 2, "定位漂移过大：row={row}");
    let window = session.rows(row, 3);
    assert_eq!(window[0].row, row);
    assert!(window[0].text.starts_with(&format!("第{row:04}行")));
}

/// 空文件与仅换行文件的安全行为。
#[test]
fn empty_and_newline_only_files() {
    let dir = tempfile::tempdir().expect("创建临时目录失败");
    let empty = write_file(dir.path(), "empty.txt", b"");
    let empty_session = FileSession::open(&empty, None, 100).expect("空文件应可打开");
    assert_eq!(empty_session.rows_total(), 1);
    assert_eq!(empty_session.rows(0, 1)[0].text, "");
    assert_eq!(empty_session.percent_at_row(0), 0.0);

    let newline_only = write_file(dir.path(), "nl.txt", b"\n");
    let newline_session = FileSession::open(&newline_only, None, 100).expect("仅换行应可打开");
    assert_eq!(newline_session.rows_total(), 1);
    assert_eq!(newline_session.rows(0, 1)[0].text, "");
}
