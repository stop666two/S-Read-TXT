//! 批量插入/序号：按范围与格式为选中行生成行首/行尾序号。
//!
//! 设计要点：
//! - 逻辑行坐标（显示分段不参与编号）；范围支持 全文 / 当前行 / 行区间 / 选区 / 非空行；
//! - 10 种内置格式（阿拉伯 / 补零 / 中文小写 / 中文大写 / 括号 / 方括号 / 带圈 / 实心带圈 / 罗马 / 罗马小写）；
//! - 超范围保护：带圈超 20、罗马超 3999、中文大数超 9999 一律**报错中止**（预览时即暴露）；
//! - 模板变量：`{n} {total} {line} {date} {time} {filename}`；未知变量报错；
//! - 全部插入合为**单次编辑**（一次撤销还原）；行数上限 [`BATCH_MAX_ROWS`] 防失控；
//! - 预览与执行共用同一渲染路径，保证「所见即所得」。

use serde::{Deserialize, Serialize};

use crate::textfile::editing::edit_doc::{EditApplied, EditDoc, EditError, EditOp};

/// 单次批量操作的最大行数（超出报错；异步分片待实现）
pub const BATCH_MAX_ROWS: u64 = 200_000;

/// 序号格式（10 种内置）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NumberFormat {
    /// 阿拉伯数字：1 2 3
    Arabic,
    /// 补零：01 02 03（宽度可配 2–10）
    ZeroPad,
    /// 中文小写：一 二 三
    ChineseLower,
    /// 中文大写：壹 贰 叁
    ChineseUpper,
    /// 括号数字：(1) (2)
    Parenthesized,
    /// 方括号：[1] [2]
    Bracketed,
    /// 带圈：① ②（≤20）
    Circled,
    /// 实心带圈：❶ ❷（≤20）
    CircledFilled,
    /// 罗马大写：I II III（≤3999）
    RomanUpper,
    /// 罗马小写：i ii iii（≤3999）
    RomanLower,
}

/// 插入位置（相对于行文本）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsertPosition {
    /// 行首（序号 + 分隔符 + 后缀 + 原文）
    LineStart,
    /// 行尾（原文 + 空格 + 序号串）
    LineEnd,
}

/// 作用范围（逻辑行坐标；标签与前端一致）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BatchScope {
    /// 全文
    All,
    /// 当前行
    CurrentLine {
        /// 行号（0 基）
        row: u64,
    },
    /// 行区间 `[from, to]`（闭区间，0 基）
    RowRange {
        /// 起始行
        from: u64,
        /// 结束行（含）
        to: u64,
    },
    /// 仅非空行（全文范围内）
    NonEmpty,
    /// 选区涉及的行区间 `[from, to]`（闭区间；由前端按选区换算）
    Selection {
        /// 起始行
        from: u64,
        /// 结束行（含）
        to: u64,
    },
}

impl Default for BatchScope {
    /// 默认全文（用于配置反序列化缺省）。
    fn default() -> Self {
        Self::All
    }
}

/// 批量序号配置（IPC 入参；camelCase）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchNumberingConfig {
    /// 序号格式
    pub format: NumberFormat,
    /// 起始序号（≥1）
    pub start: u64,
    /// 步长（≥1）
    pub step: u64,
    /// 补零宽度（仅 `ZeroPad` 生效，2–10）
    #[serde(default = "default_zero_pad_width")]
    pub zero_pad_width: u32,
    /// 序号与后缀之间的分隔符（默认 `.`）
    #[serde(default = "default_separator")]
    pub separator: String,
    /// 行尾后缀（默认空；与分隔符拼接在序号之后）
    #[serde(default)]
    pub suffix: String,
    /// 插入位置
    pub position: InsertPosition,
    /// 作用范围
    pub scope: BatchScope,
    /// 跳过空行（默认开）
    #[serde(default = "default_true")]
    pub skip_empty: bool,
    /// 自定义模板（含变量；设置后忽略 分隔符/后缀）
    #[serde(default)]
    pub template: Option<String>,
    /// 预览行数（1–200；执行时忽略）
    #[serde(default = "default_preview_lines")]
    pub preview_lines: u32,
}

