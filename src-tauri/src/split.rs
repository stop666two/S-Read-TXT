//! 拆分引擎：按行数或匹配行将文本文件切分为多个分片。
//!
//! 设计：
//! - 全程流式（`BufRead::read_until`），避免整文件进内存；预览仅抓取前若干片的
//!   首尾片段用于展示。
//! - 每个分片通过原子写落盘（临时文件 + rename），覆盖已有同名文件。
//! - 匹配行归入**下一片**（匹配行前切割）；空分片跳过。

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage::atomic::write_atomic;

/// 单文件输入上限（与比较/合并一致）。
pub const MAX_INPUT_BYTES: u64 = 256 * 1024 * 1024;
/// 输出分片数量上限。
pub const MAX_PARTS: usize = 9999;
/// 预览展示的分片数量上限。
pub const PREVIEW_PARTS: usize = 20;
/// 预览首尾片段截取字节上限。
pub const PREVIEW_SNIPPET_BYTES: usize = 200;

/// 拆分模式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SplitMode {
    /// 每 `lines_per_file` 行切一片（最后一片可少）。
    Lines { lines_per_file: u64 },
    /// 匹配行前切割（匹配行归下一片）；纯文本用包含匹配，正则用行匹配。
    Marker { marker: String, is_regex: bool },
}

/// 单个分片的元信息（预览用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPart {
    pub index: u32,
    pub name: String,
    pub lines: u64,
    pub bytes: u64,
    pub head: String,
    pub tail: String,
    /// 目标文件已存在（执行时会覆盖）。
    pub overwrites: bool,
}

/// 拆分预览计划。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPlan {
    pub parts: Vec<SplitPart>,
    pub total_lines: u64,
    pub total_bytes: u64,
    pub skipped_empty: u32,
}

/// 执行结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedSplit {
    pub files: Vec<String>,
    pub bytes: u64,
}

/// 拆分错误。
#[derive(Debug)]
pub enum SplitError {
    InvalidConfig(String),
    InvalidPattern(String),
    EmptyResult,
    TooManyParts(usize),
    TooLarge(u64),
    Io(std::io::Error),
}

impl std::fmt::Display for SplitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SplitError::InvalidConfig(reason) => write!(formatter, "拆分配置无效：{reason}"),
            SplitError::InvalidPattern(reason) => write!(formatter, "正则表达式无效：{reason}"),
            SplitError::EmptyResult => write!(formatter, "没有产生任何分片"),
            SplitError::TooManyParts(count) => {
                write!(formatter, "分片数量 {count} 超出上限 {MAX_PARTS}")
            }
            SplitError::TooLarge(bytes) => write!(formatter, "文件过大（{bytes} 字节）"),
            SplitError::Io(err) => write!(formatter, "读写失败：{err}"),
        }
    }
}

impl std::error::Error for SplitError {}

impl From<std::io::Error> for SplitError {
    fn from(err: std::io::Error) -> Self {
        SplitError::Io(err)
    }
}

/// 匹配器：纯文本包含或正则行匹配。
enum Matcher {
    Plain(String),
    Regex(regex::Regex),
}

impl Matcher {
    fn build(mode: &SplitMode) -> Result<Option<Matcher>, SplitError> {
        match mode {
            SplitMode::Lines { .. } => Ok(None),
            SplitMode::Marker { marker, is_regex } => {
                if marker.is_empty() {
                    return Err(SplitError::InvalidConfig("标记不能为空".to_string()));
                }
                if *is_regex {
                    let compiled = regex::Regex::new(marker)
                        .map_err(|err| SplitError::InvalidPattern(err.to_string()))?;
                    Ok(Some(Matcher::Regex(compiled)))
                } else {
                    Ok(Some(Matcher::Plain(marker.clone())))
                }
            }
        }
    }

    fn hit(&self, line: &[u8]) -> bool {
        match self {
            Matcher::Plain(marker) => String::from_utf8_lossy(line).contains(marker.as_str()),
            Matcher::Regex(compiled) => compiled.is_match(String::from_utf8_lossy(line).as_ref()),
        }
    }
}

fn validate(mode: &SplitMode) -> Result<(), SplitError> {
    if let SplitMode::Lines { lines_per_file } = mode {
        if *lines_per_file == 0 {
            return Err(SplitError::InvalidConfig("每片行数必须大于 0".to_string()));
        }
    }
    Matcher::build(mode).map(|_| ())
}

