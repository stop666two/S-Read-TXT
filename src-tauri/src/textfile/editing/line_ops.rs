//! 常用行操作引擎。
//!
//! 设计：
//! - 预览与执行共用同一纯变换 `transform`（输入=范围跨度内的行文本，输出=变换后的行文本）；
//! - 范围内取「首行到末行」的连续跨度统一处理：非空行范围取首末非空行之间的区间
//!   （中间空行包含在内，内容类操作由 `skip_empty` 决定是否跳过空行）；
//! - 执行以「单区间替换」组装为一次 `apply_edits`（单撤销步）；
//! - 输出为空集合（如删除/清空）时改为删除「跨度 + 相邻换行」，避免残留空行；
//! - 行数上限与批量序号同口径（[`LINE_OP_MAX_ROWS`]），防止大文件内存峰值；
//! - 结构性操作（排序/去重/移动等）不做 `skip_empty` 过滤（语义为整块变换）；
//! - 内部分隔符统一用文档首选换行（`preferred_newline`，CRLF 文件不会被改写成 LF）。

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::textfile::editing::batch::{resolve_scope_rows, BatchError, BatchScope};
use crate::textfile::editing::edit_doc::{EditApplied, EditDoc, EditError, EditOp};

/// 行操作单次作用的行数上限（与批量序号同口径）。
pub const LINE_OP_MAX_ROWS: u64 = 200_000;
/// 预览默认条数（前端可覆盖）。
pub const DEFAULT_PREVIEW_LINES: usize = 10;

/// 行操作种类（26 种；见设置规范 §3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineOp {
    /// 选中块整体上移一行
    MoveUp,
    /// 选中块整体下移一行
    MoveDown,
    /// 复制选中块（重复一份紧随其后）
    Duplicate,
    /// 删除选中块
    Delete,
    /// 合并选中块为一行（直接拼接，不加分隔）
    Merge,
    /// 按分隔符拆行
    Split,
    /// 排序（模式见 [`LineSortOrder`]）
    Sort,
    /// 反转行序
    Reverse,
    /// 去重（规则见 [`LineOpConfig`]）
    Dedupe,
    /// 删除空行（trim 后为空）
    RemoveEmptyLines,
    /// 去除每行首尾空白
    TrimLines,
    /// 去除每行行尾空白
    TrimTrailingWhitespace,
    /// 增加缩进
    Indent,
    /// 减少缩进
    Outdent,
    /// 制表符转空格（仅行首缩进段）
    TabsToSpaces,
    /// 空格转制表符（仅行首缩进段，按缩进宽度对齐）
    SpacesToTabs,
    /// 大小写转换（模式见 [`CaseMode`]）
    Case,
    /// 全角/半角互转（ASCII 全角区 + 表意空格）
    WidthConvert,
    /// 行首插入文本
    PrependText,
    /// 行尾追加文本
    AppendText,
    /// 删除行首 N 个字符
    DeleteHead,
    /// 删除行尾 N 个字符
    DeleteTail,
    /// 按分隔符提取第 N 列（1 基；缺列取空）
    ExtractColumn,
    /// 分隔符互相转换（如 CSV ↔ TSV）
    DelimiterConvert,
    /// 连续空行折叠为单个空行
    CollapseEmptyLines,
    /// 确保文档以换行结尾（文件级操作，忽略范围）
    EnsureTrailingNewline,
}

impl Default for LineOp {
    fn default() -> Self {
        Self::TrimLines
    }
}

/// 排序模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineSortOrder {
    /// 字典序（Unicode 码点）
    Lex,
    /// 自然排序（数字段按数值）
    Natural,
    /// 按行长度（同长按字典序）
    Length,
    /// 随机（按 `sort_seed` 确定性洗牌；预览与执行一致）
    Random,
}

impl Default for LineSortOrder {
    fn default() -> Self {
        Self::Lex
    }
}

/// 去重规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DedupeMode {
    /// 保留首次出现
    KeepFirst,
    /// 保留末次出现
    KeepLast,
}

impl Default for DedupeMode {
    fn default() -> Self {
        Self::KeepFirst
    }
}

/// 缩进字符。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IndentStyle {
    /// 空格
    Spaces,
    /// 制表符
    Tab,
}

impl Default for IndentStyle {
    fn default() -> Self {
        Self::Spaces
    }
}

/// 大小写转换模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaseMode {
    /// 大写
    Upper,
    /// 小写
    Lower,
    /// 标题式（每个单词首字母大写、其余小写）
    Title,
}

impl Default for CaseMode {
    fn default() -> Self {
        Self::Lower
    }
}

/// 全角/半角方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WidthDirection {
    /// 全角转半角
    ToHalf,
    /// 半角转全角
    ToFull,
}

impl Default for WidthDirection {
    fn default() -> Self {
        Self::ToHalf
    }
}