fn default_zero_pad_width() -> u32 {
    2
}
fn default_separator() -> String {
    ".".to_string()
}
fn default_true() -> bool {
    true
}
fn default_preview_lines() -> u32 {
    10
}

/// 批量错误（中文消息；供 IPC 与日志）
#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    /// 带圈格式超出上限
    #[error("带圈序号最大支持 20（当前序号 {value}），请调整起始值或改用其他格式")]
    CircledOverflow {
        /// 超限的序号
        value: u64,
    },
    /// 罗马数字超出上限
    #[error("罗马数字最大支持 3999（当前序号 {value}），请调整起始值或改用其他格式")]
    RomanOverflow {
        /// 超限的序号
        value: u64,
    },
    /// 中文数字超出上限
    #[error("中文数字最大支持 9999（当前序号 {value}），请调整起始值或改用其他格式")]
    ChineseTooLarge {
        /// 超限的序号
        value: u64,
    },
    /// 补零宽度非法
    #[error("补零宽度须在 2–10 之间（当前 {width}）")]
    ZeroPadWidthInvalid {
        /// 非法宽度
        width: u32,
    },
    /// 模板非法
    #[error("模板格式非法：{message}")]
    InvalidTemplate {
        /// 原因
        message: String,
    },
    /// 模板变量未知
    #[error("模板变量未知：{{{name}}}（可用：n / total / line / date / time / filename）")]
    UnknownTemplateVar {
        /// 变量名
        name: String,
    },
    /// 行区间非法
    #[error("行区间无效：{message}")]
    RowRangeInvalid {
        /// 原因
        message: String,
    },
    /// 行数超上限
    #[error("本次将处理 {count} 行，超过单次上限 {limit} 行，请缩小范围")]
    TooManyRows {
        /// 行数
        count: u64,
        /// 上限
        limit: u64,
    },
    /// 范围内没有可处理的行
    #[error("所选范围内没有可处理的行（已跳过空行或范围为空）")]
    EmptyScope,
    /// 起始序号/步长非法
    #[error("起始序号与步长必须 ≥ 1")]
    InvalidRange,
    /// 编辑引擎错误
    #[error(transparent)]
    Edit(#[from] EditError),
}

/// 预览条目
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchPreviewItem {
    /// 行号（0 基）
    pub row: u64,
    /// 插入后行文本（示意，不含换行）
    pub after: String,
    /// 该行将插入的序号文本
    pub insert_text: String,
}

/// 预览结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchPreview {
    /// 预览条目（前 N 条）
    pub items: Vec<BatchPreviewItem>,
    /// 将被处理的总行数
    pub total_rows: u64,
    /// 是否截断（true = 仅展示前 N 条）
    pub truncated: bool,
}

/// 执行结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchNumberingOutcome {
    /// 引擎编辑结果
    pub applied: EditApplied,
    /// 处理的行数
    pub affected: u64,
}

/// 渲染上下文（模板变量取值）
struct RenderContext<'a> {
    number: u64,
    total: u64,
    line: u64,
    date: &'a str,
    time: &'a str,
    filename: &'a str,
}

impl EditDoc {
    /// 预览批量序号：返回前 N 条示意与总行数；格式超限在预览时即报错。
    pub fn preview_batch_numbering(
        &self,
        config: &BatchNumberingConfig,
    ) -> Result<BatchPreview, BatchError> {
        let rows = self.batch_target_rows(config)?;
        let (date, time) = now_date_time();
        let filename = self
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let limit = config.preview_lines.clamp(1, 200) as usize;
        validate_capacity(config, rows.len() as u64)?;
        let mut items = Vec::new();
        for (index, row) in rows.iter().enumerate().take(limit) {
            let number = config.start + index as u64 * config.step;
            let insert_text = render_insert(
                config,
                number,
                rows.len() as u64,
                *row,
                &filename,
                &date,
                &time,
            )?;
            let text = self.row_text(*row).unwrap_or_default();
            let text = text.strip_suffix('\r').unwrap_or(&text).to_string();
            let after = match config.position {
                InsertPosition::LineStart => format!("{insert_text}{text}"),
                InsertPosition::LineEnd => format!("{text} {insert_text}"),
            };
            items.push(BatchPreviewItem {
                row: *row,
                after,
                insert_text,
            });
        }
        Ok(BatchPreview {
            items,
            total_rows: rows.len() as u64,
            truncated: rows.len() > limit,
        })
    }

