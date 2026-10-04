//! 保存链：编码询问流后端、原文直拷 vs 全量转码、`.bak`、原子写、外部冲突检测。
//!
//! 职责边界（设计 §5.4）：
//! - 本模块只负责「把内存文档写回磁盘」的机制与校验；
//! - 「每次询问编码」的交互流、「首次保存」判定、`.bak` 开关属于会话层
//!   （阶段 4 接线），通过 [`SaveOptions`] 传入决策结果。
//!
//! 关键策略：
//! - 同编码保存：原文片段**字节直拷**（快）；新增片段（UTF-8）按目标编码编码；
//! - 换编码保存：原文片段先按原编码解码、再按目标编码编码；
//! - 目标编码无法表示的字符：报 [`SaveError::Unrepresentable`]（UI 提示转 UTF-8 或取消），
//!   临时文件回滚，磁盘不受影响；
//! - BOM：UTF-16 目标始终写对应 BOM（否则不易被检测）；UTF-8 目标仅在原文件
//!   为 UTF-8+BOM 时保留 BOM；传统编码不写 BOM；
//! - 写入 = 同目录临时文件 + `sync_all` + `rename` 原子替换；
//!   映射保持打开（Rust 打开句柄带 `FILE_SHARE_DELETE`），替换后旧映射仍有效。

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::textfile::editing::edit_doc::EditDoc;
use crate::textfile::editing::piece::PieceSource;
use crate::textfile::encoding::FileEncoding;

/// UTF-8 BOM。
const BOM_UTF8: [u8; 3] = [0xEF, 0xBB, 0xBF];
/// UTF-16LE BOM。
const BOM_UTF16LE: [u8; 2] = [0xFF, 0xFE];
/// UTF-16BE BOM。
const BOM_UTF16BE: [u8; 2] = [0xFE, 0xFF];

/// 磁盘快照（修改时间 + 字节长度），用于外部修改冲突检测。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskSnapshot {
    /// 文件修改时间
    pub modified: SystemTime,
    /// 文件长度（字节）
    pub len: u64,
}

/// 保存决策（由会话层按设置与交互结果构造）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveOptions {
    /// 目标编码（编码询问流的结果；默认 = 原编码）
    pub target_encoding: FileEncoding,
    /// 是否写入 `.bak`（会话层判定：设置开启 且 本会话首次保存 且 文件已存在）
    pub make_backup: bool,
    /// 冲突时是否强制覆盖（用户选择「覆盖」后为 `true`）
    pub force: bool,
    /// 打开/上次保存时的磁盘快照（`None` = 无基准，跳过冲突检测）
    pub expected: Option<DiskSnapshot>,
}

/// 保存结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveOutcome {
    /// 实际写入字节数（含 BOM）
    pub bytes_written: u64,
    /// `.bak` 路径（未写时为 `None`）
    pub backup_path: Option<PathBuf>,
    /// 实际保存编码
    pub encoding: FileEncoding,
}

