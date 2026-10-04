//! 大纲提取（P2-6 V-09）：按可编辑正则扫描文档，产出章节列表。
//!
//! 语义（与 `docs/configuration.md` 的 `display.outlinePatterns` 保持同步）：
//! - 逐显示行匹配正则列表，**首个命中即视为章节**；
//! - 标题 = 行文本去除首尾空白后截断（120 字符，超出加省略号）；
//! - `level` 由行首缩进近似推导（空格 1 / Tab 与全角空格 2，每 2 单位 1 级，封顶 9）；
//! - 正则列表为空时由调用方（命令层）回退内置默认；
//! - 结果上限 [`OUTLINE_MAX_ITEMS`]，超出截断（长文足够导航使用）。
//!
//! 性能：按 512 行窗口 `fetch_rows` 批量取文（虚拟渲染同源），
//! 正则预编译一次，逐行 `is_match`；不整读文件。

use serde::Serialize;
use thiserror::Error;

use crate::textfile::source::DocumentSource;

/// 大纲条目（显示行坐标；`level` 供面板缩进展示）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineItem {
    /// 显示行号（0 基，与渲染/跳转坐标一致）
    pub row: u64,
    /// 章节标题（已截断）
    pub title: String,
    /// 缩进层级（0–9）
    pub level: u8,
}

/// 大纲提取错误。
#[derive(Debug, Error)]
pub enum OutlineError {
    /// 正则语法非法（`index` 为 1 基序号，与设置条目顺序一致）
    #[error("大纲正则语法非法（第 {index} 条）：{message}")]
    InvalidPattern {
        /// 条目序号（1 基）
        index: usize,
        /// 正则引擎报错原文
        message: String,
    },
}

/// 单次提取的章节数量上限。
pub const OUTLINE_MAX_ITEMS: usize = 5_000;
/// 折叠区间数量上限。
pub const FOLD_MAX_REGIONS: usize = 5_000;
/// 缩进折叠扫描的最大行数（防御性护栏）。
pub const FOLD_SCAN_MAX_ROWS: u64 = 1_000_000;
/// 批量取行窗口（与虚拟渲染一致的粒度）。
const ROW_BATCH: u64 = 512;
/// 标题截断长度（字符数）。
const TITLE_MAX_CHARS: usize = 120;

/// 提取大纲。
///
/// 参数：
/// - `source`：文档数据源（只读会话或编辑文档，显示行语义）；
/// - `patterns`：正则列表（调用方保证非空；空列表返回空结果）；
/// - `max_items`：条目上限（一般传 [`OUTLINE_MAX_ITEMS`]）。
///
/// 返回：按行号升序的章节列表；正则非法时报 [`OutlineError::InvalidPattern`]。
pub fn extract(
    source: &dyn DocumentSource,
    patterns: &[String],
    max_items: usize,
) -> Result<Vec<OutlineItem>, OutlineError> {
    if patterns.is_empty() || max_items == 0 {
        return Ok(Vec::new());
    }
    let compiled: Vec<regex::Regex> = patterns
        .iter()
        .enumerate()
        .map(|(index, pattern)| {
            regex::Regex::new(pattern).map_err(|error| OutlineError::InvalidPattern {
                index: index + 1,
                message: error.to_string(),
            })
        })
        .collect::<Result<_, _>>()?;

    let total = source.rows_total();
    let mut items: Vec<OutlineItem> = Vec::new();
    let mut start = 0u64;
    while start < total && items.len() < max_items {
        let count = ROW_BATCH.min(total - start) as usize;
        let rows = source.fetch_rows(start, count);
        if rows.is_empty() {
            break;
        }
        for row in &rows {
            if !compiled.iter().any(|re| re.is_match(&row.text)) {
                continue;
            }
            let title = normalize_title(&row.text);
            if title.is_empty() {
                continue;
            }
            items.push(OutlineItem {
                row: row.row,
                title,
                level: indent_level(&row.text),
            });
            if items.len() >= max_items {
                break;
            }
        }
        start += rows.len() as u64;
    }
    Ok(items)
}

/// 折叠区间（显示行坐标，闭区间；`start_row` 为可点击的折叠标记行）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldRegion {
    /// 折叠头部行
    pub start_row: u64,
    /// 折叠末行（含）
    pub end_row: u64,
}