    /// 执行批量序号（单次编辑 = 单撤销步）。
    pub fn apply_batch_numbering(
        &mut self,
        config: &BatchNumberingConfig,
    ) -> Result<BatchNumberingOutcome, BatchError> {
        Ok(self
            .apply_batch_numbering_cancellable(config, &|| false, &mut |_, _| {})?
            .expect("不取消时必然返回执行结果"))
    }

    /// 可取消并上报进度的批量序号；`Ok(None)` = 已取消（文档完整回滚、无撤销步骤）。
    pub fn apply_batch_numbering_cancellable(
        &mut self,
        config: &BatchNumberingConfig,
        cancel: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Option<BatchNumberingOutcome>, BatchError> {
        let rows = self.batch_target_rows(config)?;
        validate_capacity(config, rows.len() as u64)?;
        let (date, time) = now_date_time();
        let filename = self
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut ops = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            if index % 512 == 0 && cancel() {
                return Ok(None);
            }
            let number = config.start + index as u64 * config.step;
            let insert_text = render_insert(
                config,
                number,
                rows.len() as u64,
                *row,
                &filename,
                &date,
                &time,
            )?;
            let utf16 = match config.position {
                InsertPosition::LineStart => 0,
                InsertPosition::LineEnd => {
                    let text = self.row_text(*row).unwrap_or_default();
                    let text = text.strip_suffix('\r').unwrap_or(&text);
                    text.chars().map(|c| c.len_utf16() as u64).sum()
                }
            };
            // 行尾插入统一补前导空格：原文 + " " + 序号串（与预览完全一致）
            let text = match config.position {
                InsertPosition::LineStart => insert_text,
                InsertPosition::LineEnd => format!(" {insert_text}"),
            };
            ops.push(EditOp::Insert {
                row: *row,
                utf16,
                text,
            });
        }
        let applied = self.apply_edits_cancellable(&ops, cancel, progress)?;
        Ok(applied.map(|applied| BatchNumberingOutcome {
            applied,
            affected: rows.len() as u64,
        }))
    }

    /// 计算目标行集合（校验范围、应用跳过规则、执行上限保护）。
    fn batch_target_rows(&self, config: &BatchNumberingConfig) -> Result<Vec<u64>, BatchError> {
        if config.start == 0 || config.step == 0 {
            return Err(BatchError::InvalidRange);
        }
        if matches!(config.format, NumberFormat::ZeroPad)
            && !(2..=10).contains(&config.zero_pad_width)
        {
            return Err(BatchError::ZeroPadWidthInvalid {
                width: config.zero_pad_width,
            });
        }
        resolve_scope_rows(self, &config.scope, config.skip_empty, BATCH_MAX_ROWS)
    }
}

/// 解析作用范围为具体行集合（批量序号与行操作共用）。
///
/// 语义：
/// - `All` / `NonEmpty`：全文件（后者仅由调用方按需过滤空行）；
/// - `CurrentLine`：单行；
/// - `RowRange` / `Selection`：闭区间（`to` 超出文档时收敛到末行）；
/// - `skip_empty`：过滤 trim 后为空的行；
/// - `max_rows`：行数上限保护（超出报 [`BatchError::TooManyRows`]）；
/// - 结果为空报 [`BatchError::EmptyScope`]。
pub(crate) fn resolve_scope_rows(
    doc: &EditDoc,
    scope: &BatchScope,
    skip_empty: bool,
    max_rows: u64,
) -> Result<Vec<u64>, BatchError> {
    let total = doc.rows_total();
    let (from, to) = match *scope {
        BatchScope::All | BatchScope::NonEmpty => (0, total.saturating_sub(1)),
        BatchScope::CurrentLine { row } => {
            if row >= total {
                return Err(BatchError::RowRangeInvalid {
                    message: format!("当前行 {row} 超出文档行数 {total}"),
                });
            }
            (row, row)
        }
        BatchScope::RowRange { from, to } | BatchScope::Selection { from, to } => {
            if from > to {
                return Err(BatchError::RowRangeInvalid {
                    message: format!("起始行 {from} 大于结束行 {to}"),
                });
            }
            if from >= total {
                return Err(BatchError::RowRangeInvalid {
                    message: format!("起始行 {from} 超出文档行数 {total}"),
                });
            }
            (from, to.min(total - 1))
        }
    };
    let mut rows = Vec::new();
    if skip_empty {
        // 单次扫描取候选行文本（逐行 row_text 在大文件上是 O(行×文件) 的重扫）
        let candidates: Vec<u64> = (from..=to).collect();
        let texts = doc.texts_for_rows(&candidates);
        for (index, text) in texts.iter().enumerate() {
            if text.trim().is_empty() {
                continue;
            }
            rows.push(candidates[index]);
            if rows.len() as u64 > max_rows {
                return Err(BatchError::TooManyRows {
                    count: rows.len() as u64,
                    limit: max_rows,
                });
            }
        }
    } else {
        rows.extend(from..=to);
        if rows.len() as u64 > max_rows {
            return Err(BatchError::TooManyRows {
                count: rows.len() as u64,
                limit: max_rows,
            });
        }
    }
    if rows.is_empty() {
        return Err(BatchError::EmptyScope);
    }
    Ok(rows)
}