/// 保存错误。
#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    /// 文件被外部修改（大小或修改时间与基准不一致）
    #[error("文件已被外部修改")]
    Conflict,
    /// 目标编码无法表示某字符（UI 应提示转 UTF-8 保存或取消）
    #[error("存在目标编码无法表示的字符：{ch}")]
    Unrepresentable {
        /// 无法表示的字符
        ch: char,
    },
    /// IO 失败（创建临时文件/写盘/替换）
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// 读取磁盘快照；文件不存在返回 `Ok(None)`。
pub fn snapshot_of(path: &Path) -> std::io::Result<Option<DiskSnapshot>> {
    match std::fs::metadata(path) {
        Ok(meta) => Ok(Some(DiskSnapshot {
            modified: meta.modified()?,
            len: meta.len(),
        })),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// `.bak` 路径派生：`<文件名>.bak`（与文件同目录）。
pub fn backup_path_for(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// 保存文档到 `path`。
///
/// 成功后调用 `doc.mark_saved()`（脏标记清零）。
/// 失败保证磁盘原文件不变（临时文件已清理）。
pub fn save_doc(
    doc: &mut EditDoc,
    path: &Path,
    options: &SaveOptions,
) -> Result<SaveOutcome, SaveError> {
    // 1) 冲突检测（无基准或强制时跳过）
    let current = snapshot_of(path)?;
    if !options.force {
        if let (Some(expected), Some(now)) = (options.expected, current) {
            if expected != now {
                return Err(SaveError::Conflict);
            }
        }
    }
    // 2) 首存 .bak（覆盖前备份当前磁盘内容）
    let backup_path = if options.make_backup && current.is_some() {
        let backup = backup_path_for(path);
        std::fs::copy(path, &backup)?;
        Some(backup)
    } else {
        None
    };
    // 3) 临时文件写入 + 原子替换
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("out.txt"));
    let directory = path.parent().filter(|dir| !dir.as_os_str().is_empty());
    let temp_path = match directory {
        Some(dir) => dir.join(format!(".{file_name}.srt-tmp-{}", std::process::id())),
        None => PathBuf::from(format!(".{file_name}.srt-tmp-{}", std::process::id())),
    };
    let written = match write_content(doc, &temp_path, options.target_encoding) {
        Ok(written) => written,
        Err(err) => {
            let _ = std::fs::remove_file(&temp_path);
            return Err(err);
        }
    };
    if let Err(err) = std::fs::rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(SaveError::Io(err));
    }
    doc.mark_saved();
    Ok(SaveOutcome {
        bytes_written: written,
        backup_path,
        encoding: options.target_encoding,
    })
}

/// 生成完整文档字节（BOM + 片段流；供应商应急快照与测试复用）。
pub fn document_bytes(doc: &EditDoc, target: FileEncoding) -> Result<Vec<u8>, SaveError> {
    let mut buffer: Vec<u8> = Vec::new();
    encode_into(doc, &mut buffer, target)?;
    Ok(buffer)
}

/// 写出完整内容（BOM + 片段流）；返回写入字节数。
fn write_content(doc: &EditDoc, path: &Path, target: FileEncoding) -> Result<u64, SaveError> {
    let mut file = File::create(path)?;
    let written = encode_into(doc, &mut file, target)?;
    file.flush()?;
    file.sync_all()?;
    Ok(written)
}

/// 将完整文档编码写入任意写入器（BOM + 片段流）。
fn encode_into<W: std::io::Write>(
    doc: &EditDoc,
    writer: &mut W,
    target: FileEncoding,
) -> Result<u64, SaveError> {
    let mut written = 0u64;
    if let Some(bom) = bom_for(doc, target) {
        writer.write_all(bom)?;
        written += bom.len() as u64;
    }
    let original = doc.original_bytes();
    for piece in doc.pieces() {
        let from = piece.off as usize;
        let to = piece.end() as usize;
        match piece.source {
            PieceSource::Original => {
                let bytes = &original[from..to];
                if target == doc.encoding() {
                    // 编码不变：原文片段字节直拷
                    writer.write_all(bytes)?;
                    written += piece.len;
                } else {
                    let text = doc
                        .encoding()
                        .encoding()
                        .decode_without_bom_handling(bytes)
                        .0;
                    written += write_encoded(writer, &text, target)?;
                }
            }
            PieceSource::Added => {
                let utf8 = &doc.added()[from..to];
                if target == FileEncoding::Utf8 {
                    writer.write_all(utf8)?;
                    written += piece.len;
                } else {
                    let text = String::from_utf8_lossy(utf8);
                    written += write_encoded(writer, &text, target)?;
                }
            }
        }
    }
    Ok(written)
}

/// BOM 策略（见模块说明）。
fn bom_for(doc: &EditDoc, target: FileEncoding) -> Option<&'static [u8]> {
    match target {
        FileEncoding::Utf16Le => Some(&BOM_UTF16LE),
        FileEncoding::Utf16Be => Some(&BOM_UTF16BE),
        FileEncoding::Utf8 if doc.encoding() == FileEncoding::Utf8 && doc.bom_len() == 3 => {
            Some(&BOM_UTF8)
        }
        _ => None,
    }
}