/// 生成分片文件名：`{stem}-{4 位序号}.{ext}`（无扩展名时省略点号）。
fn part_name(stem: &str, ext: &str, index: u32) -> String {
    if ext.is_empty() {
        format!("{stem}-{index:04}")
    } else {
        format!("{stem}-{index:04}.{ext}")
    }
}

struct TargetInfo {
    stem: String,
    ext: String,
    out_dir: PathBuf,
}

impl TargetInfo {
    fn from_source(path: &Path, out_dir: Option<&Path>) -> TargetInfo {
        let stem = path
            .file_stem()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "part".to_string());
        let ext = path
            .extension()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let out_dir = out_dir.map(Path::to_path_buf).unwrap_or_else(|| {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });
        TargetInfo { stem, ext, out_dir }
    }
}

/// 流式扫描一次，产出分片预览（不写盘）。
pub fn plan_file(
    path: &Path,
    mode: &SplitMode,
    out_dir: Option<&Path>,
) -> Result<SplitPlan, SplitError> {
    validate(mode)?;
    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_INPUT_BYTES {
        return Err(SplitError::TooLarge(meta.len()));
    }
    let target = TargetInfo::from_source(path, out_dir);
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let matcher = Matcher::build(mode)?;

    let mut parts: Vec<SplitPart> = Vec::new();
    let mut total_lines: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut skipped_empty: u32 = 0;

    let mut current_index: u32 = 0;
    let mut current_lines: u64 = 0;
    let mut current_bytes: u64 = 0;
    let mut head: Vec<u8> = Vec::new();
    let mut tail: Vec<u8> = Vec::new();

    let mut flush =
        |index: u32, lines: u64, bytes: u64, head: &[u8], tail: &[u8]| -> Result<(), SplitError> {
            if lines == 0 || bytes == 0 {
                return Ok(());
            }
            let name = part_name(&target.stem, &target.ext, index);
            let overwrites = target.out_dir.join(&name).is_file();
            parts.push(SplitPart {
                index,
                name,
                lines,
                bytes,
                head: snippet(head),
                tail: snippet(tail),
                overwrites,
            });
            if parts.len() > MAX_PARTS {
                return Err(SplitError::TooManyParts(parts.len()));
            }
            Ok(())
        };

    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        total_lines += 1;
        total_bytes += read as u64;

        let cut = matcher.as_ref().is_some_and(|m| m.hit(&line))
            || matches!(mode, SplitMode::Lines { lines_per_file } if current_lines >= *lines_per_file);
        if cut {
            if current_bytes == 0 {
                skipped_empty += 1;
            } else {
                current_index += 1;
                flush(current_index, current_lines, current_bytes, &head, &tail)?;
            }
            current_lines = 0;
            current_bytes = 0;
            head.clear();
            tail.clear();
        }

        current_lines += 1;
        current_bytes += read as u64;
        if head.len() < PREVIEW_SNIPPET_BYTES {
            let need = PREVIEW_SNIPPET_BYTES - head.len();
            head.extend_from_slice(&line[..line.len().min(need)]);
        }
        tail.extend_from_slice(&line);
        if tail.len() > PREVIEW_SNIPPET_BYTES {
            let excess = tail.len() - PREVIEW_SNIPPET_BYTES;
            tail.drain(..excess);
        }
    }
    if current_bytes == 0 {
        skipped_empty += 1;
    } else {
        current_index += 1;
        flush(current_index, current_lines, current_bytes, &head, &tail)?;
    }

    if parts.is_empty() {
        return Err(SplitError::EmptyResult);
    }
    Ok(SplitPlan {
        parts,
        total_lines,
        total_bytes,
        skipped_empty,
    })
}