/// 容量预检：带圈/罗马/中文格式按「首个超限序号」提前报错（预览与执行共用）。
fn validate_capacity(config: &BatchNumberingConfig, count: u64) -> Result<(), BatchError> {
    let cap = match config.format {
        NumberFormat::ChineseLower | NumberFormat::ChineseUpper => Some(9999),
        NumberFormat::RomanUpper | NumberFormat::RomanLower => Some(3999),
        NumberFormat::Circled | NumberFormat::CircledFilled => Some(20),
        _ => None,
    };
    let Some(cap) = cap else { return Ok(()) };
    if count == 0 {
        return Ok(());
    }
    let last = config.start + (count - 1) * config.step;
    if last <= cap {
        return Ok(());
    }
    let offset = if config.start > cap {
        0
    } else {
        (cap - config.start) / config.step + 1
    };
    let value = config.start + offset * config.step;
    Err(match config.format {
        NumberFormat::RomanUpper | NumberFormat::RomanLower => BatchError::RomanOverflow { value },
        NumberFormat::ChineseLower | NumberFormat::ChineseUpper => {
            BatchError::ChineseTooLarge { value }
        }
        _ => BatchError::CircledOverflow { value },
    })
}

/// 生成单行插入文本（结构化字段或模板）。
fn render_insert(
    config: &BatchNumberingConfig,
    number: u64,
    total: u64,
    line: u64,
    filename: &str,
    date: &str,
    time: &str,
) -> Result<String, BatchError> {
    let number_text = format_number(config.format, number, config.zero_pad_width)?;
    if let Some(template) = config.template.as_deref() {
        return render_template(
            template,
            &RenderContext {
                number,
                total,
                line: line + 1,
                date,
                time,
                filename,
            },
        );
    }
    Ok(format!(
        "{number_text}{}{}",
        config.separator, config.suffix
    ))
}