/// 行操作参数（各操作只读取自己相关的字段；其余字段保留默认即可）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LineOpConfig {
    /// 操作种类
    pub op: LineOp,
    /// 作用范围（与批量序号共用语义）
    pub scope: BatchScope,
    /// 排序模式
    pub sort_order: LineSortOrder,
    /// 随机排序种子（前端生成；保证预览与执行结果一致）
    pub sort_seed: u64,
    /// 去重规则
    pub dedupe_mode: DedupeMode,
    /// 去重忽略大小写
    pub dedupe_ignore_case: bool,
    /// 去重模糊匹配（NFKC + 忽略空白）
    pub dedupe_fuzzy: bool,
    /// 缩进宽度（1–16）
    pub indent_width: u32,
    /// 缩进字符
    pub indent_style: IndentStyle,
    /// 大小写模式
    pub case_mode: CaseMode,
    /// 全半角方向
    pub width_direction: WidthDirection,
    /// 待插入/拆分行文本
    pub text: String,
    /// 列号（1 基）或删除字符数
    pub count: u32,
    /// 分隔符（拆分/提取列/转换源）
    pub delimiter: String,
    /// 转换目标分隔符
    pub delimiter_to: String,
    /// 内容类操作是否跳过空行
    pub skip_empty: bool,
    /// 预览条数
    pub preview_lines: usize,
}

impl Default for LineOpConfig {
    fn default() -> Self {
        Self {
            op: LineOp::TrimLines,
            scope: BatchScope::All,
            sort_order: LineSortOrder::Lex,
            sort_seed: 1,
            dedupe_mode: DedupeMode::KeepFirst,
            dedupe_ignore_case: false,
            dedupe_fuzzy: false,
            indent_width: 4,
            indent_style: IndentStyle::Spaces,
            case_mode: CaseMode::Lower,
            width_direction: WidthDirection::ToHalf,
            text: String::new(),
            count: 1,
            delimiter: "\t".to_string(),
            delimiter_to: ",".to_string(),
            skip_empty: false,
            preview_lines: DEFAULT_PREVIEW_LINES,
        }
    }
}

/// 行操作错误。
#[derive(Debug, thiserror::Error)]
pub enum LineOpError {
    /// 作用范围解析失败（复用批量序号错误，含越界/空范围/超上限）
    #[error(transparent)]
    Scope(#[from] BatchError),
    /// 缩进宽度非法
    #[error("缩进宽度须在 1–16 之间（当前 {width}）")]
    IndentWidthInvalid {
        /// 实际宽度
        width: u32,
    },
    /// 分隔符为空
    #[error("分隔符不能为空")]
    DelimiterEmpty,
    /// 列号非法
    #[error("列号须为 ≥ 1 的整数（当前 {index}）")]
    ColumnIndexInvalid {
        /// 实际列号
        index: u32,
    },
    /// 删除字符数非法
    #[error("删除字符数须为 ≥ 1 的整数（当前 {count}）")]
    CountInvalid {
        /// 实际数量
        count: u32,
    },
    /// 插入文本为空
    #[error("待插入文本不能为空")]
    TextEmpty,
    /// 编辑引擎错误（透传）
    #[error(transparent)]
    Edit(#[from] EditError),
}

/// 预览单行结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineOpPreviewItem {
    /// 结果行对应的显示行号（内容类操作与源行一致；结构性操作为结果顺序号）
    pub row: u64,
    /// 变换后的行文本
    pub text: String,
}

/// 预览结果。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineOpPreview {
    /// 涉及行数（跨度长度；无操作时为 0）
    pub affected_rows: u64,
    /// 预览是否被截断（结果多于 `preview_lines`）
    pub truncated: bool,
    /// 预览条目（前 `preview_lines` 条结果）
    pub items: Vec<LineOpPreviewItem>,
    /// 提示（无操作原因等；前端以 Toast 展示）
    pub warning: Option<String>,
}

/// 执行结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineOpOutcome {
    /// 涉及行数（无操作时为 0）
    pub affected: u64,
    /// 编辑结果（无操作时为 `None`）
    pub applied: Option<EditApplied>,
    /// 提示（无操作原因等）
    pub warning: Option<String>,
}

impl EditDoc {
    /// 预览行操作（不修改文档；校验与执行完全一致）。
    pub fn preview_line_op(&self, config: &LineOpConfig) -> Result<LineOpPreview, LineOpError> {
        validate(config)?;
        if matches!(config.op, LineOp::EnsureTrailingNewline) {
            return Ok(self.preview_trailing_newline());
        }
        let rows = resolve_scope_rows(self, &config.scope, false, LINE_OP_MAX_ROWS)?;
        let (first, last, warning) = self.plan_span(config, &rows)?;
        let input = self.span_texts(first, last);
        let output = if warning.is_some() {
            input.clone()
        } else {
            transform(&input, config)?
        };
        let affected = if output == input {
            0
        } else {
            input.len() as u64
        };
        let truncated = output.len() > config.preview_lines;
        let items = output
            .iter()
            .take(config.preview_lines)
            .enumerate()
            .map(|(index, text)| LineOpPreviewItem {
                row: first + index as u64,
                text: text.clone(),
            })
            .collect();
        Ok(LineOpPreview {
            affected_rows: affected,
            truncated,
            items,
            warning: warning.or_else(|| (affected == 0).then(|| "内容没有变化。".to_string())),
        })
    }