/// 计算折叠区间。
///
/// 模式：
/// - `Off`：空；
/// - `Heading`/`Regex`：由大纲条目推导（层级使用栈：子项区间从其下一同级/更高级条目行前结束）；
/// - `Indent`：逐行扫描首行缩进（空格 1 / Tab 4 / 全角空格 2），缩进减小处闭合上一个区间。
pub fn fold_regions(
    source: &dyn DocumentSource,
    mode: crate::settings::display::FoldingMode,
    patterns: &[String],
) -> Result<Vec<FoldRegion>, OutlineError> {
    use crate::settings::display::FoldingMode;
    match mode {
        FoldingMode::Off | FoldingMode::Unknown => Ok(Vec::new()),
        FoldingMode::Heading | FoldingMode::Regex => {
            let items = extract(source, patterns, FOLD_MAX_REGIONS)?;
            Ok(regions_from_items(&items, source.rows_total()))
        }
        FoldingMode::Indent => Ok(indent_regions(source)),
    }
}

/// 由大纲条目推导折叠区间（层级栈；仅保留 start<end 的有效区间）。
fn regions_from_items(items: &[OutlineItem], total_rows: u64) -> Vec<FoldRegion> {
    let mut regions: Vec<FoldRegion> = Vec::new();
    // 栈中每项：（层级，头部行）
    let mut stack: Vec<(u8, u64)> = Vec::new();
    for item in items {
        while let Some((level, start)) = stack.last().copied() {
            if level >= item.level {
                stack.pop();
                if item.row > start + 1 {
                    regions.push(FoldRegion {
                        start_row: start,
                        end_row: item.row - 1,
                    });
                }
            } else {
                break;
            }
        }
        stack.push((item.level, item.row));
    }
    if !items.is_empty() && total_rows > 0 {
        let total = total_rows;
        for (level, start) in stack {
            let _ = level;
            if total > start + 1 {
                regions.push(FoldRegion {
                    start_row: start,
                    end_row: total - 1,
                });
            }
        }
    }
    regions.sort_by_key(|region| (region.start_row, region.end_row));
    regions
}

/// 缩进折叠：扫描全部行，按首行缩进拆分区间。
fn indent_regions(source: &dyn DocumentSource) -> Vec<FoldRegion> {
    let total = source.rows_total().min(FOLD_SCAN_MAX_ROWS);
    let mut regions: Vec<FoldRegion> = Vec::new();
    // 栈：（缩进单位，头部行）
    let mut stack: Vec<(u32, u64)> = Vec::new();
    let mut start = 0u64;
    while start < total && regions.len() < FOLD_MAX_REGIONS {
        let count = ROW_BATCH.min(total - start) as usize;
        let rows = source.fetch_rows(start, count);
        if rows.is_empty() {
            break;
        }
        for row in &rows {
            if row.text.trim().is_empty() {
                continue;
            }
            let indent = indent_units(&row.text);
            while let Some((top, head)) = stack.last().copied() {
                if indent <= top {
                    stack.pop();
                    if row.row > head + 1 {
                        regions.push(FoldRegion {
                            start_row: head,
                            end_row: row.row - 1,
                        });
                    }
                } else {
                    break;
                }
            }
            stack.push((indent, row.row));
        }
        start += rows.len() as u64;
    }
    // 收尾：剩余栈按扫描范围末尾闭合
    let last_scanned = total.saturating_sub(1);
    for (_, head) in stack {
        if last_scanned > head + 1 {
            regions.push(FoldRegion {
                start_row: head,
                end_row: last_scanned,
            });
        }
    }
    regions.sort_by_key(|region| (region.start_row, region.end_row));
    regions
}

/// 首行缩进单位：空格 1 / Tab 4 / 全角空格 2。
fn indent_units(text: &str) -> u32 {
    let mut units = 0;
    for ch in text.chars() {
        match ch {
            ' ' => units += 1,
            '\t' => units += 4,
            '\u{3000}' => units += 2,
            _ => break,
        }
    }
    units
}

/// 标题归一：去除首尾空白（含全角空格），截断到 [`TITLE_MAX_CHARS`] 并加省略号。
fn normalize_title(text: &str) -> String {
    let trimmed = text.trim_matches(|ch: char| ch.is_whitespace() || ch == '\u{3000}');
    let total = trimmed.chars().count();
    let mut title: String = trimmed.chars().take(TITLE_MAX_CHARS).collect();
    if total > TITLE_MAX_CHARS {
        title.push('…');
    }
    title
}