/// 模板渲染：`{var}` 替换；未知变量与不配对花括号一律报错。
fn render_template(template: &str, context: &RenderContext<'_>) -> Result<String, BatchError> {
    let mut out = String::with_capacity(template.len() + 8);
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '{' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(c) => name.push(c),
                        None => {
                            return Err(BatchError::InvalidTemplate {
                                message: "花括号不配对（缺少 '}'）".to_string(),
                            })
                        }
                    }
                }
                let value = match name.as_str() {
                    "n" => context.number.to_string(),
                    "total" => context.total.to_string(),
                    "line" => context.line.to_string(),
                    "date" => context.date.to_string(),
                    "time" => context.time.to_string(),
                    "filename" => context.filename.to_string(),
                    other => {
                        return Err(BatchError::UnknownTemplateVar {
                            name: other.to_string(),
                        })
                    }
                };
                out.push_str(&value);
            }
            '}' => {
                return Err(BatchError::InvalidTemplate {
                    message: "花括号不配对（多余的 '}'）".to_string(),
                })
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

/// 数字格式化（10 种内置；超范围保护见 [`BatchError`]）。
fn format_number(format: NumberFormat, n: u64, zero_pad_width: u32) -> Result<String, BatchError> {
    match format {
        NumberFormat::Arabic => Ok(n.to_string()),
        NumberFormat::ZeroPad => {
            if !(2..=10).contains(&zero_pad_width) {
                return Err(BatchError::ZeroPadWidthInvalid {
                    width: zero_pad_width,
                });
            }
            Ok(format!("{n:0width$}", width = zero_pad_width as usize))
        }
        NumberFormat::ChineseLower => chinese_number(n, false),
        NumberFormat::ChineseUpper => chinese_number(n, true),
        NumberFormat::Parenthesized => Ok(format!("({n})")),
        NumberFormat::Bracketed => Ok(format!("[{n}]")),
        NumberFormat::Circled => {
            if !(1..=20).contains(&n) {
                return Err(BatchError::CircledOverflow { value: n });
            }
            Ok(char::from_u32(0x2460 + (n - 1) as u32)
                .expect("带圈字符范围合法")
                .to_string())
        }
        NumberFormat::CircledFilled => {
            if !(1..=20).contains(&n) {
                return Err(BatchError::CircledOverflow { value: n });
            }
            let code = if n <= 10 {
                0x2776 + (n - 1) as u32
            } else {
                0x24EB + (n - 11) as u32
            };
            Ok(char::from_u32(code)
                .expect("实心带圈字符范围合法")
                .to_string())
        }
        NumberFormat::RomanUpper => roman_number(n, true),
        NumberFormat::RomanLower => roman_number(n, false),
    }
}

/// 中文数字（1–9999；小写/大写）。
fn chinese_number(n: u64, upper: bool) -> Result<String, BatchError> {
    if n == 0 || n > 9999 {
        return Err(BatchError::ChineseTooLarge { value: n });
    }
    let digits: [&str; 10] = if upper {
        ["零", "壹", "贰", "叁", "肆", "伍", "陆", "柒", "捌", "玖"]
    } else {
        ["零", "一", "二", "三", "四", "五", "六", "七", "八", "九"]
    };
    let units: [&str; 4] = if upper {
        ["", "拾", "佰", "仟"]
    } else {
        ["", "十", "百", "千"]
    };
    let text = n.to_string();
    let chars: Vec<u64> = text
        .chars()
        .map(|c| c.to_digit(10).expect("数字串") as u64)
        .collect();
    let len = chars.len();
    let mut out = String::new();
    for (index, &digit) in chars.iter().enumerate() {
        let unit = len - 1 - index;
        if digit == 0 {
            let has_later_nonzero = chars[index + 1..].iter().any(|&value| value != 0);
            if has_later_nonzero && !out.ends_with(digits[0]) {
                out.push_str(digits[0]);
            }
            continue;
        }
        let skip_leading_one = !upper && digit == 1 && unit == 1 && index == 0;
        if !skip_leading_one {
            out.push_str(digits[digit as usize]);
        }
        out.push_str(units[unit]);
    }
    Ok(out)
}

/// 罗马数字（1–3999；大写/小写）。
fn roman_number(mut n: u64, upper: bool) -> Result<String, BatchError> {
    if n == 0 || n > 3999 {
        return Err(BatchError::RomanOverflow { value: n });
    }
    const PAIRS: &[(u64, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for &(value, symbol) in PAIRS {
        while n >= value {
            out.push_str(symbol);
            n -= value;
        }
    }
    Ok(if upper { out } else { out.to_lowercase() })
}

/// 当前 UTC 日期（YYYY-MM-DD）与时间（HH:MM:SS），取自 RFC 3339 时间戳。
fn now_date_time() -> (String, String) {
    let stamp = crate::time_util::now_rfc3339();
    let date = stamp.get(0..10).unwrap_or_default().to_string();
    let time = stamp.get(11..19).unwrap_or_default().to_string();
    (date, time)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// 构造文档（各元素一行，以 `\n` 连接；不追加末尾换行）。
    fn doc_with(lines: &[&str]) -> (tempfile::TempDir, EditDoc) {
        let tmp = tempfile::tempdir().expect("临时目录");
        let path = tmp.path().join("sample.txt");
        fs::write(&path, lines.join("\n")).expect("写样本失败");
        let doc = EditDoc::open(&path, None, 64).expect("打开失败");
        (tmp, doc)
    }

    /// 基础配置构造器（阿拉伯数字 / 全文 / 行首）。
    fn config(format: NumberFormat, scope: BatchScope) -> BatchNumberingConfig {
        BatchNumberingConfig {
            format,
            start: 1,
            step: 1,
            zero_pad_width: 2,
            separator: ".".to_string(),
            suffix: String::new(),
            position: InsertPosition::LineStart,
            scope,
            skip_empty: true,
            template: None,
            preview_lines: 10,
        }
    }

    #[test]
    fn arabic_all_lines_and_single_undo() {
        let (_tmp, mut doc) = doc_with(&["aa", "bb", "cc"]);
        let outcome = doc
            .apply_batch_numbering(&config(NumberFormat::Arabic, BatchScope::All))
            .expect("执行失败");
        assert_eq!(outcome.affected, 3);
        assert_eq!(doc.row_text(0).as_deref(), Some("1.aa"));
        assert_eq!(doc.row_text(2).as_deref(), Some("3.cc"));
        assert!(doc.is_dirty());
        doc.undo();
        assert_eq!(doc.row_text(0).as_deref(), Some("aa"));
        assert_eq!(doc.row_text(2).as_deref(), Some("cc"));
        assert!(!doc.is_dirty(), "单撤销应完全还原");
    }

    #[test]
    fn zero_pad_formats_and_validates_width() {
        let (_tmp, mut doc) = doc_with(&["x", "y"]);
        let mut cfg = config(NumberFormat::ZeroPad, BatchScope::All);
        cfg.zero_pad_width = 3;
        doc.apply_batch_numbering(&cfg).expect("执行失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("001.x"));

        let (_tmp2, mut doc2) = doc_with(&["x"]);
        let mut bad = config(NumberFormat::ZeroPad, BatchScope::All);
        bad.zero_pad_width = 1;
        assert!(matches!(
            doc2.apply_batch_numbering(&bad),
            Err(BatchError::ZeroPadWidthInvalid { .. })
        ));
    }

    #[test]
    fn chinese_numbers_lower_and_upper() {
        let (_tmp, mut lower) = doc_with(&["a", "b", "c"]);
        let mut cfg = config(NumberFormat::ChineseLower, BatchScope::All);
        cfg.start = 9;
        lower.apply_batch_numbering(&cfg).expect("执行失败");
        assert_eq!(lower.row_text(0).as_deref(), Some("九.a"));
        assert_eq!(lower.row_text(1).as_deref(), Some("十.b"));
        assert_eq!(lower.row_text(2).as_deref(), Some("十一.c"));

        let (_tmp2, mut upper) = doc_with(&["a"]);
        upper
            .apply_batch_numbering(&config(NumberFormat::ChineseUpper, BatchScope::All))
            .expect("执行失败");
        assert_eq!(upper.row_text(0).as_deref(), Some("壹.a"));
    }

    #[test]
    fn chinese_overflow_is_rejected() {
        let (_tmp, mut doc) = doc_with(&["a", "b"]);
        let mut cfg = config(NumberFormat::ChineseLower, BatchScope::All);
        cfg.start = 9999;
        assert!(matches!(
            doc.apply_batch_numbering(&cfg),
            Err(BatchError::ChineseTooLarge { value: 10000 })
        ));
        assert_eq!(doc.row_text(0).as_deref(), Some("a"), "失败不应改动文档");
    }

    #[test]
    fn roman_upper_lower_and_overflow() {
        let (_tmp, mut doc) = doc_with(&["a", "b", "c"]);
        let mut cfg = config(NumberFormat::RomanUpper, BatchScope::All);
        cfg.start = 4;
        doc.apply_batch_numbering(&cfg).expect("执行失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("IV.a"));
        assert_eq!(doc.row_text(1).as_deref(), Some("V.b"));
        assert_eq!(doc.row_text(2).as_deref(), Some("VI.c"));

        let (_tmp2, mut doc2) = doc_with(&["a"]);
        let mut lower = config(NumberFormat::RomanLower, BatchScope::All);
        lower.start = 9;
        doc2.apply_batch_numbering(&lower).expect("执行失败");
        assert_eq!(doc2.row_text(0).as_deref(), Some("ix.a"));

        let (_tmp3, mut doc3) = doc_with(&["a", "b"]);
        let mut overflow = config(NumberFormat::RomanUpper, BatchScope::All);
        overflow.start = 3999;
        assert!(matches!(
            doc3.apply_batch_numbering(&overflow),
            Err(BatchError::RomanOverflow { value: 4000 })
        ));
    }

    #[test]
    fn circled_formats_and_cap() {
        let (_tmp, mut doc) = doc_with(&["a", "b"]);
        let mut cfg = config(NumberFormat::Circled, BatchScope::All);
        cfg.start = 19;
        doc.apply_batch_numbering(&cfg).expect("执行失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("⑲.a"));
        assert_eq!(doc.row_text(1).as_deref(), Some("⑳.b"));

        let (_tmp2, mut doc2) = doc_with(&["a", "b", "c"]);
        let mut over = config(NumberFormat::CircledFilled, BatchScope::All);
        over.start = 19;
        assert!(matches!(
            doc2.apply_batch_numbering(&over),
            Err(BatchError::CircledOverflow { value: 21 })
        ));

        let (_tmp3, mut doc3) = doc_with(&["a", "b"]);
        let mut filled = config(NumberFormat::CircledFilled, BatchScope::All);
        filled.start = 10;
        doc3.apply_batch_numbering(&filled).expect("执行失败");
        assert_eq!(doc3.row_text(0).as_deref(), Some("❿.a"));
        assert_eq!(doc3.row_text(1).as_deref(), Some("⓫.b"));
    }

    #[test]
    fn parenthesized_and_bracketed() {
        let (_tmp, mut doc) = doc_with(&["a"]);
        doc.apply_batch_numbering(&config(NumberFormat::Parenthesized, BatchScope::All))
            .expect("执行失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("(1).a"));

        let (_tmp2, mut doc2) = doc_with(&["a"]);
        doc2.apply_batch_numbering(&config(NumberFormat::Bracketed, BatchScope::All))
            .expect("执行失败");
        assert_eq!(doc2.row_text(0).as_deref(), Some("[1].a"));
    }

    #[test]
    fn row_range_only_touches_target_rows() {
        let (_tmp, mut doc) = doc_with(&["aa", "bb", "cc", "dd", "ee"]);
        let cfg = config(
            NumberFormat::Arabic,
            BatchScope::RowRange { from: 1, to: 2 },
        );
        let outcome = doc.apply_batch_numbering(&cfg).expect("执行失败");
        assert_eq!(outcome.affected, 2);
        assert_eq!(doc.row_text(0).as_deref(), Some("aa"));
        assert_eq!(doc.row_text(1).as_deref(), Some("1.bb"));
        assert_eq!(doc.row_text(2).as_deref(), Some("2.cc"));
        assert_eq!(doc.row_text(3).as_deref(), Some("dd"));
    }

    #[test]
    fn selection_scope_and_invalid_range() {
        let (_tmp, mut doc) = doc_with(&["aa", "bb", "cc"]);
        let cfg = config(
            NumberFormat::Arabic,
            BatchScope::Selection { from: 2, to: 2 },
        );
        doc.apply_batch_numbering(&cfg).expect("执行失败");
        assert_eq!(doc.row_text(2).as_deref(), Some("1.cc"));

        let (_tmp2, mut doc2) = doc_with(&["aa"]);
        assert!(matches!(
            doc2.apply_batch_numbering(&config(
                NumberFormat::Arabic,
                BatchScope::RowRange { from: 5, to: 9 }
            )),
            Err(BatchError::RowRangeInvalid { .. })
        ));
        assert!(matches!(
            doc2.apply_batch_numbering(&config(
                NumberFormat::Arabic,
                BatchScope::RowRange { from: 2, to: 1 }
            )),
            Err(BatchError::RowRangeInvalid { .. })
        ));
    }

    #[test]
    fn skip_empty_lines_by_default() {
        let (_tmp, mut doc) = doc_with(&["a", "", "b"]);
        let outcome = doc
            .apply_batch_numbering(&config(NumberFormat::Arabic, BatchScope::All))
            .expect("执行失败");
        assert_eq!(outcome.affected, 2);
        assert_eq!(doc.row_text(0).as_deref(), Some("1.a"));
        assert_eq!(doc.row_text(1).as_deref(), Some(""));
        assert_eq!(doc.row_text(2).as_deref(), Some("2.b"));
    }

    #[test]
    fn line_end_position_with_unicode() {
        let (_tmp, mut doc) = doc_with(&["中文", "abc"]);
        let mut cfg = config(NumberFormat::Arabic, BatchScope::All);
        cfg.position = InsertPosition::LineEnd;
        doc.apply_batch_numbering(&cfg).expect("执行失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("中文 1."));
        assert_eq!(doc.row_text(1).as_deref(), Some("abc 2."));
    }

    #[test]
    fn line_end_preview_matches_apply() {
        let (_tmp, doc) = doc_with(&["中文"]);
        let mut cfg = config(NumberFormat::Arabic, BatchScope::All);
        cfg.position = InsertPosition::LineEnd;
        let preview = doc.preview_batch_numbering(&cfg).expect("预览失败");
        assert_eq!(preview.items[0].after, "中文 1.");
    }

    #[test]
    fn template_variables_and_errors() {
        let (_tmp, mut doc) = doc_with(&["a", "b"]);
        let mut cfg = config(NumberFormat::Arabic, BatchScope::All);
        cfg.template = Some("L{line}/{total}:".to_string());
        doc.apply_batch_numbering(&cfg).expect("执行失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("L1/2:a"));
        assert_eq!(doc.row_text(1).as_deref(), Some("L2/2:b"));

        let (_tmp2, mut doc2) = doc_with(&["a"]);
        let mut unknown = config(NumberFormat::Arabic, BatchScope::All);
        unknown.template = Some("{bogus}".to_string());
        assert!(matches!(
            doc2.apply_batch_numbering(&unknown),
            Err(BatchError::UnknownTemplateVar { .. })
        ));

        let mut broken = config(NumberFormat::Arabic, BatchScope::All);
        broken.template = Some("{n".to_string());
        assert!(matches!(
            doc2.apply_batch_numbering(&broken),
            Err(BatchError::InvalidTemplate { .. })
        ));
    }

    #[test]
    fn preview_truncates_and_reports_total() {
        let (_tmp, doc) = doc_with(&["a", "b", "c", "d", "e"]);
        let mut cfg = config(NumberFormat::Arabic, BatchScope::All);
        cfg.preview_lines = 2;
        let preview = doc.preview_batch_numbering(&cfg).expect("预览失败");
        assert_eq!(preview.items.len(), 2);
        assert!(preview.truncated);
        assert_eq!(preview.total_rows, 5);
        assert_eq!(preview.items[1].insert_text, "2.");
    }

    #[test]
    fn empty_scope_is_rejected() {
        let (_tmp, doc) = doc_with(&["", "", ""]);
        assert!(matches!(
            doc.preview_batch_numbering(&config(NumberFormat::Arabic, BatchScope::All)),
            Err(BatchError::EmptyScope)
        ));
    }

    #[test]
    fn too_many_rows_is_rejected() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let path = tmp.path().join("huge.txt");
        // 200_001 行（空行；skip_empty=false 以防被跳过；结尾换行不产生额外空行）
        fs::write(&path, "\n".repeat(BATCH_MAX_ROWS as usize + 1)).expect("写大文件失败");
        let doc = EditDoc::open(&path, None, 64).expect("打开失败");
        let mut cfg = config(NumberFormat::Arabic, BatchScope::All);
        cfg.skip_empty = false;
        assert!(matches!(
            doc.preview_batch_numbering(&cfg),
            Err(BatchError::TooManyRows { .. })
        ));
    }

    #[test]
    fn separator_and_suffix_compose() {
        let (_tmp, mut doc) = doc_with(&["a"]);
        let mut cfg = config(NumberFormat::Arabic, BatchScope::All);
        cfg.separator = "、".to_string();
        cfg.suffix = " ".to_string();
        doc.apply_batch_numbering(&cfg).expect("执行失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("1、 a"));
    }
}
