//! 过滤视图的只读扫描（P1-4 / D61）：按正则或字面量匹配行、可选隐藏空行。
//!
//! 约束：
//! - 仅面向只读会话（[`FileSession`]）；编辑模式不提供过滤视图（由前端约束）。
//! - 命中行号采用**显示行**语义（与阅读虚拟化一致；超长行的 8KB 分段不参与匹配）。
//! - 扫描为线性一趟；命中/扫描到上限即截断（返回 `truncated`），不阻塞超大文件。

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::textfile::session::FileSession;

/// 单次扫描的显示行上限（到上限即截断；防极端输入下过长耗时）。
pub const FILTER_SCAN_ROW_LIMIT: u64 = 2_000_000;
/// 命中行数量上限（`u64 × 5 万 ≈ 400KB`，内存保护）。
pub const FILTER_MAX_MATCHES: usize = 50_000;
/// 匹配表达式最大字符数（防止恶意超长表达式）。
pub const FILTER_QUERY_MAX_CHARS: usize = 512;
/// 每批取行的数量（512 与行索引检查点粒度一致）。
const SCAN_BATCH_ROWS: usize = 512;

/// 过滤条件（IPC 入参；camelCase 与前端一致）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterQuery {
    /// 匹配文本（空字符串 = 只按 `hide_empty` 过滤）
    pub text: String,
    /// 是否按正则表达式匹配
    pub regex: bool,
    /// 是否大小写敏感
    pub case_sensitive: bool,
    /// 是否隐藏空行（含纯空白行）
    pub hide_empty: bool,
}

/// 过滤错误。
#[derive(Debug, thiserror::Error)]
pub enum FilterError {
    /// 正则表达式非法
    #[error("正则表达式无效：{message}")]
    InvalidRegex {
        /// 来自正则引擎的编译错误说明
        message: String,
    },
    /// 匹配表达式过长
    #[error("匹配表达式过长（上限 {limit} 字符）")]
    QueryTooLong {
        /// 允许的最大字符数
        limit: usize,
    },
}

/// 过滤扫描结果（IPC 出参；camelCase）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterResult {
    /// 命中显示行号（升序；受命中上限截断）
    pub rows: Vec<u64>,
    /// 已扫描的显示行数（截断时小于总行数）
    pub scanned: u64,
    /// 是否被上限截断（命中满或扫描满）
    pub truncated: bool,
}

/// 扫描会话并返回命中行号。
///
/// 参数：`session` 只读会话；`query` 过滤条件。
/// 返回：`Ok(FilterResult)` 或 `Err(FilterError)`（表达式非法/过长）。
/// 边界：空文本 + `hide_empty=false` 视为「匹配全部行」（前端不把该组合当过滤使用）。
pub fn scan(session: &FileSession, query: &FilterQuery) -> Result<FilterResult, FilterError> {
    scan_with(session, query, FILTER_MAX_MATCHES, FILTER_SCAN_ROW_LIMIT)
}

/// 可注入上限的扫描核心（供单元测试验证截断行为）。
fn scan_with(
    session: &FileSession,
    query: &FilterQuery,
    max_matches: usize,
    scan_row_limit: u64,
) -> Result<FilterResult, FilterError> {
    if query.text.chars().count() > FILTER_QUERY_MAX_CHARS {
        return Err(FilterError::QueryTooLong {
            limit: FILTER_QUERY_MAX_CHARS,
        });
    }
    let matcher = build_matcher(query)?;
    let rows_total = session.rows_total();
    let scan_limit = rows_total.min(scan_row_limit);
    let mut rows: Vec<u64> = Vec::new();
    let mut scanned = 0u64;
    let mut start = 0u64;
    while start < scan_limit {
        let count = ((scan_limit - start) as usize).min(SCAN_BATCH_ROWS);
        let batch = session.rows(start, count);
        if batch.is_empty() {
            break;
        }
        let batch_len = batch.len() as u64;
        for row in &batch {
            scanned = row.row + 1;
            if matches_row(matcher.as_ref(), row.text.as_str(), query) {
                rows.push(row.row);
                if rows.len() >= max_matches {
                    return Ok(FilterResult {
                        rows,
                        scanned,
                        truncated: true,
                    });
                }
            }
        }
        start += batch_len;
    }
    let truncated = scan_limit < rows_total;
    Ok(FilterResult {
        rows,
        scanned,
        truncated,
    })
}