/// 截断到字节上限并保证 UTF-8 边界安全（越界字节以替换符呈现）。
fn snippet(bytes: &[u8]) -> String {
    let cut = bytes.len().min(PREVIEW_SNIPPET_BYTES);
    let mut end = cut;
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// 执行拆分：流式读源文件，逐片原子写到输出目录。
pub fn apply_file(
    path: &Path,
    mode: &SplitMode,
    out_dir: Option<&Path>,
) -> Result<AppliedSplit, SplitError> {
    let plan = plan_file(path, mode, out_dir)?;
    let target = TargetInfo::from_source(path, out_dir);
    std::fs::create_dir_all(&target.out_dir)?;
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let matcher = Matcher::build(mode)?;

    let mut files: Vec<String> = Vec::new();
    let mut written_bytes: u64 = 0;
    let mut buffer: Vec<u8> = Vec::new();
    let mut current_lines: u64 = 0;
    let mut index: u32 = 0;

    let mut flush = |buffer: &mut Vec<u8>, index: &mut u32| -> Result<(), SplitError> {
        if buffer.is_empty() {
            return Ok(());
        }
        *index += 1;
        let name = part_name(&target.stem, &target.ext, *index);
        let dest = target.out_dir.join(&name);
        write_atomic(&dest, buffer)?;
        written_bytes += buffer.len() as u64;
        files.push(dest.to_string_lossy().into_owned());
        buffer.clear();
        Ok(())
    };

    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader.read_until(b'\n', &mut line)?;
        if read == 0 {
            break;
        }
        let cut = matcher.as_ref().is_some_and(|m| m.hit(&line))
            || matches!(mode, SplitMode::Lines { lines_per_file } if current_lines >= *lines_per_file);
        if cut {
            flush(&mut buffer, &mut index)?;
            current_lines = 0;
        }
        current_lines += 1;
        buffer.extend_from_slice(&line);
    }
    flush(&mut buffer, &mut index)?;

    debug_assert_eq!(files.len(), plan.parts.len());
    Ok(AppliedSplit {
        files,
        bytes: written_bytes,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{apply_file, plan_file, SplitError, SplitMode};
    use crate::storage::atomic::write_atomic;

    fn write_source(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        write_atomic(&path, content.as_bytes()).expect("写源文件失败");
        path
    }

    fn read_part(dir: &std::path::Path, name: &str) -> String {
        fs::read_to_string(dir.join(name)).expect("读取分片失败")
    }

    /// 行数模式：10 行切成 4+3+3，命名与内容正确。
    #[test]
    fn lines_mode_splits_with_names() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let content = (1..=10).map(|i| format!("l{i}\n")).collect::<String>();
        let src = write_source(temp.path(), "book.txt", &content);
        let plan =
            plan_file(&src, &SplitMode::Lines { lines_per_file: 3 }, None).expect("预览失败");
        assert_eq!(plan.parts.len(), 4);
        assert_eq!(plan.parts[0].name, "book-0001.txt");
        assert_eq!(plan.parts[0].lines, 3);
        assert_eq!(plan.parts[3].lines, 1);
        let applied =
            apply_file(&src, &SplitMode::Lines { lines_per_file: 3 }, None).expect("拆分失败");
        assert_eq!(applied.files.len(), 4);
        assert_eq!(read_part(temp.path(), "book-0001.txt"), "l1\nl2\nl3\n");
        assert_eq!(read_part(temp.path(), "book-0004.txt"), "l10\n");
    }

    /// 无结尾换行的单行文件可拆为一片。
    #[test]
    fn single_line_without_newline() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let src = write_source(temp.path(), "one.txt", "solo");
        let applied =
            apply_file(&src, &SplitMode::Lines { lines_per_file: 10 }, None).expect("拆分失败");
        assert_eq!(applied.files.len(), 1);
        assert_eq!(read_part(temp.path(), "one-0001.txt"), "solo");
    }

    /// 空文件 → EmptyResult。
    #[test]
    fn empty_file_rejected() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let src = write_source(temp.path(), "empty.txt", "");
        let err = plan_file(&src, &SplitMode::Lines { lines_per_file: 5 }, None).unwrap_err();
        assert!(matches!(err, SplitError::EmptyResult));
    }

    /// 标记模式：匹配行归下一片；首行匹配不产生空片。
    #[test]
    fn marker_mode_cuts_before_match() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let src = write_source(temp.path(), "log.txt", "header\nx\n---\ny\n---\nz\n");
        let applied = apply_file(
            &src,
            &SplitMode::Marker {
                marker: "---".to_string(),
                is_regex: false,
            },
            None,
        )
        .expect("拆分失败");
        assert_eq!(applied.files.len(), 3);
        assert_eq!(read_part(temp.path(), "log-0001.txt"), "header\nx\n");
        assert_eq!(read_part(temp.path(), "log-0002.txt"), "---\ny\n");
        assert_eq!(read_part(temp.path(), "log-0003.txt"), "---\nz\n");

        let src2 = write_source(temp.path(), "first.txt", "---\na\n");
        let applied2 = apply_file(
            &src2,
            &SplitMode::Marker {
                marker: "---".to_string(),
                is_regex: false,
            },
            None,
        )
        .expect("拆分失败");
        assert_eq!(applied2.files.len(), 1);
    }

    /// 正则标记模式。
    #[test]
    fn marker_regex_mode() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let src = write_source(temp.path(), "re.txt", "a\n=== 1 ===\nb\n=== 2 ===\nc\n");
        let applied = apply_file(
            &src,
            &SplitMode::Marker {
                marker: "^=== ".to_string(),
                is_regex: true,
            },
            None,
        )
        .expect("拆分失败");
        assert_eq!(applied.files.len(), 3);
        assert_eq!(read_part(temp.path(), "re-0002.txt"), "=== 1 ===\nb\n");
    }

    /// 非法正则 → InvalidPattern；零行数 → InvalidConfig；空标记 → InvalidConfig。
    #[test]
    fn invalid_configs_rejected() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let src = write_source(temp.path(), "x.txt", "a\n");
        assert!(matches!(
            plan_file(&src, &SplitMode::Lines { lines_per_file: 0 }, None).unwrap_err(),
            SplitError::InvalidConfig(_)
        ));
        assert!(matches!(
            plan_file(
                &src,
                &SplitMode::Marker {
                    marker: "(".to_string(),
                    is_regex: true
                },
                None
            )
            .unwrap_err(),
            SplitError::InvalidPattern(_)
        ));
        assert!(matches!(
            plan_file(
                &src,
                &SplitMode::Marker {
                    marker: String::new(),
                    is_regex: false
                },
                None
            )
            .unwrap_err(),
            SplitError::InvalidConfig(_)
        ));
    }

    /// CRLF 行尾保持原始字节。
    #[test]
    fn crlf_bytes_preserved() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let src = write_source(temp.path(), "crlf.txt", "a\r\nb\r\nc\r\n");
        apply_file(&src, &SplitMode::Lines { lines_per_file: 2 }, None).expect("拆分失败");
        let first = fs::read(temp.path().join("crlf-0001.txt")).expect("读取失败");
        assert_eq!(first, b"a\r\nb\r\n");
    }

    /// 预览首尾片段截断到上限内。
    #[test]
    fn preview_snippets_truncated() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let long = format!("{}\n", "x".repeat(600));
        let src = write_source(temp.path(), "big.txt", &long.repeat(3));
        let plan =
            plan_file(&src, &SplitMode::Lines { lines_per_file: 1 }, None).expect("预览失败");
        assert_eq!(plan.parts.len(), 3);
        for part in &plan.parts {
            assert!(part.head.len() <= super::PREVIEW_SNIPPET_BYTES);
            assert!(part.tail.len() <= super::PREVIEW_SNIPPET_BYTES);
        }
    }

    /// 输出目录可指定；已存在同名目标在预览中标记覆盖。
    #[test]
    fn out_dir_and_overwrite_flag() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let out = temp.path().join("out");
        fs::create_dir_all(&out).expect("建目录失败");
        fs::write(out.join("book-0001.txt"), "old").expect("写文件失败");
        let src = write_source(temp.path(), "book.txt", "a\nb\n");
        let plan =
            plan_file(&src, &SplitMode::Lines { lines_per_file: 1 }, Some(&out)).expect("预览失败");
        assert!(plan.parts[0].overwrites);
        assert!(!plan.parts[1].overwrites);
        let applied = apply_file(&src, &SplitMode::Lines { lines_per_file: 1 }, Some(&out))
            .expect("拆分失败");
        assert_eq!(applied.files.len(), 2);
        assert_eq!(read_part(&out, "book-0001.txt"), "a\n");
    }

    /// 拆分数量超上限拒绝。
    #[test]
    fn too_many_parts_rejected() {
        let temp = tempfile::tempdir().expect("临时目录失败");
        let content = "z\n".repeat(super::MAX_PARTS + 5);
        let src = write_source(temp.path(), "many.txt", &content);
        let err = plan_file(&src, &SplitMode::Lines { lines_per_file: 1 }, None).unwrap_err();
        assert!(matches!(err, SplitError::TooManyParts(_)));
    }
}