/// 行首缩进层级：空格 1、Tab/全角空格 2，每 2 单位 1 级（封顶 9）。
fn indent_level(text: &str) -> u8 {
    let mut units: u32 = 0;
    for ch in text.chars() {
        match ch {
            ' ' => units += 1,
            '\t' => units += 2,
            '\u{3000}' => units += 2,
            _ => break,
        }
    }
    (units / 2).min(9) as u8
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{extract, OutlineError, OUTLINE_MAX_ITEMS};
    use crate::settings::defaults::DEFAULT_OUTLINE_PATTERNS;
    use crate::textfile::session::FileSession;

    fn patterns() -> Vec<String> {
        DEFAULT_OUTLINE_PATTERNS
            .iter()
            .map(|pattern| (*pattern).to_string())
            .collect()
    }

    fn session_with(contents: &[u8]) -> (tempfile::TempDir, FileSession) {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("outline.txt");
        let mut file = std::fs::File::create(&path).expect("创建文件失败");
        file.write_all(contents).expect("写入失败");
        drop(file);
        let session = FileSession::open(&path, None, 100).expect("打开失败");
        (dir, session)
    }

    /// 中文章节：命中行提取、缩进推导层级、空行与非章节行忽略。
    #[test]
    fn chinese_chapters_with_indent_levels() {
        let (_dir, session) = session_with(
            "序言\n第一章 起点\n正文甲\n  第一节 小节\n正文乙\n\n第二章 终局\n".as_bytes(),
        );
        let items = extract(&session, &patterns(), OUTLINE_MAX_ITEMS).expect("提取失败");
        let rows: Vec<u64> = items.iter().map(|item| item.row).collect();
        assert_eq!(rows, vec![1, 3, 6]);
        assert_eq!(items[0].title, "第一章 起点");
        assert_eq!(items[0].level, 0);
        assert_eq!(items[1].title, "第一节 小节");
        assert_eq!(items[1].level, 1);
        assert_eq!(items[2].title, "第二章 终局");
    }

    /// 英文章节（Chapter/Part + 数字或罗马数字）命中。
    #[test]
    fn english_chapters_match() {
        let (_dir, session) =
            session_with("Intro\nChapter 3\nText\nPart IV\nMore\nChapter no-number\n".as_bytes());
        let items = extract(&session, &patterns(), OUTLINE_MAX_ITEMS).expect("提取失败");
        let titles: Vec<&str> = items.iter().map(|item| item.title.as_str()).collect();
        assert_eq!(titles, vec!["Chapter 3", "Part IV"]);
    }

    /// 非法正则报错并携带 1 基序号。
    #[test]
    fn invalid_pattern_reports_index() {
        let (_dir, session) = session_with(b"a\nb\n");
        let bad = vec!["^ok$".to_string(), "(unclosed".to_string()];
        let error = extract(&session, &bad, OUTLINE_MAX_ITEMS).expect_err("应报错");
        match error {
            OutlineError::InvalidPattern { index, .. } => assert_eq!(index, 2),
        }
    }

    /// 条目上限截断。
    #[test]
    fn items_are_capped() {
        let (_dir, session) = session_with("第一章 A\n第一章 B\n第一章 C\n第一章 D\n".as_bytes());
        let items = extract(&session, &patterns(), 2).expect("提取失败");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].row, 0);
        assert_eq!(items[1].row, 1);
    }

    /// 标题折叠区间：嵌套层级 + 末尾区间闭合到文件末行。
    #[test]
    fn heading_fold_regions() {
        use super::{fold_regions, FoldRegion};
        use crate::settings::display::FoldingMode;
        let (_dir, session) =
            session_with("第一章\n正文\n  第一节\n正文\n第二章\n正文\n".as_bytes());
        let regions = fold_regions(&session, FoldingMode::Heading, &patterns()).expect("失败");
        assert_eq!(
            regions,
            vec![
                FoldRegion {
                    start_row: 0,
                    end_row: 3,
                },
                FoldRegion {
                    start_row: 2,
                    end_row: 3,
                },
                FoldRegion {
                    start_row: 4,
                    end_row: 5,
                },
            ]
        );
    }

    /// 缩进折叠区间：缩进回退处闭合。
    #[test]
    fn indent_fold_regions() {
        use super::{fold_regions, FoldRegion};
        use crate::settings::display::FoldingMode;
        let (_dir, session) = session_with("根\n  子1\n  子2\n平级\n\t深\n".as_bytes());
        let regions = fold_regions(&session, FoldingMode::Indent, &[]).expect("失败");
        assert_eq!(
            regions,
            vec![FoldRegion {
                start_row: 0,
                end_row: 2,
            }]
        );
    }
}
