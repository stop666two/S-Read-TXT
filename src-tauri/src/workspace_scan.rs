//! 多文件（工作区）查找与替换的扫描辅助。
//!
//! 定位：只负责「跨标签扫描」的数据产出；标签遍历与替换执行在
//! `AppState`（需要锁内访问标签表）。
//!
//! 两套扫描路径：
//! - **编辑态标签**：复用编辑引擎 `find_request` 的流式扫描——跨块/跨片段、
//!   跨行正则、全词与超时语义与查找条完全一致（真值来源）；
//! - **只读标签**：按 512 行批量解码**逐行匹配**（行内匹配）——跨行正则
//!   （如 `a\nb`）在只读标签中不命中（与过滤视图同口径，界面已注明）；
//!   逐行匹配统一走正则引擎（字面量先 `regex::escape`），与编辑引擎的
//!   字面折叠在极冷门 Unicode 特例上可能存在差异（可接受，已注释）。
//!
//! 上限：单文件列举 [`WORKSPACE_FILE_MATCH_CAP`] 条（`total` 仍为完整计数，
//! 受 [`MATCH_COUNT_LIMIT`] 保护）；正则扫描受设置超时约束（超时保留已得结果
//! 并标记 `timed_out`）。

use std::time::{Duration, Instant};

use regex::{Regex, RegexBuilder};
use serde::Serialize;

use crate::textfile::editing::edit_doc::{EditDoc, EditError};
use crate::textfile::editing::search::{SearchMode, SearchRequest, MATCH_COUNT_LIMIT};
use crate::textfile::session::FileSession;

/// 单文件返回的命中条目上限（前端结果列表展示用；超出仅计数）。
pub const WORKSPACE_FILE_MATCH_CAP: usize = 200;

/// 只读标签逐行扫描的批量行数。
const ROW_BATCH: usize = 512;

/// 一条工作区命中（显示行 + 段内 UTF-16 坐标，与查找条坐标系一致）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceHit {
    /// 显示行号
    pub row: u64,
    /// 段内起始 UTF-16 偏移
    pub start_utf16: u64,
    /// 段内结束 UTF-16 偏移
    pub end_utf16: u64,
    /// 命中行文本摘要（去首尾空白、按字符截断 ≤160，列表展示用）
    pub preview: String,
}