    /// 执行行操作（单次编辑 = 单撤销步）。
    pub fn apply_line_op(&mut self, config: &LineOpConfig) -> Result<LineOpOutcome, LineOpError> {
        validate(config)?;
        if matches!(config.op, LineOp::EnsureTrailingNewline) {
            return self.apply_trailing_newline();
        }
        let rows = resolve_scope_rows(self, &config.scope, false, LINE_OP_MAX_ROWS)?;
        let (first, last, warning) = self.plan_span(config, &rows)?;
        let input = self.span_texts(first, last);
        if warning.is_some() {
            return Ok(LineOpOutcome {
                affected: 0,
                applied: None,
                warning,
            });
        }
        let output = transform(&input, config)?;
        if output == input {
            return Ok(LineOpOutcome {
                affected: 0,
                applied: None,
                warning: Some("内容没有变化。".to_string()),
            });
        }
        let edit = self.build_span_edit(first, last, &output);
        let applied = self.apply_edits(&[edit])?;
        Ok(LineOpOutcome {
            affected: input.len() as u64,
            applied: Some(applied),
            warning: None,
        })
    }

    /// 计算最终跨度（含移动类操作的边界扩展与越界提示；已解析行集合需非空）。
    fn plan_span(
        &self,
        config: &LineOpConfig,
        rows: &[u64],
    ) -> Result<(u64, u64, Option<String>), LineOpError> {
        let total = self.rows_total();
        let first = rows[0];
        let last = *rows.last().expect("已解析行集合非空");
        match config.op {
            LineOp::MoveUp => {
                if first == 0 {
                    Ok((first, last, Some("已在文档开头，无法上移。".to_string())))
                } else {
                    Ok((first - 1, last, None))
                }
            }
            LineOp::MoveDown => {
                if last + 1 >= total {
                    Ok((first, last, Some("已在文档末尾，无法下移。".to_string())))
                } else {
                    Ok((first, last + 1, None))
                }
            }
            _ => Ok((first, last, None)),
        }
    }

    /// 读取跨度内的全部行文本。
    fn span_texts(&self, first: u64, last: u64) -> Vec<String> {
        (first..=last)
            .map(|row| self.row_text(row).unwrap_or_default())
            .collect()
    }

    /// 组装跨度替换编辑；输出为空时删除「跨度 + 相邻换行」，避免残留空行。
    fn build_span_edit(&self, first: u64, last: u64, output: &[String]) -> EditOp {
        let total = self.rows_total();
        let last_len = self
            .row_text(last)
            .map(|text| text.encode_utf16().count() as u64)
            .unwrap_or(0);
        if !output.is_empty() {
            return EditOp::Replace {
                start_row: first,
                start_utf16: 0,
                end_row: last,
                end_utf16: last_len,
                text: output.join(self.preferred_newline()),
            };
        }
        // 输出为空：优先连同后随换行一起删除；末尾无后随换行则连同前行换行删除
        if last + 1 < total {
            return EditOp::Delete {
                start_row: first,
                start_utf16: 0,
                end_row: last + 1,
                end_utf16: 0,
            };
        }
        if first > 0 {
            let prev_len = self
                .row_text(first - 1)
                .map(|text| text.encode_utf16().count() as u64)
                .unwrap_or(0);
            return EditOp::Delete {
                start_row: first - 1,
                start_utf16: prev_len,
                end_row: last,
                end_utf16: last_len,
            };
        }
        EditOp::Replace {
            start_row: first,
            start_utf16: 0,
            end_row: last,
            end_utf16: last_len,
            text: String::new(),
        }
    }

    /// 预览「确保文档以换行结尾」。
    fn preview_trailing_newline(&self) -> LineOpPreview {
        if self.has_trailing_newline() {
            return LineOpPreview {
                affected_rows: 0,
                truncated: false,
                items: Vec::new(),
                warning: Some("文档末尾已有换行。".to_string()),
            };
        }
        let last = self.rows_total().saturating_sub(1);
        let text = self.row_text(last).unwrap_or_default();
        LineOpPreview {
            affected_rows: 1,
            truncated: false,
            items: vec![LineOpPreviewItem { row: last, text }],
            warning: None,
        }
    }

    /// 执行「确保文档以换行结尾」。
    fn apply_trailing_newline(&mut self) -> Result<LineOpOutcome, LineOpError> {
        if self.has_trailing_newline() {
            return Ok(LineOpOutcome {
                affected: 0,
                applied: None,
                warning: Some("文档末尾已有换行。".to_string()),
            });
        }
        let last = self.rows_total().saturating_sub(1);
        let last_len = self
            .row_text(last)
            .map(|text| text.encode_utf16().count() as u64)
            .unwrap_or(0);
        let edit = EditOp::Insert {
            row: last,
            utf16: last_len,
            text: self.preferred_newline().to_string(),
        };
        let applied = self.apply_edits(&[edit])?;
        Ok(LineOpOutcome {
            affected: 1,
            applied: Some(applied),
            warning: None,
        })
    }
}