/// 行匹配：`hide_empty` 先排除空/纯空白行，再按可选的文本匹配器判断。
fn matches_row(matcher: Option<&Regex>, text: &str, query: &FilterQuery) -> bool {
    if query.hide_empty && text.trim().is_empty() {
        return false;
    }
    match matcher {
        None => true,
        Some(re) => re.is_match(text),
    }
}

/// 构建行匹配器：空文本 → `None`；字面量模式先 `regex::escape`；大小写按需。
fn build_matcher(query: &FilterQuery) -> Result<Option<Regex>, FilterError> {
    if query.text.is_empty() {
        return Ok(None);
    }
    let pattern = if query.regex {
        query.text.clone()
    } else {
        regex::escape(&query.text)
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!query.case_sensitive)
        .build()
        .map(Some)
        .map_err(|err| FilterError::InvalidRegex {
            message: err.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    /// 构造临时会话（UTF-8 内容，上限 100MB 足够测试）。
    fn session_with(content: &str) -> (tempfile::TempDir, FileSession) {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("filter.txt");
        let mut file = std::fs::File::create(&path).expect("创建文件失败");
        file.write_all(content.as_bytes()).expect("写入失败");
        let session = FileSession::open(&path, None, 100).expect("打开失败");
        (dir, session)
    }

    /// 构造字面量查询（默认大小写不敏感、不隐藏空行）。
    fn literal(text: &str) -> FilterQuery {
        FilterQuery {
            text: text.to_string(),
            regex: false,
            case_sensitive: false,
            hide_empty: false,
        }
    }

    /// 字面量默认大小写不敏感。
    #[test]
    fn literal_match_is_case_insensitive_by_default() {
        let (_dir, session) = session_with("Alpha\nbeta\nALPHA\ngamma");
        let result = scan(&session, &literal("alpha")).expect("扫描失败");
        assert_eq!(result.rows, vec![0, 2]);
        assert!(!result.truncated);
    }

    /// 大小写敏感开关。
    #[test]
    fn case_sensitive_matches_exact() {
        let (_dir, session) = session_with("Alpha\nbeta\nALPHA\ngamma");
        let mut query = literal("Alpha");
        query.case_sensitive = true;
        let result = scan(&session, &query).expect("扫描失败");
        assert_eq!(result.rows, vec![0]);
    }

    /// 正则模式。
    #[test]
    fn regex_mode_matches_pattern() {
        let (_dir, session) = session_with("Alpha\nbeta\nALPHA\ngamma");
        let mut query = literal("^b");
        query.regex = true;
        let result = scan(&session, &query).expect("扫描失败");
        assert_eq!(result.rows, vec![1]);
    }

    /// 非法正则报错（错误信息来自正则引擎）。
    #[test]
    fn invalid_regex_is_reported() {
        let (_dir, session) = session_with("a\nb");
        let mut query = literal("(");
        query.regex = true;
        assert!(matches!(
            scan(&session, &query),
            Err(FilterError::InvalidRegex { .. })
        ));
    }

    /// 空文本 + 隐藏空行：只保留非空行（空白行也算空）。
    #[test]
    fn hide_empty_only_keeps_non_empty_rows() {
        let (_dir, session) = session_with("a\n\n  \nb");
        let mut query = literal("");
        query.hide_empty = true;
        let result = scan(&session, &query).expect("扫描失败");
        assert_eq!(result.rows, vec![0, 3]);
    }

    /// 命中上限截断。
    #[test]
    fn match_limit_truncates() {
        let (_dir, session) = session_with("hit\nhit\nhit\nhit\nhit");
        let result =
            scan_with(&session, &literal("hit"), 3, FILTER_SCAN_ROW_LIMIT).expect("扫描失败");
        assert_eq!(result.rows.len(), 3);
        assert!(result.truncated);
    }

    /// 扫描行上限截断（用极小上限模拟超大文件）。
    #[test]
    fn scan_limit_truncates() {
        let (_dir, session) = session_with("hit\nhit\nhit\nhit\nhit");
        let result = scan_with(&session, &literal("hit"), FILTER_MAX_MATCHES, 2).expect("扫描失败");
        assert_eq!(result.rows, vec![0, 1]);
        assert!(result.truncated);
    }

    /// 超长表达式拒绝。
    #[test]
    fn overlong_query_is_rejected() {
        let (_dir, session) = session_with("a");
        let long = "x".repeat(FILTER_QUERY_MAX_CHARS + 1);
        assert!(matches!(
            scan(&session, &literal(&long)),
            Err(FilterError::QueryTooLong { .. })
        ));
    }
}