/// 命中行文本摘要：去首尾空白 + 超长按字符截断（避免前端为列表再取行）。
fn preview_of(line: &str) -> String {
    const PREVIEW_CHARS: usize = 160;
    let trimmed = line.trim();
    let mut out = String::with_capacity(trimmed.len().min(PREVIEW_CHARS * 4));
    for (index, ch) in trimmed.chars().enumerate() {
        if index >= PREVIEW_CHARS {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

/// 单文件扫描结果。
#[derive(Debug, Clone, Default)]
pub struct FileScanOutcome {
    /// 命中列表（上限 `cap`，按行序）
    pub matches: Vec<WorkspaceHit>,
    /// 命中总数（完整计数，受 `MATCH_COUNT_LIMIT` 截断保护）
    pub total: u64,
    /// 总数达到上限被截断
    pub truncated: bool,
    /// 正则扫描超时（保留已得结果）
    pub timed_out: bool,
}

/// 编辑文档扫描（流式；与查找条一致的跨行/跨块语义）。
///
/// 返回 `Err` 仅限查询本身非法（非法正则等）；超时以 `timed_out` 标记返回。
pub fn scan_edit_doc(
    doc: &EditDoc,
    request: &SearchRequest<'_>,
    cap: usize,
) -> Result<FileScanOutcome, EditError> {
    let mut outcome = FileScanOutcome::default();
    let mut from: Option<(u64, u64)> = None;
    loop {
        match doc.find_request(request, from, None) {
            Ok(None) => break,
            Ok(Some(hit)) => {
                outcome.total += 1;
                if outcome.matches.len() < cap {
                    let preview = doc
                        .fetch_display_rows(hit.start_row, 1)
                        .first()
                        .map(|row| preview_of(&row.text))
                        .unwrap_or_default();
                    outcome.matches.push(WorkspaceHit {
                        row: hit.start_row,
                        start_utf16: hit.start_utf16,
                        end_utf16: hit.end_utf16,
                        preview,
                    });
                }
                if outcome.total as usize >= MATCH_COUNT_LIMIT {
                    outcome.truncated = true;
                    break;
                }
                from = Some((hit.end_row, hit.end_utf16));
            }
            Err(EditError::RegexTimeout) => {
                outcome.timed_out = true;
                break;
            }
            Err(err) => return Err(err),
        }
    }
    Ok(outcome)
}

/// 只读会话扫描（逐行匹配；跨行查询不命中，见模块文档）。
///
/// 返回 `Err(String)` 仅限查询非法（非法正则）或行解码异常（读取失败按空行跳过）。
pub fn scan_file_session(
    session: &FileSession,
    request: &SearchRequest<'_>,
    cap: usize,
) -> Result<FileScanOutcome, String> {
    let matcher = RowMatcher::build(request)?;
    let deadline = request
        .timeout_ms
        .map(|ms| Instant::now() + Duration::from_millis(u64::from(ms)));
    let mut outcome = FileScanOutcome::default();
    let total_rows = session.rows_total();
    let mut start = 0u64;
    while start < total_rows {
        if let Some(deadline) = deadline {
            if Instant::now() >= deadline {
                outcome.timed_out = true;
                break;
            }
        }
        let rows = session.rows(start, ROW_BATCH);
        if rows.is_empty() {
            break;
        }
        for row_text in &rows {
            for (start_utf16, end_utf16) in matcher.matches(&row_text.text) {
                outcome.total += 1;
                if outcome.matches.len() < cap {
                    outcome.matches.push(WorkspaceHit {
                        row: row_text.row,
                        start_utf16,
                        end_utf16,
                        preview: preview_of(&row_text.text),
                    });
                }
                if outcome.total as usize >= MATCH_COUNT_LIMIT {
                    outcome.truncated = true;
                    break;
                }
            }
            if outcome.truncated {
                break;
            }
        }
        if outcome.truncated {
            break;
        }
        start += rows.len() as u64;
    }
    Ok(outcome)
}

/// 行内匹配器（统一正则实现；字面量先转义，全词以 `\b(?:…)\b` 包裹）。
struct RowMatcher {
    regex: Regex,
}

impl RowMatcher {
    /// 按查找请求构造；非法正则返回可读错误消息。
    fn build(request: &SearchRequest<'_>) -> Result<Self, String> {
        let pattern = match request.mode {
            SearchMode::Literal => regex::escape(request.query),
            SearchMode::Regex => request.query.to_string(),
        };
        let pattern = if request.whole_word {
            format!("\\b(?:{pattern})\\b")
        } else {
            pattern
        };
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!request.case_sensitive)
            .multi_line(true)
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Self { regex })
    }

    /// 返回行内全部命中（UTF-16 偏移；零宽匹配跳过）。
    fn matches(&self, text: &str) -> Vec<(u64, u64)> {
        self.regex
            .find_iter(text)
            .filter(|matched| matched.start() != matched.end())
            .map(|matched| {
                let start = text[..matched.start()].encode_utf16().count() as u64;
                let end = text[..matched.end()].encode_utf16().count() as u64;
                (start, end)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在临时文件上打开只读会话（辅助）。
    fn session_with(dir: &std::path::Path, bytes: &[u8]) -> FileSession {
        let path = dir.join("sample.txt");
        std::fs::write(&path, bytes).expect("写样本失败");
        FileSession::open(&path, None, 2048).expect("打开样本失败")
    }

    fn request<'a>(query: &'a str, mode: SearchMode) -> SearchRequest<'a> {
        SearchRequest {
            query,
            case_sensitive: false,
            mode,
            whole_word: false,
            timeout_ms: None,
        }
    }

    /// 只读行内匹配：字面量、大小写不敏感与 UTF-16 偏移正确。
    #[test]
    fn session_scan_finds_literal_rows() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let session = session_with(tmp.path(), "alpha\nBETA beta\nгамма\n".as_bytes());
        let outcome = scan_file_session(&session, &request("beta", SearchMode::Literal), 100)
            .expect("扫描失败");
        assert_eq!(outcome.total, 2);
        assert_eq!(outcome.matches[0].row, 1);
        assert_eq!(outcome.matches[0].start_utf16, 0);
        assert_eq!(outcome.matches[1].start_utf16, 5);
    }

    /// 只读全词匹配生效（cat 不命中 cat-cat 的两个片段）。
    #[test]
    fn session_scan_whole_word() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let session = session_with(tmp.path(), "cat\nconcat\ncat-cat\n".as_bytes());
        let mut req = request("cat", SearchMode::Literal);
        req.whole_word = true;
        let outcome = scan_file_session(&session, &req, 100).expect("扫描失败");
        // `\b` 语义：连字符构成词边界，`cat-cat` 的两个片段均命中（与编辑引擎一致）。
        assert_eq!(outcome.total, 3, "第 0 行 1 处 + 第 2 行 2 处");
        assert_eq!(outcome.matches[2].row, 2);
    }

    /// 列表上限：条目截断但总数完整。
    #[test]
    fn session_scan_caps_list_but_counts_total() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let session = session_with(tmp.path(), "x\nx\nx\nx\n".as_bytes());
        let outcome =
            scan_file_session(&session, &request("x", SearchMode::Literal), 2).expect("扫描失败");
        assert_eq!(outcome.matches.len(), 2);
        assert_eq!(outcome.total, 4);
        assert!(!outcome.truncated);
    }

    /// 非法正则返回可读错误。
    #[test]
    fn session_scan_invalid_regex() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let session = session_with(tmp.path(), "abc\n".as_bytes());
        let outcome = scan_file_session(&session, &request("(", SearchMode::Regex), 10);
        assert!(outcome.is_err());
    }
}