/// 参数校验（只校验与当前操作相关的字段）。
fn validate(config: &LineOpConfig) -> Result<(), LineOpError> {
    match config.op {
        LineOp::Indent | LineOp::Outdent | LineOp::TabsToSpaces | LineOp::SpacesToTabs => {
            if !(1..=16).contains(&config.indent_width) {
                return Err(LineOpError::IndentWidthInvalid {
                    width: config.indent_width,
                });
            }
        }
        LineOp::Split | LineOp::ExtractColumn | LineOp::DelimiterConvert => {
            if config.delimiter.is_empty() {
                return Err(LineOpError::DelimiterEmpty);
            }
            if matches!(config.op, LineOp::DelimiterConvert) && config.delimiter_to.is_empty() {
                return Err(LineOpError::DelimiterEmpty);
            }
            if matches!(config.op, LineOp::ExtractColumn) && config.count == 0 {
                return Err(LineOpError::ColumnIndexInvalid { index: 0 });
            }
        }
        LineOp::DeleteHead | LineOp::DeleteTail => {
            if config.count == 0 {
                return Err(LineOpError::CountInvalid { count: 0 });
            }
        }
        LineOp::PrependText | LineOp::AppendText => {
            if config.text.is_empty() {
                return Err(LineOpError::TextEmpty);
            }
        }
        _ => {}
    }
    Ok(())
}

/// 纯变换：输入行文本 → 输出行文本（预览与执行共用）。
fn transform(input: &[String], config: &LineOpConfig) -> Result<Vec<String>, LineOpError> {
    Ok(match config.op {
        LineOp::MoveUp => {
            let mut output = input[1..].to_vec();
            output.push(input[0].clone());
            output
        }
        LineOp::MoveDown => {
            let (last, head) = input.split_last().expect("跨度非空");
            let mut output = Vec::with_capacity(input.len());
            output.push(last.clone());
            output.extend(head.iter().cloned());
            output
        }
        LineOp::Duplicate => {
            let mut output = input.to_vec();
            output.extend(input.iter().cloned());
            output
        }
        LineOp::Delete => Vec::new(),
        LineOp::Merge => vec![input.concat()],
        LineOp::Split => {
            let mut output = Vec::new();
            for line in input {
                if config.skip_empty && line.trim().is_empty() {
                    output.push(line.clone());
                    continue;
                }
                output.extend(line.split(&config.delimiter).map(str::to_string));
            }
            output
        }
        LineOp::Sort => {
            let mut output = input.to_vec();
            match config.sort_order {
                LineSortOrder::Lex => output.sort(),
                LineSortOrder::Natural => output.sort_by(|a, b| natural_cmp(a, b)),
                LineSortOrder::Length => {
                    output.sort_by(|a, b| {
                        a.chars()
                            .count()
                            .cmp(&b.chars().count())
                            .then_with(|| a.cmp(b))
                    });
                }
                LineSortOrder::Random => output = shuffled(output, config.sort_seed),
            }
            output
        }
        LineOp::Reverse => input.iter().rev().cloned().collect(),
        LineOp::Dedupe => dedupe(input, config),
        LineOp::RemoveEmptyLines => input
            .iter()
            .filter(|line| !line.trim().is_empty())
            .cloned()
            .collect(),
        LineOp::TrimLines => map_rows(input, config, |line| line.trim().to_string()),
        LineOp::TrimTrailingWhitespace => {
            map_rows(input, config, |line| line.trim_end().to_string())
        }
        LineOp::Indent => map_rows(input, config, |line| match config.indent_style {
            IndentStyle::Tab => format!("\t{line}"),
            IndentStyle::Spaces => {
                let mut output = " ".repeat(config.indent_width as usize);
                output.push_str(line);
                output
            }
        }),
        LineOp::Outdent => map_rows(input, config, |line| outdent(line, config.indent_width)),
        LineOp::TabsToSpaces => map_rows(input, config, |line| {
            let width = config.indent_width as usize;
            let mut output = String::new();
            let mut consumed = 0usize;
            for ch in line.chars() {
                match ch {
                    '\t' => {
                        output.push_str(&" ".repeat(width));
                        consumed += 1;
                    }
                    ' ' => {
                        output.push(' ');
                        consumed += 1;
                    }
                    _ => break,
                }
            }
            output.push_str(&line[consumed..]);
            output
        }),
        LineOp::SpacesToTabs => map_rows(input, config, |line| {
            let width = config.indent_width as usize;
            let mut spaces = 0usize;
            for ch in line.chars() {
                if ch == ' ' {
                    spaces += 1;
                } else {
                    break;
                }
            }
            let mut output = "\t".repeat(spaces / width);
            output.push_str(&" ".repeat(spaces % width));
            output.push_str(&line[spaces..]);
            output
        }),
        LineOp::Case => match config.case_mode {
            CaseMode::Upper => map_rows(input, config, |line| line.to_uppercase()),
            CaseMode::Lower => map_rows(input, config, |line| line.to_lowercase()),
            CaseMode::Title => map_rows(input, config, |line| title_case(line)),
        },
        LineOp::WidthConvert => match config.width_direction {
            WidthDirection::ToHalf => map_rows(input, config, |line| to_half_width(line)),
            WidthDirection::ToFull => map_rows(input, config, |line| to_full_width(line)),
        },
        LineOp::PrependText => map_rows(input, config, |line| format!("{}{line}", config.text)),
        LineOp::AppendText => map_rows(input, config, |line| format!("{line}{}", config.text)),
        LineOp::DeleteHead => {
            let count = config.count as usize;
            map_rows(input, config, |line| line.chars().skip(count).collect())
        }
        LineOp::DeleteTail => {
            let count = config.count as usize;
            map_rows(input, config, |line| {
                let kept = line.chars().count().saturating_sub(count);
                line.chars().take(kept).collect()
            })
        }
        LineOp::ExtractColumn => {
            let index = config.count as usize - 1;
            map_rows(input, config, |line| {
                line.split(&config.delimiter)
                    .nth(index)
                    .unwrap_or("")
                    .to_string()
            })
        }
        LineOp::DelimiterConvert => map_rows(input, config, |line| {
            line.split(&config.delimiter)
                .collect::<Vec<_>>()
                .join(&config.delimiter_to)
        }),
        LineOp::CollapseEmptyLines => {
            let mut output: Vec<String> = Vec::with_capacity(input.len());
            let mut previous_empty = false;
            for line in input {
                let empty = line.trim().is_empty();
                if empty && previous_empty {
                    continue;
                }
                previous_empty = empty;
                output.push(line.clone());
            }
            output
        }
        // 文件级操作在 apply/preview 中单独处理
        LineOp::EnsureTrailingNewline => input.to_vec(),
    })
}