/// 将字符串按目标编码流式写出；无法表示时返回 [`SaveError::Unrepresentable`]。
fn write_encoded<W: std::io::Write>(
    writer: &mut W,
    text: &str,
    target: FileEncoding,
) -> Result<u64, SaveError> {
    match target {
        FileEncoding::Utf8 => {
            writer.write_all(text.as_bytes())?;
            Ok(text.len() as u64)
        }
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => {
            // 注意：encoding_rs 的 `Encoding::encode` 对 UTF-16 是 Web 表单语义
            // （会改写为 UTF-8），因此 UTF-16 分支手工编码
            let little_endian = target == FileEncoding::Utf16Le;
            let mut buffer = [0u8; 2];
            let mut written = 0u64;
            for unit in text.encode_utf16() {
                let pair = if little_endian {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                };
                buffer.copy_from_slice(&pair);
                writer.write_all(&buffer)?;
                written += 2;
            }
            Ok(written)
        }
        _ => {
            let mut encoder = target.encoding().new_encoder();
            let mut input = text;
            let mut buffer = [0u8; 4096];
            let mut written = 0u64;
            loop {
                // `_without_replacement`：不可表示字符作为致命错误上报
                // （普通 encode_from_utf8 会替换为数字字符引用，不符合文本保存语义）
                let (result, read, produced) =
                    encoder.encode_from_utf8_without_replacement(input, &mut buffer, true);
                writer.write_all(&buffer[..produced])?;
                written += produced as u64;
                input = &input[read..];
                match result {
                    encoding_rs::EncoderResult::InputEmpty => break,
                    encoding_rs::EncoderResult::OutputFull => continue,
                    encoding_rs::EncoderResult::Unmappable(ch) => {
                        return Err(SaveError::Unrepresentable { ch })
                    }
                }
            }
            Ok(written)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textfile::editing::edit_doc::EditOp;

    /// 打开文档（测试辅助）。
    fn open_doc(path: &Path) -> EditDoc {
        EditDoc::open(path, None, 100).expect("打开失败")
    }

    /// 默认保存选项（UTF-8 目标、无备份、无冲突基准）。
    fn plain_options() -> SaveOptions {
        SaveOptions {
            target_encoding: FileEncoding::Utf8,
            make_backup: false,
            force: false,
            expected: None,
        }
    }

    /// 编辑 → 保存 → 重开一致性 + 脏标记清零。
    #[test]
    fn save_roundtrip_and_mark_clean() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("a.txt");
        std::fs::write(&path, b"abc\ndef").expect("写文件失败");
        let mut doc = open_doc(&path);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 1,
            text: "X".into(),
        }])
        .expect("插入失败");
        assert!(doc.is_dirty());
        let outcome = save_doc(&mut doc, &path, &plain_options()).expect("保存失败");
        assert_eq!(outcome.bytes_written, 8);
        assert!(!doc.is_dirty());
        assert_eq!(std::fs::read(&path).expect("读文件失败"), b"aXbc\ndef");
        let reopened = open_doc(&path);
        assert_eq!(reopened.row_text(0).as_deref(), Some("aXbc"));
        assert_eq!(reopened.row_text(1).as_deref(), Some("def"));
    }

    /// UTF-8 + BOM：保存保留 BOM。
    #[test]
    fn save_keeps_utf8_bom() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("bom.txt");
        let mut bytes = BOM_UTF8.to_vec();
        bytes.extend_from_slice("中文".as_bytes());
        std::fs::write(&path, &bytes).expect("写文件失败");
        let mut doc = open_doc(&path);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 2,
            text: "-".into(),
        }])
        .expect("插入失败");
        save_doc(&mut doc, &path, &plain_options()).expect("保存失败");
        let saved = std::fs::read(&path).expect("读文件失败");
        assert!(saved.starts_with(&BOM_UTF8));
        assert_eq!(
            String::from_utf8(saved[BOM_UTF8.len()..].to_vec()).expect("应为 UTF-8"),
            "中文-"
        );
    }

    /// GB18030 → UTF-8 转码保存。
    #[test]
    fn transcode_gb18030_to_utf8() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("gbk.txt");
        let gbk = encoding_rs::GB18030.encode("中文内容").0.into_owned();
        std::fs::write(&path, &gbk).expect("写文件失败");
        let mut doc = open_doc(&path);
        assert_eq!(doc.encoding(), FileEncoding::Gb18030);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 2,
            text: "-add-".into(),
        }])
        .expect("插入失败");
        save_doc(&mut doc, &path, &plain_options()).expect("保存失败");
        let saved = std::fs::read(&path).expect("读文件失败");
        assert_eq!(
            String::from_utf8(saved).expect("应为 UTF-8"),
            "中文-add-内容"
        );
    }

    /// 目标编码无法表示：原子失败，磁盘不变、无临时文件残留。
    #[test]
    fn unrepresentable_is_rejected_atomically() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("c.txt");
        std::fs::write(&path, b"abc").expect("写文件失败");
        let mut doc = open_doc(&path);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 1,
            text: "😀".into(),
        }])
        .expect("插入失败");
        let options = SaveOptions {
            target_encoding: FileEncoding::Big5,
            ..plain_options()
        };
        match save_doc(&mut doc, &path, &options) {
            Err(SaveError::Unrepresentable { ch }) => assert_eq!(ch, '😀'),
            other => panic!("应为 Unrepresentable：{other:?}"),
        }
        assert_eq!(std::fs::read(&path).expect("读文件失败"), b"abc");
        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .expect("读目录失败")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains("srt-tmp"))
            .collect();
        assert!(leftovers.is_empty(), "存在临时文件残留：{leftovers:?}");
        assert!(doc.is_dirty());
    }

    /// `.bak`：覆盖前备份磁盘原内容。
    #[test]
    fn backup_created_from_original() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("d.txt");
        std::fs::write(&path, b"old").expect("写文件失败");
        let mut doc = open_doc(&path);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 3,
            text: "!".into(),
        }])
        .expect("插入失败");
        let options = SaveOptions {
            make_backup: true,
            ..plain_options()
        };
        let outcome = save_doc(&mut doc, &path, &options).expect("保存失败");
        let backup = outcome.backup_path.expect("应生成 .bak");
        assert_eq!(std::fs::read(&backup).expect("读备份失败"), b"old");
        assert_eq!(std::fs::read(&path).expect("读文件失败"), b"old!");
    }

    /// 冲突检测与强制覆盖。
    #[test]
    fn conflict_detection_and_force() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("e.txt");
        std::fs::write(&path, b"abc").expect("写文件失败");
        let mut doc = open_doc(&path);
        let snapshot = snapshot_of(&path).expect("快照失败").expect("应存在");
        // 外部修改按编辑器惯例以「临时文件 + rename」模拟：in-place 写会被
        // 我们的 mmap 拒绝（Windows ERROR_USER_MAPPED_FILE）
        let external = dir.path().join("e.external");
        std::fs::write(&external, b"abc-changed").expect("外部写失败");
        std::fs::rename(&external, &path).expect("外部替换失败");
        let options = SaveOptions {
            expected: Some(snapshot),
            ..plain_options()
        };
        match save_doc(&mut doc, &path, &options) {
            Err(SaveError::Conflict) => {}
            other => panic!("应为 Conflict：{other:?}"),
        }
        let forced = SaveOptions {
            force: true,
            ..options
        };
        save_doc(&mut doc, &path, &forced).expect("强制保存失败");
        assert_eq!(std::fs::read(&path).expect("读文件失败"), b"abc");
    }

    /// 另存为：目标为新路径，源文件不变。
    #[test]
    fn save_as_new_path() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let source = dir.path().join("f.txt");
        std::fs::write(&source, b"one").expect("写文件失败");
        let target = dir.path().join("g.txt");
        let mut doc = open_doc(&source);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 3,
            text: " two".into(),
        }])
        .expect("插入失败");
        save_doc(&mut doc, &target, &plain_options()).expect("另存为失败");
        assert_eq!(std::fs::read(&target).expect("读目标失败"), b"one two");
        assert_eq!(std::fs::read(&source).expect("读源失败"), b"one");
    }

    /// UTF-16 往返：原文 UTF-16 + 新增 UTF-8 片段 → 保存为 UTF-16 可重开。
    #[test]
    fn utf16_roundtrip_with_added_utf8() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("h.txt");
        let mut bytes = BOM_UTF16LE.to_vec();
        for unit in "第一行\n第二行".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        std::fs::write(&path, &bytes).expect("写文件失败");
        let mut doc = open_doc(&path);
        assert_eq!(doc.encoding(), FileEncoding::Utf16Le);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 3,
            text: "X".into(),
        }])
        .expect("插入失败");
        let options = SaveOptions {
            target_encoding: FileEncoding::Utf16Le,
            ..plain_options()
        };
        save_doc(&mut doc, &path, &options).expect("保存失败");
        let saved = std::fs::read(&path).expect("读文件失败");
        assert!(saved.starts_with(&BOM_UTF16LE));
        let reopened = open_doc(&path);
        assert_eq!(reopened.encoding(), FileEncoding::Utf16Le);
        assert_eq!(reopened.row_text(0).as_deref(), Some("第一行X"));
        assert_eq!(reopened.row_text(1).as_deref(), Some("第二行"));
    }
}