/// 内容类操作的统一映射（`skip_empty` 时空行原样保留）。
fn map_rows(input: &[String], config: &LineOpConfig, map: impl Fn(&str) -> String) -> Vec<String> {
    input
        .iter()
        .map(|line| {
            if config.skip_empty && line.trim().is_empty() {
                line.clone()
            } else {
                map(line)
            }
        })
        .collect()
}

/// 去重键（模糊模式：NFKC 规范化 + 去除空白；可选忽略大小写）。
fn dedupe_key(line: &str, config: &LineOpConfig) -> String {
    let mut key = if config.dedupe_fuzzy {
        line.nfkc().collect::<String>()
    } else {
        line.to_string()
    };
    if config.dedupe_fuzzy {
        key = key.chars().filter(|ch| !ch.is_whitespace()).collect();
    }
    if config.dedupe_ignore_case {
        key = key.to_lowercase();
    }
    key
}

/// 去重实现（保留首次 / 保留末次；保序）。
fn dedupe(input: &[String], config: &LineOpConfig) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    match config.dedupe_mode {
        DedupeMode::KeepFirst => input
            .iter()
            .filter(|line| seen.insert(dedupe_key(line, config)))
            .cloned()
            .collect(),
        DedupeMode::KeepLast => {
            let mut reversed = Vec::new();
            for line in input.iter().rev() {
                if seen.insert(dedupe_key(line, config)) {
                    reversed.push(line.clone());
                }
            }
            reversed.reverse();
            reversed
        }
    }
}

/// 自然排序比较：数字段按数值（忽略前导零；长度相同再字典序；再比原始长度）。
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let av: Vec<char> = a.chars().collect();
    let bv: Vec<char> = b.chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    while i < av.len() && j < bv.len() {
        let (ca, cb) = (av[i], bv[j]);
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let start_i = i;
            while i < av.len() && av[i].is_ascii_digit() {
                i += 1;
            }
            let start_j = j;
            while j < bv.len() && bv[j].is_ascii_digit() {
                j += 1;
            }
            let na: String = av[start_i..i].iter().collect();
            let nb: String = bv[start_j..j].iter().collect();
            let ta = na.trim_start_matches('0');
            let tb = nb.trim_start_matches('0');
            let ord = ta
                .len()
                .cmp(&tb.len())
                .then_with(|| ta.cmp(tb))
                .then_with(|| na.len().cmp(&nb.len()));
            if ord != Ordering::Equal {
                return ord;
            }
        } else {
            if ca != cb {
                return ca.cmp(&cb);
            }
            i += 1;
            j += 1;
        }
    }
    av.len().cmp(&bv.len())
}

/// 按种子确定性洗牌（xorshift64；Fisher–Yates）。
fn shuffled(mut items: Vec<String>, seed: u64) -> Vec<String> {
    let mut state = seed | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for i in (1..items.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
    items
}

/// 标题式：单词首字符大写、其余小写（非字母数字作为单词边界）。
fn title_case(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut at_word_start = true;
    for ch in line.chars() {
        if ch.is_alphanumeric() {
            if at_word_start {
                output.extend(ch.to_uppercase());
                at_word_start = false;
            } else {
                output.extend(ch.to_lowercase());
            }
        } else {
            output.push(ch);
            at_word_start = true;
        }
    }
    output
}

/// 减少缩进：优先去掉一个制表符，否则去掉最多 `width` 个前导空格。
fn outdent(line: &str, width: u32) -> String {
    if let Some(rest) = line.strip_prefix('\t') {
        return rest.to_string();
    }
    let mut removed = 0usize;
    for (index, ch) in line.char_indices() {
        if removed as u32 >= width || ch != ' ' {
            return line[index..].to_string();
        }
        removed += 1;
    }
    String::new()
}

/// 全角转半角（U+FF01–U+FF5E → ASCII；U+3000 → 空格）。
fn to_half_width(line: &str) -> String {
    line.chars()
        .map(|ch| match ch {
            '\u{3000}' => ' ',
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(u32::from(ch) - 0xFEE0).unwrap_or(ch),
            _ => ch,
        })
        .collect()
}

/// 半角转全角（ASCII 可打印 → U+FF01–U+FF5E；空格 → U+3000）。
fn to_full_width(line: &str) -> String {
    line.chars()
        .map(|ch| match ch {
            ' ' => '\u{3000}',
            '!'..='~' => char::from_u32(u32::from(ch) + 0xFEE0).unwrap_or(ch),
            _ => ch,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::textfile::editing::batch::BatchScope;

    use super::*;

    /// 用例文档：`content` 写入临时文件并以自动检测编码打开。
    fn open_doc(content: &str) -> (tempfile::TempDir, EditDoc) {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("lines.txt");
        std::fs::write(&path, content).expect("写入");
        let doc = EditDoc::open(&path, None, 100).expect("打开");
        (dir, doc)
    }

    /// 构造配置：仅指定操作，其余默认。
    fn config(op: LineOp) -> LineOpConfig {
        LineOpConfig {
            op,
            ..Default::default()
        }
    }

    /// 读取全部行文本（断言辅助）。
    fn rows(doc: &EditDoc) -> Vec<String> {
        (0..doc.rows_total())
            .map(|row| doc.row_text(row).unwrap_or_default())
            .collect()
    }

    /// 移动：块上移/下移；边界返回无操作提示。
    #[test]
    fn move_up_down_block_and_boundaries() {
        let (_dir, mut doc) = open_doc("a\nb\nc\nd\n");
        let mut cfg = config(LineOp::MoveDown);
        cfg.scope = BatchScope::RowRange { from: 1, to: 2 };
        let outcome = doc.apply_line_op(&cfg).expect("下移");
        assert_eq!(outcome.affected, 3);
        assert_eq!(rows(&doc), vec!["a", "d", "b", "c"]);
        doc.undo();
        let mut up = config(LineOp::MoveUp);
        up.scope = BatchScope::RowRange { from: 0, to: 0 };
        let boundary = doc.apply_line_op(&up).expect("上移边界");
        assert_eq!(boundary.affected, 0);
        assert!(boundary.warning.is_some());
        assert_eq!(rows(&doc), vec!["a", "b", "c", "d"]);
    }

    /// 复制与删除：删除不残留空行。
    #[test]
    fn duplicate_and_delete_no_residue() {
        let (_dir, mut doc) = open_doc("a\nb\nc\n");
        let mut dup = config(LineOp::Duplicate);
        dup.scope = BatchScope::RowRange { from: 1, to: 1 };
        doc.apply_line_op(&dup).expect("复制");
        assert_eq!(rows(&doc), vec!["a", "b", "b", "c"]);
        doc.undo();
        let mut del = config(LineOp::Delete);
        del.scope = BatchScope::RowRange { from: 1, to: 1 };
        doc.apply_line_op(&del).expect("删除");
        assert_eq!(rows(&doc), vec!["a", "c"]);
        // 删除后磁盘语义：无残留空行（行数为 2）
        assert_eq!(doc.rows_total(), 2);
    }

    /// 合并与拆分。
    #[test]
    fn merge_and_split() {
        let (_dir, mut doc) = open_doc("a\nb\nc\n");
        let mut merge = config(LineOp::Merge);
        merge.scope = BatchScope::RowRange { from: 0, to: 1 };
        doc.apply_line_op(&merge).expect("合并");
        assert_eq!(rows(&doc), vec!["ab", "c"]);
        doc.undo();
        let mut split = config(LineOp::Split);
        split.text = ",".to_string();
        split.delimiter = ",".to_string();
        doc.apply_edits(&[EditOp::Replace {
            start_row: 0,
            start_utf16: 0,
            end_row: 0,
            end_utf16: 1,
            text: "x,y".to_string(),
        }])
        .expect("预置逗号行");
        doc.apply_line_op(&split).expect("拆分");
        assert_eq!(rows(&doc), vec!["x", "y", "b", "c"]);
    }

    /// 排序三模式 + 随机确定性。
    #[test]
    fn sort_modes_and_random_seed() {
        let (_dir, mut doc) = open_doc("b10\na2\na10\nc\n");
        let mut lex = config(LineOp::Sort);
        lex.sort_order = LineSortOrder::Lex;
        doc.apply_line_op(&lex).expect("字典序");
        assert_eq!(rows(&doc), vec!["a10", "a2", "b10", "c"]);
        doc.undo();
        let mut natural = config(LineOp::Sort);
        natural.sort_order = LineSortOrder::Natural;
        doc.apply_line_op(&natural).expect("自然排序");
        assert_eq!(rows(&doc), vec!["a2", "a10", "b10", "c"]);
        doc.undo();
        let mut by_len = config(LineOp::Sort);
        by_len.sort_order = LineSortOrder::Length;
        doc.apply_line_op(&by_len).expect("按长度");
        assert_eq!(rows(&doc), vec!["c", "a2", "a10", "b10"]);
        doc.undo();
        let mut random = config(LineOp::Sort);
        random.sort_order = LineSortOrder::Random;
        random.sort_seed = 42;
        let preview = doc.preview_line_op(&random).expect("预览");
        doc.apply_line_op(&random).expect("随机排序");
        // 同一 seed 的预览与执行一致（且是原集合的排列）
        let applied: Vec<String> = preview.items.iter().map(|item| item.text.clone()).collect();
        assert_eq!(rows(&doc), applied);
        let mut sorted = rows(&doc);
        sorted.sort();
        assert_eq!(sorted, vec!["a10", "a2", "b10", "c"]);
    }

    /// 去重：保留首次/末次、忽略大小写、模糊（NFKC + 忽略空白）。
    #[test]
    fn dedupe_modes() {
        let (_dir, mut doc) = open_doc("A\na\nB\nb\n");
        let mut first = config(LineOp::Dedupe);
        first.dedupe_ignore_case = true;
        doc.apply_line_op(&first).expect("保留首次+忽略大小写");
        assert_eq!(rows(&doc), vec!["A", "B"]);
        doc.undo();
        let mut last = config(LineOp::Dedupe);
        last.dedupe_mode = DedupeMode::KeepLast;
        last.dedupe_ignore_case = true;
        doc.apply_line_op(&last).expect("保留末次");
        assert_eq!(rows(&doc), vec!["a", "b"]);
        doc.undo();
        // 模糊：全角 Ａ 与 A、含空格与不含空格视为重复
        let (_dir2, mut doc2) = open_doc("Ａ\nA\n x \nx\n");
        let mut fuzzy = config(LineOp::Dedupe);
        fuzzy.dedupe_fuzzy = true;
        doc2.apply_line_op(&fuzzy).expect("模糊去重");
        assert_eq!(rows(&doc2), vec!["Ａ", " x "]);
    }

    /// 空行处理：删除空行、折叠连续空行。
    #[test]
    fn empty_line_ops() {
        let (_dir, mut doc) = open_doc("a\n\n\nb\n\n");
        let mut collapse = config(LineOp::CollapseEmptyLines);
        collapse.scope = BatchScope::All;
        doc.apply_line_op(&collapse).expect("折叠");
        assert_eq!(rows(&doc), vec!["a", "", "b", ""]);
        doc.undo();
        let remove = config(LineOp::RemoveEmptyLines);
        doc.apply_line_op(&remove).expect("删除空行");
        assert_eq!(rows(&doc), vec!["a", "b"]);
    }

    /// 缩进/反缩进往返；Tab 与空格互转。
    #[test]
    fn indent_roundtrip_and_tab_conversion() {
        let (_dir, mut doc) = open_doc("a\n\tb\n");
        let mut indent = config(LineOp::Indent);
        indent.indent_width = 2;
        doc.apply_line_op(&indent).expect("缩进");
        assert_eq!(rows(&doc), vec!["  a", "  \tb"]);
        doc.undo();
        let mut outdent = config(LineOp::Outdent);
        outdent.indent_width = 1;
        doc.apply_line_op(&outdent).expect("反缩进");
        assert_eq!(rows(&doc), vec!["a", "b"]);

        let (_dir2, mut doc2) = open_doc("\t\ta\n");
        let mut to_spaces = config(LineOp::TabsToSpaces);
        to_spaces.indent_width = 4;
        doc2.apply_line_op(&to_spaces).expect("Tab→空格");
        assert_eq!(rows(&doc2), vec!["        a"]);
        doc2.undo();
        let mut to_tabs = config(LineOp::SpacesToTabs);
        to_tabs.indent_width = 4;
        doc2.apply_line_op(&to_tabs).expect("空格→Tab");
        assert_eq!(rows(&doc2), vec!["\t\ta"]);
    }

    /// 大小写：上/下/标题式。
    #[test]
    fn case_ops() {
        let (_dir, mut doc) = open_doc("hello wORLD\n");
        let mut upper = config(LineOp::Case);
        upper.case_mode = CaseMode::Upper;
        doc.apply_line_op(&upper).expect("大写");
        assert_eq!(rows(&doc), vec!["HELLO WORLD"]);
        doc.undo();
        let mut title = config(LineOp::Case);
        title.case_mode = CaseMode::Title;
        doc.apply_line_op(&title).expect("标题式");
        assert_eq!(rows(&doc), vec!["Hello World"]);
    }

    /// 全半角互转往返。
    #[test]
    fn width_conversion_roundtrip() {
        let (_dir, mut doc) = open_doc("ABC 123\n");
        let mut to_full = config(LineOp::WidthConvert);
        to_full.width_direction = WidthDirection::ToFull;
        doc.apply_line_op(&to_full).expect("转全角");
        assert_eq!(rows(&doc), vec!["ＡＢＣ　１２３"]);
        let mut to_half = config(LineOp::WidthConvert);
        to_half.width_direction = WidthDirection::ToHalf;
        doc.apply_line_op(&to_half).expect("转半角");
        assert_eq!(rows(&doc), vec!["ABC 123"]);
    }

    /// 行首/行尾增删。
    #[test]
    fn prepend_append_delete_ops() {
        let (_dir, mut doc) = open_doc("abc\ndef\n");
        let mut prepend = config(LineOp::PrependText);
        prepend.text = "> ".to_string();
        doc.apply_line_op(&prepend).expect("行首插入");
        assert_eq!(rows(&doc), vec!["> abc", "> def"]);
        doc.undo();
        let mut append = config(LineOp::AppendText);
        append.text = "!".to_string();
        doc.apply_line_op(&append).expect("行尾追加");
        assert_eq!(rows(&doc), vec!["abc!", "def!"]);
        doc.undo();
        let mut head = config(LineOp::DeleteHead);
        head.count = 1;
        doc.apply_line_op(&head).expect("删行首");
        assert_eq!(rows(&doc), vec!["bc", "ef"]);
        doc.undo();
        let mut tail = config(LineOp::DeleteTail);
        tail.count = 2;
        doc.apply_line_op(&tail).expect("删行尾");
        assert_eq!(rows(&doc), vec!["a", "d"]);
        // 空文本报错
        let empty = config(LineOp::PrependText);
        assert!(matches!(
            doc.preview_line_op(&empty),
            Err(LineOpError::TextEmpty)
        ));
    }

    /// 列提取与分隔符转换（含缺列取空、1 基列号）。
    #[test]
    fn column_and_delimiter_ops() {
        let (_dir, mut doc) = open_doc("a,b,c\nx,y\n");
        let mut extract = config(LineOp::ExtractColumn);
        extract.delimiter = ",".to_string();
        extract.count = 2;
        doc.apply_line_op(&extract).expect("提取第 2 列");
        assert_eq!(rows(&doc), vec!["b", "y"]);
        doc.undo();
        let mut missing = config(LineOp::ExtractColumn);
        missing.delimiter = ",".to_string();
        missing.count = 3;
        doc.apply_line_op(&missing).expect("提取越界列");
        assert_eq!(rows(&doc), vec!["c", ""]);
        doc.undo();
        let mut convert = config(LineOp::DelimiterConvert);
        convert.delimiter = ",".to_string();
        convert.delimiter_to = "\t".to_string();
        doc.apply_line_op(&convert).expect("CSV→TSV");
        assert_eq!(rows(&doc), vec!["a\tb\tc", "x\ty"]);
        // 空分隔符报错
        let mut bad = config(LineOp::DelimiterConvert);
        bad.delimiter = String::new();
        assert!(matches!(
            doc.preview_line_op(&bad),
            Err(LineOpError::DelimiterEmpty)
        ));
    }

    /// 确保换行结尾：一次补齐、二次提示。
    #[test]
    fn trailing_newline_op() {
        let (_dir, mut doc) = open_doc("a\nb");
        let cfg = config(LineOp::EnsureTrailingNewline);
        let outcome = doc.apply_line_op(&cfg).expect("补齐换行");
        assert_eq!(outcome.affected, 1);
        let second = doc.apply_line_op(&cfg).expect("二次");
        assert_eq!(second.affected, 0);
        assert!(second.warning.is_some());
    }

    /// CRLF 文档：替换后新增文本使用 CRLF（不被改写成 LF）。
    #[test]
    fn crlf_style_preserved() {
        let (_dir, mut doc) = open_doc("a\r\nb\r\n");
        let mut dup = config(LineOp::Duplicate);
        dup.scope = BatchScope::RowRange { from: 0, to: 0 };
        doc.apply_line_op(&dup).expect("复制");
        assert_eq!(rows(&doc), vec!["a", "a", "b"]);
        let added = String::from_utf8_lossy(doc.added()).to_string();
        assert!(added.contains("\r\n"), "新增文本应使用 CRLF：{added:?}");
    }

    /// 范围越界 / 空范围错误透传。
    #[test]
    fn scope_errors_pass_through() {
        let (_dir, doc) = open_doc("a\nb\n");
        let mut cfg = config(LineOp::Reverse);
        cfg.scope = BatchScope::RowRange { from: 5, to: 9 };
        assert!(matches!(
            doc.preview_line_op(&cfg),
            Err(LineOpError::Scope(_))
        ));
    }

    /// 无变化时返回 affected=0 + 提示。
    #[test]
    fn no_change_reports_warning() {
        let (_dir, doc) = open_doc("a\nb\nc\n");
        let cfg = config(LineOp::Sort);
        let preview = doc.preview_line_op(&cfg).expect("预览");
        assert_eq!(preview.affected_rows, 0);
        assert!(preview.warning.is_some());
    }
}
