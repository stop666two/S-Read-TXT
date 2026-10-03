//! 查找与替换（双模式：标准字面 / 用户正则；大小写敏感开关）——设计文档 §5.3。
//!
//! 实现要点：
//! - **流式扫描**：按 1MB 文件字节分块解码，块间保留接续区（carry）——
//!   字面量按 `查询长度×4 + 8`、正则按固定 64KB；可命中跨块与跨片段的匹配，
//!   内存与文件大小无关；
//! - **坐标**：扫描同时维护「逻辑行 + 行内 UTF-16」游标（CRLF 视为单一换行
//!   单元），命中直接产出与前端编辑坐标一致的显示坐标；
//! - **大小写不敏感**（字面模式）= `char::to_lowercase` 简单折叠逐字符比对，
//!   另补 `ς→σ`、`İ→i` 两个特例（见 `fold_chars`）；正则模式映射为
//!   `RegexBuilder::case_insensitive`；
//! - **正则模式**：Rust `regex` 语法（线性时间、无回溯引用/环视）；`^`/`$`
//!   按行锚定（multi-line）；零宽匹配跳过；替换支持 `$1`/`${name}` 捕获展开；
//! - **替换流程**：全部替换 = 预览（[`EditDoc::preview_replace_all`]）→
//!   二次确认（可逐条剔除）→ 执行（[`EditDoc::replace_matches`]，单撤销步）；
//!   命中数超过 [`REPLACE_ALL_LIMIT`] 一律拒绝（防止构造超大操作批次）；
//! - 以 CRLF 的 `\n` 起始的命中会向前扩展包含 `\r`，保证替换坐标可精确解析。

use regex::Regex;
use serde::Serialize;

use std::time::{Duration, Instant};

use super::edit_doc::{EditApplied, EditDoc, EditError, EditOp};
use super::piece::PieceSource;

/// 扫描块大小（文件字节）。
const SEARCH_CHUNK_BYTES: usize = 1 << 20;

/// 「全部替换」命中数上限（超出报 [`EditError::TooManyMatches`]）。
pub const REPLACE_ALL_LIMIT: usize = 200_000;

/// 正则模式的固定接续区大小：两解码块之间保留的尾部字节数
/// （保证跨块匹配可被检出；匹配长度超过该窗口属已知边界）。
const REGEX_CARRY_BYTES: usize = 64 * 1024;

/// 高亮窗口单次返回的命中上限（可见行窗口内足够用）。
const MATCH_WINDOW_LIMIT: usize = 2_000;

/// 预览条目中文本截断长度（字符数；防止超长行塞满 IPC 载荷）。
const PREVIEW_TEXT_CHARS: usize = 200;

/// 预览弹窗列举的命中上限（超出时仅支持整体替换，不支持逐条剔除）。
pub const PREVIEW_LIST_CAP: usize = 500;

/// 匹配计数上限（超出仅报“截断”，用于计数显示；不影响替换路径自己的上限）。
pub const MATCH_COUNT_LIMIT: usize = 200_000;

/// 一次查找命中（**显示行坐标**：段号 + 段内 UTF-16 偏移；超长行分段对前端透明）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindHit {
    /// 起始显示行号
    pub start_row: u64,
    /// 起始段内 UTF-16 偏移
    pub start_utf16: u64,
    /// 结束显示行号
    pub end_row: u64,
    /// 结束段内 UTF-16 偏移
    pub end_utf16: u64,
}

/// 「替换下一次」的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceNextOutcome {
    /// 本次替换的编辑结果
    pub applied: EditApplied,
    /// 替换后从光标起继续查找的下一次命中
    pub next: Option<FindHit>,
}

/// 「全部替换」的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceAllOutcome {
    /// 实际替换次数
    pub replaced: usize,
    /// 编辑结果（无命中时为 `None`，文档保持不变）
    pub applied: Option<EditApplied>,
}

/// 匹配计数结果（`truncated` 为真表示达到上限后提前停止）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCount {
    /// 统计到的命中数（截断时为上限值）
    pub total: usize,
    /// 是否因超过上限而截断
    pub truncated: bool,
}

/// 查找请求（全词/大小写/模式/超时的统一参数包）。
pub struct SearchRequest<'a> {
    /// 查询文本（空查询不产生命中）
    pub query: &'a str,
    /// 大小写敏感
    pub case_sensitive: bool,
    /// 字面 / 正则
    pub mode: SearchMode,
    /// 全词匹配（以 `\b` 边界实现；字面模式对查询整体包裹）
    pub whole_word: bool,
    /// 正则扫描超时（毫秒；`None`/0 不限时）
    pub timeout_ms: Option<u32>,
}

impl SearchRequest<'_> {
    /// 计算扫描截止时刻（`timeout_ms` 为空或 0 时不限时）。
    fn deadline(&self) -> Option<Instant> {
        self.timeout_ms
            .filter(|ms| *ms > 0)
            .map(|ms| Instant::now() + Duration::from_millis(u64::from(ms)))
    }
}

/// 查找模式：标准（字面）或正则（用户自写）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    /// 字面匹配（特殊符号按原样，不做正则解释）
    Literal,
    /// 正则匹配（Rust `regex` 语法；替换支持 `$1`/`${name}` 捕获展开）
    Regex,
}

/// 「全部替换」预览中的单条命中（显示坐标 + 前后文本，供确认弹窗展示与逐条剔除）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacePreviewItem {
    /// 命中序号（0 起；与执行时 `selected` 下标一致）
    pub index: usize,
    /// 起始显示行号
    pub start_row: u64,
    /// 起始段内 UTF-16 偏移
    pub start_utf16: u64,
    /// 结束显示行号
    pub end_row: u64,
    /// 结束段内 UTF-16 偏移
    pub end_utf16: u64,
    /// 命中所在行文本（截断）
    pub line_text: String,
    /// 被替换的原文（截断）
    pub matched_text: String,
    /// 替换后的文本（正则已展开 `$n`；截断）
    pub replacement_text: String,
}

/// 「全部替换」预览结果（执行前二次确认用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplacePreview {
    /// 预览时的文档状态号（执行时回传校验，防止预览后文档变化）
    pub state_id: u64,
    /// 命中总数
    pub total: usize,
    /// 是否因数量超过列举上限而截断（截断时仅支持整体替换，不支持逐条剔除）
    pub truncated: bool,
    /// 列举的命中（最多 `max_items` 条）
    pub items: Vec<ReplacePreviewItem>,
}

impl EditDoc {
    /// 从文档位置 `from`（默认文档开头）起向后查找下一次出现。
    ///
    /// - `case_sensitive`：大小写敏感开关；
    /// - `mode`：标准（字面）或正则；空查询不视为命中；
    /// - 不环绕（前端负责到文件末尾后从头重试）；
    /// - `from` 与返回值均为**显示行坐标**（超长行分段对前端透明）。
    pub fn find(
        &self,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        from: Option<(u64, u64)>,
    ) -> Result<Option<FindHit>, EditError> {
        let request = SearchRequest {
            query,
            case_sensitive,
            mode,
            whole_word: false,
            timeout_ms: None,
        };
        self.find_request(&request, from, None)
    }

    /// 请求式查找：支持全词匹配、（正则）超时与行区间上界（`until_row`，显示行，含该行）。
    pub fn find_request(
        &self,
        request: &SearchRequest<'_>,
        from: Option<(u64, u64)>,
        until_row: Option<u64>,
    ) -> Result<Option<FindHit>, EditError> {
        let matcher = Matcher::build_opts(
            request.query,
            request.case_sensitive,
            request.mode,
            request.whole_word,
        )?;
        let logical_from = from.map(|(seg, local)| self.to_logical_pos(seg, local));
        let until_logical = until_row.map(|row| self.to_logical_pos(row, 0).0);
        let max_row = until_logical.map(|row| row.saturating_add(1));
        let found = self.find_logical(&matcher, logical_from, max_row, request.deadline())?;
        // 同一解码块内可能越过 max_row：回调后再做严格上界检查
        // （`max_row` 仅保证块间提前退出，块内命中仍会先上报）。
        let bounded = match (found, until_logical) {
            (Some(hit), Some(limit)) if hit.start_row > limit => None,
            (found, _) => found,
        };
        Ok(bounded.map(|hit| self.to_display_hit(hit)))
    }

    /// 统计命中总数（达到 [`MATCH_COUNT_LIMIT`] 后停止并标记截断）。
    pub fn count_request(&self, request: &SearchRequest<'_>) -> Result<MatchCount, EditError> {
        self.count_request_with(request, MATCH_COUNT_LIMIT)
    }

    /// 带显式上限的计数（上限可注入：单测用小值验证截断）。
    pub fn count_request_with(
        &self,
        request: &SearchRequest<'_>,
        limit: usize,
    ) -> Result<MatchCount, EditError> {
        if request.query.is_empty() {
            return Ok(MatchCount {
                total: 0,
                truncated: false,
            });
        }
        let matcher = Matcher::build_opts(
            request.query,
            request.case_sensitive,
            request.mode,
            request.whole_word,
        )?;
        let mut total = 0usize;
        let mut truncated = false;
        self.scan(
            &matcher,
            None,
            None,
            request.deadline(),
            |_hit, _matched| {
                total += 1;
                if total > limit {
                    truncated = true;
                    return false;
                }
                true
            },
        )?;
        if truncated {
            total = limit;
        }
        Ok(MatchCount { total, truncated })
    }

    /// 内部：查找逻辑坐标命中（编辑操作用）。
    fn find_logical(
        &self,
        matcher: &Matcher,
        from: Option<(u64, u64)>,
        max_row: Option<u64>,
        deadline: Option<Instant>,
    ) -> Result<Option<FindHit>, EditError> {
        let mut hit = None;
        self.scan(matcher, from, max_row, deadline, |found, _matched| {
            hit = Some(found);
            false
        })?;
        Ok(hit)
    }

    /// 内部：查找逻辑坐标命中与匹配原文（替换展开用）。
    fn find_logical_match(
        &self,
        matcher: &Matcher,
        from: Option<(u64, u64)>,
        deadline: Option<Instant>,
    ) -> Result<Option<(FindHit, String)>, EditError> {
        let mut found = None;
        self.scan(matcher, from, None, deadline, |hit, matched| {
            found = Some((hit, matched.to_string()));
            false
        })?;
        Ok(found)
    }

    /// 逻辑坐标命中 → 显示坐标命中（普通行恒等；超长行按段映射）。
    fn to_display_hit(&self, hit: FindHit) -> FindHit {
        let (start_row, start_utf16) = self.seg_of_row_utf16(hit.start_row, hit.start_utf16);
        let (end_row, end_utf16) = self.seg_of_row_utf16(hit.end_row, hit.end_utf16);
        FindHit {
            start_row,
            start_utf16,
            end_row,
            end_utf16,
        }
    }

    /// 显示坐标（段号 + 段内 UTF-16 偏移）→ 逻辑坐标（行号 + 行内 UTF-16 偏移）。
    fn to_logical_pos(&self, seg: u64, seg_utf16: u64) -> (u64, u64) {
        match self.seg_to_row_utf16(seg) {
            Some((row, base, _index)) => (row, base + seg_utf16),
            None => (seg, seg_utf16),
        }
    }

    /// 查找并从 `from` 起替换下一次出现；返回替换结果与后续命中（便于连续替换）。
    ///
    /// `from` 为显示坐标；无命中时返回 `Ok(None)`（文档不变）。
    /// 正则模式的 `replacement` 支持 `$1`/`${name}` 捕获展开。
    pub fn replace_next(
        &mut self,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        from: Option<(u64, u64)>,
        replacement: &str,
    ) -> Result<Option<ReplaceNextOutcome>, EditError> {
        let request = SearchRequest {
            query,
            case_sensitive,
            mode,
            whole_word: false,
            timeout_ms: None,
        };
        self.replace_next_request(&request, from, replacement)
    }

    /// 请求式「替换下一次」（全词/超时感知；`from` 为显示坐标）。
    pub fn replace_next_request(
        &mut self,
        request: &SearchRequest<'_>,
        from: Option<(u64, u64)>,
        replacement: &str,
    ) -> Result<Option<ReplaceNextOutcome>, EditError> {
        let matcher = Matcher::build_opts(
            request.query,
            request.case_sensitive,
            request.mode,
            request.whole_word,
        )?;
        let deadline = request.deadline();
        let logical_from = from.map(|(seg, local)| self.to_logical_pos(seg, local));
        let Some((hit, matched)) = self.find_logical_match(&matcher, logical_from, deadline)?
        else {
            return Ok(None);
        };
        let op = EditOp::Replace {
            start_row: hit.start_row,
            start_utf16: hit.start_utf16,
            end_row: hit.end_row,
            end_utf16: hit.end_utf16,
            text: matcher.expand(&matched, replacement),
        };
        let applied = self.apply_edits(&[op])?;
        let (next_row, next_utf16) = self.to_logical_pos(applied.caret_row, applied.caret_utf16);
        let next = self
            .find_logical(&matcher, Some((next_row, next_utf16)), None, deadline)?
            .map(|found| self.to_display_hit(found));
        Ok(Some(ReplaceNextOutcome { applied, next }))
    }

    /// 全部替换为 `replacement`（单次编辑 = 单个撤销步）。
    ///
    /// 命中数上限 [`REPLACE_ALL_LIMIT`]；超出时报 `TooManyMatches` 且文档不变。
    /// 说明：IPC 层的「全部替换」走「预览 → 二次确认（可逐条剔除）→ 执行」流程
    /// （见 [`EditDoc::preview_replace_all`] 与 [`EditDoc::replace_matches`]）；
    /// 本方法是引擎级便捷入口（单测使用）。
    pub fn replace_all(
        &mut self,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        replacement: &str,
    ) -> Result<ReplaceAllOutcome, EditError> {
        let request = SearchRequest {
            query,
            case_sensitive,
            mode,
            whole_word: false,
            timeout_ms: None,
        };
        self.replace_all_request(&request, replacement)
    }

    /// 请求式全部替换（全词/超时感知；单次编辑 = 单个撤销步）。
    pub fn replace_all_request(
        &mut self,
        request: &SearchRequest<'_>,
        replacement: &str,
    ) -> Result<ReplaceAllOutcome, EditError> {
        let matcher = Matcher::build_opts(
            request.query,
            request.case_sensitive,
            request.mode,
            request.whole_word,
        )?;
        let mut hits: Vec<(FindHit, String)> = Vec::new();
        let mut overflow = false;
        self.scan(
            &matcher,
            None,
            None,
            request.deadline(),
            |found, matched| {
                hits.push((found, matcher.expand(matched, replacement)));
                if hits.len() > REPLACE_ALL_LIMIT {
                    overflow = true;
                    false
                } else {
                    true
                }
            },
        )?;
        if overflow {
            return Err(EditError::TooManyMatches {
                limit: REPLACE_ALL_LIMIT,
            });
        }
        self.apply_hits(hits)
    }

    /// 生成「全部替换」预览：命中总数 + 前 `max_items` 条前后文本（含展开后的替换文本）。
    ///
    /// - 命中数超过 [`REPLACE_ALL_LIMIT`] 时返回 `TooManyMatches`（与执行口径一致）；
    /// - `truncated` 为真表示仅列举了部分命中（前端此时仅支持整体替换）。
    pub fn preview_replace_all(
        &self,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        replacement: &str,
        max_items: usize,
    ) -> Result<ReplacePreview, EditError> {
        let request = SearchRequest {
            query,
            case_sensitive,
            mode,
            whole_word: false,
            timeout_ms: None,
        };
        self.preview_replace_all_request(&request, replacement, max_items)
    }

    /// 请求式「全部替换预览」（全词/超时感知）。
    pub fn preview_replace_all_request(
        &self,
        request: &SearchRequest<'_>,
        replacement: &str,
        max_items: usize,
    ) -> Result<ReplacePreview, EditError> {
        if request.query.is_empty() {
            return Ok(ReplacePreview {
                state_id: self.state_id(),
                total: 0,
                truncated: false,
                items: Vec::new(),
            });
        }
        let matcher = Matcher::build_opts(
            request.query,
            request.case_sensitive,
            request.mode,
            request.whole_word,
        )?;
        let state_id = self.state_id();
        let mut total = 0usize;
        let mut items: Vec<ReplacePreviewItem> = Vec::new();
        let mut overflow = false;
        self.scan(&matcher, None, None, request.deadline(), |hit, matched| {
            let index = total;
            total += 1;
            if total > REPLACE_ALL_LIMIT {
                overflow = true;
                return false;
            }
            if items.len() < max_items {
                let (start_row, start_utf16) =
                    self.seg_of_row_utf16(hit.start_row, hit.start_utf16);
                let (end_row, end_utf16) = self.seg_of_row_utf16(hit.end_row, hit.end_utf16);
                items.push(ReplacePreviewItem {
                    index,
                    start_row,
                    start_utf16,
                    end_row,
                    end_utf16,
                    line_text: truncate_chars(
                        &self.row_text(hit.start_row).unwrap_or_default(),
                        PREVIEW_TEXT_CHARS,
                    ),
                    matched_text: truncate_chars(matched, PREVIEW_TEXT_CHARS),
                    replacement_text: truncate_chars(
                        &matcher.expand(matched, replacement),
                        PREVIEW_TEXT_CHARS,
                    ),
                });
            }
            true
        })?;
        if overflow {
            return Err(EditError::TooManyMatches {
                limit: REPLACE_ALL_LIMIT,
            });
        }
        Ok(ReplacePreview {
            state_id,
            total,
            truncated: total > items.len(),
            items,
        })
    }

    /// 执行「全部替换」：`indices = None` 替换全部命中；`Some` 仅替换列出的命中
    /// （升序下标，来自预览条目；用于用户在确认弹窗中剔除个别项）。
    ///
    /// - `expect_state_id` 必须等于当前文档状态号（预览后文档被修改时报 `StaleSearch`）；
    /// - 单次编辑 = 单个撤销步；命中数上限 [`REPLACE_ALL_LIMIT`]。
    pub fn replace_matches(
        &mut self,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        replacement: &str,
        indices: Option<&[usize]>,
        expect_state_id: u64,
    ) -> Result<ReplaceAllOutcome, EditError> {
        let request = SearchRequest {
            query,
            case_sensitive,
            mode,
            whole_word: false,
            timeout_ms: None,
        };
        self.replace_matches_request(&request, replacement, indices, expect_state_id)
    }

    /// 请求式「按命中执行替换」（全词/超时感知；`indices = None` 全部，`Some` 仅列出的升序下标）。
    pub fn replace_matches_request(
        &mut self,
        request: &SearchRequest<'_>,
        replacement: &str,
        indices: Option<&[usize]>,
        expect_state_id: u64,
    ) -> Result<ReplaceAllOutcome, EditError> {
        if self.state_id() != expect_state_id {
            return Err(EditError::StaleSearch);
        }
        let matcher = Matcher::build_opts(
            request.query,
            request.case_sensitive,
            request.mode,
            request.whole_word,
        )?;
        let mut hits: Vec<(FindHit, String)> = Vec::new();
        let mut index = 0usize;
        let mut overflow = false;
        self.scan(&matcher, None, None, request.deadline(), |hit, matched| {
            let include = match indices {
                None => true,
                Some(list) => list.binary_search(&index).is_ok(),
            };
            if include {
                hits.push((hit, matcher.expand(matched, replacement)));
            }
            index += 1;
            if index > REPLACE_ALL_LIMIT {
                overflow = true;
                false
            } else {
                true
            }
        })?;
        if overflow {
            return Err(EditError::TooManyMatches {
                limit: REPLACE_ALL_LIMIT,
            });
        }
        self.apply_hits(hits)
    }

    /// 可见行窗口内的全部命中（显示坐标；文档高亮用）。
    ///
    /// `start_row`/`count` 为显示行窗口；单次上限 [`MATCH_WINDOW_LIMIT`] 条；
    /// 扫描在越过窗口行后提前结束（不做全文件扫描）。
    pub fn match_window(
        &self,
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        start_row: u64,
        count: u64,
    ) -> Result<Vec<FindHit>, EditError> {
        let request = SearchRequest {
            query,
            case_sensitive,
            mode,
            whole_word: false,
            timeout_ms: None,
        };
        self.match_window_request(&request, start_row, count)
    }

    /// 请求式窗口高亮（全词/超时感知）。
    pub fn match_window_request(
        &self,
        request: &SearchRequest<'_>,
        start_row: u64,
        count: u64,
    ) -> Result<Vec<FindHit>, EditError> {
        if request.query.is_empty() || count == 0 {
            return Ok(Vec::new());
        }
        let matcher = Matcher::build_opts(
            request.query,
            request.case_sensitive,
            request.mode,
            request.whole_word,
        )?;
        let window_end = start_row.saturating_add(count);
        let (logical_row, _) = self.to_logical_pos(start_row, 0);
        let (last_logical, _) = self.to_logical_pos(window_end.saturating_sub(1), 0);
        let mut hits: Vec<FindHit> = Vec::new();
        self.scan(
            &matcher,
            Some((logical_row, 0)),
            Some(last_logical.saturating_add(1)),
            request.deadline(),
            |hit, _matched| {
                let display = self.to_display_hit(hit);
                if display.start_row >= window_end {
                    return false;
                }
                if display.start_row >= start_row {
                    hits.push(display);
                }
                hits.len() < MATCH_WINDOW_LIMIT
            },
        )?;
        Ok(hits)
    }

    /// 命中列表 → 单次编辑（内部共用；空列表返回无变更）。
    fn apply_hits(&mut self, hits: Vec<(FindHit, String)>) -> Result<ReplaceAllOutcome, EditError> {
        if hits.is_empty() {
            return Ok(ReplaceAllOutcome {
                replaced: 0,
                applied: None,
            });
        }
        let ops: Vec<EditOp> = hits
            .into_iter()
            .map(|(hit, text)| EditOp::Replace {
                start_row: hit.start_row,
                start_utf16: hit.start_utf16,
                end_row: hit.end_row,
                end_utf16: hit.end_utf16,
                text,
            })
            .collect();
        let replaced = ops.len();
        let applied = self.apply_edits(&ops)?;
        Ok(ReplaceAllOutcome {
            replaced,
            applied: Some(applied),
        })
    }

    /// 核心扫描：流式解码 + 游标推进，逐次回调命中（含匹配原文）；回调返回 `false` 时停止。
    ///
    /// - `matcher`：标准/正则匹配器（空查询直接返回）；
    /// - `max_row`（逻辑行，可选）：游标越过该行后提前结束（窗口高亮/行区间上界用）；
    /// - `deadline`（可选）：超过该时刻立即中断并报 [`EditError::RegexTimeout`]；
    /// - 命中与回调均为逻辑行坐标。
    fn scan(
        &self,
        matcher: &Matcher,
        from: Option<(u64, u64)>,
        max_row: Option<u64>,
        deadline: Option<Instant>,
        mut on_hit: impl FnMut(FindHit, &str) -> bool,
    ) -> Result<(), EditError> {
        if matcher.query_is_empty() {
            return Ok(());
        }
        let timed_out =
            |deadline: Option<Instant>| deadline.is_some_and(|limit| Instant::now() >= limit);
        let (start_row, start_utf16) = from.unwrap_or((0, 0));
        let start_pos = self.resolve_pos(start_row, start_utf16)?;
        let global_start = self.global_offset(start_pos);
        let mut cursor = ScanCursor {
            row: start_row,
            utf16: start_utf16,
            prev_cr: self.char_before_is_cr(global_start),
        };
        let mut carry = String::new();
        let pieces = self.pieces();
        for piece_index in start_pos.piece..pieces.len() {
            let piece = pieces[piece_index];
            let mut off = if piece_index == start_pos.piece {
                start_pos.off
            } else {
                0
            };
            if off >= piece.len {
                continue;
            }
            match piece.source {
                PieceSource::Added => {
                    let bytes = self.piece_bytes(&piece);
                    while off < piece.len {
                        if timed_out(deadline) {
                            return Err(EditError::RegexTimeout);
                        }
                        let mut take = ((piece.len - off) as usize).min(SEARCH_CHUNK_BYTES);
                        let end = (off as usize) + take;
                        if end < piece.len as usize {
                            take = ceil_char_boundary_byte(bytes, end) - off as usize;
                        }
                        let chunk =
                            String::from_utf8_lossy(&bytes[off as usize..off as usize + take]);
                        let keep = self.consume_chunk(
                            chunk.as_ref(),
                            matcher,
                            &mut carry,
                            &mut cursor,
                            &mut on_hit,
                        );
                        off += take as u64;
                        if !keep {
                            return Ok(());
                        }
                        if cursor.row >= max_row.unwrap_or(u64::MAX) {
                            return Ok(());
                        }
                    }
                }
                PieceSource::Original => {
                    let bytes = self.piece_bytes(&piece);
                    let encoding = self.encoding_for(&piece);
                    let mut decoder = encoding.encoding().new_decoder_without_bom_handling();
                    while off < piece.len {
                        if timed_out(deadline) {
                            return Err(EditError::RegexTimeout);
                        }
                        let take = ((piece.len - off) as usize).min(SEARCH_CHUNK_BYTES);
                        let mut buffer = String::with_capacity(take + 16);
                        let (_result, read, _replaced) = decoder.decode_to_string(
                            &bytes[off as usize..off as usize + take],
                            &mut buffer,
                            false,
                        );
                        if read == 0 {
                            // 仅可能出现在剩余不足一个字符时：用 last 冲洗缓冲
                            let (_r, _rd, _rp) = decoder.decode_to_string(&[], &mut buffer, true);
                        }
                        if !buffer.is_empty() {
                            let keep = self.consume_chunk(
                                &buffer,
                                matcher,
                                &mut carry,
                                &mut cursor,
                                &mut on_hit,
                            );
                            if !keep {
                                return Ok(());
                            }
                        }
                        if cursor.row >= max_row.unwrap_or(u64::MAX) {
                            return Ok(());
                        }
                        if read == 0 {
                            break;
                        }
                        off += read as u64;
                    }
                }
            }
        }
        Ok(())
    }

    /// 处理一个已解码文本块：拼接接续区、搜索命中、推进游标、更新接续区。
    ///
    /// 返回 `false` 表示调用方应停止扫描（回调要求停止）。
    fn consume_chunk(
        &self,
        chunk: &str,
        matcher: &Matcher,
        carry: &mut String,
        cursor: &mut ScanCursor,
        on_hit: &mut impl FnMut(FindHit, &str) -> bool,
    ) -> bool {
        if chunk.is_empty() {
            return true;
        }
        let carry_len = carry.len();
        let mut text = String::with_capacity(carry_len + chunk.len());
        text.push_str(carry);
        text.push_str(chunk);
        let text_bytes = text.as_bytes();
        let mut work = *cursor;
        let mut text_pos = 0usize;
        let mut search_from = 0usize;
        let mut keep_going = true;
        while let Some((match_start, match_end)) = matcher.find_at(&text, search_from) {
            let mut start = match_start;
            search_from = match_end;
            // 完全落在接续区内的命中：上一块已处理过
            if match_end <= carry_len {
                continue;
            }
            // 以 CRLF 的 `\n` 起始：向前扩展包含其 `\r`，保证编辑坐标可精确解析
            if start > 0 && text_bytes[start] == b'\n' && text_bytes[start - 1] == b'\r' {
                if start - 1 < text_pos {
                    continue; // 无法扩展（会与上一命中重叠）：防御性跳过
                }
                start -= 1;
            } else if start == 0 && text_bytes[0] == b'\n' && work.prev_cr {
                // `\r` 落在上一解码块：起始坐标无法表达，防御性跳过
                continue;
            }
            work.advance(&text[text_pos..start]);
            let start_row = work.row;
            let start_utf16 = work.utf16;
            work.advance(&text[start..match_end]);
            text_pos = match_end;
            let hit = FindHit {
                start_row,
                start_utf16,
                end_row: work.row,
                end_utf16: work.utf16,
            };
            // 展开用「真实匹配区间」的原文（CRLF 扩展的 `\r` 不属于匹配）
            if !on_hit(hit, &text[match_start..match_end]) {
                keep_going = false;
                break;
            }
        }
        // 游标推进到新接续区起点（与命中处理相互独立，保证坐标一致性）
        let keep_start = matcher.carry_start(&text);
        cursor.advance(&text[..keep_start]);
        *carry = text[keep_start..].to_string();
        keep_going
    }
}

/// 扫描游标：把「已消费文本」累计为（逻辑行号, 行内 UTF-16 偏移）。
#[derive(Debug, Clone, Copy)]
struct ScanCursor {
    /// 当前行号（逻辑行）
    row: u64,
    /// 行内 UTF-16 偏移
    utf16: u64,
    /// 上一字符是否为 `\r`（CRLF 单元的换行计数去重）
    prev_cr: bool,
}

impl ScanCursor {
    /// 消费一段文本，推进游标（换行族 `\n` / `\r\n` / `\r` 语义与行索引一致）。
    fn advance(&mut self, text: &str) {
        for ch in text.chars() {
            match ch {
                '\r' => {
                    self.row += 1;
                    self.utf16 = 0;
                    self.prev_cr = true;
                }
                '\n' => {
                    if !self.prev_cr {
                        self.row += 1;
                        self.utf16 = 0;
                    }
                    self.prev_cr = false;
                }
                _ => {
                    self.utf16 += ch.len_utf16() as u64;
                    self.prev_cr = false;
                }
            }
        }
    }
}

/// 匹配器：标准（字面）或正则；封装查找、接续与替换展开。
enum Matcher {
    /// 字面匹配（大小写可敏感）
    Literal {
        /// 查询文本
        query: String,
        /// 大小写不敏感时的折叠形式（敏感时为空）
        fold: Vec<char>,
        /// 是否大小写敏感
        case_sensitive: bool,
    },
    /// 正则匹配（Rust `regex` 语法；multi-line 使 `^`/`$` 按行匹配）
    Regex(Regex),
}

impl Matcher {
    /// 带全词选项的构建：全词以 `\b` 边界包裹实现（字面模式先 `regex::escape`）。
    ///
    /// 说明：包裹后 `\b` 对以非单词字符开头/结尾的查询可能出现不匹配
    /// （与主流编辑器语义一致；已在文档记录）。
    fn build_opts(
        query: &str,
        case_sensitive: bool,
        mode: SearchMode,
        whole_word: bool,
    ) -> Result<Self, EditError> {
        match (mode, whole_word) {
            (SearchMode::Literal, false) => Ok(Matcher::Literal {
                query: query.to_string(),
                fold: if case_sensitive {
                    Vec::new()
                } else {
                    query.chars().flat_map(fold_chars).collect()
                },
                case_sensitive,
            }),
            (SearchMode::Literal, true) => {
                let pattern = format!(r"\b(?:{})\b", regex::escape(query));
                Self::build_regex(&pattern, case_sensitive)
            }
            (SearchMode::Regex, whole_word) => {
                if whole_word {
                    let pattern = format!(r"\b(?:{})\b", query);
                    Self::build_regex(&pattern, case_sensitive)
                } else {
                    Self::build_regex(query, case_sensitive)
                }
            }
        }
    }

    /// 编译正则匹配器（multi-line 使 `^`/`$` 按行；大小写按开关）。
    fn build_regex(pattern: &str, case_sensitive: bool) -> Result<Self, EditError> {
        let re = regex::RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive)
            .multi_line(true)
            .build()
            .map_err(|error| EditError::InvalidRegex {
                message: error.to_string(),
            })?;
        Ok(Matcher::Regex(re))
    }

    /// 查询是否为空（空查询不产生命中）。
    fn query_is_empty(&self) -> bool {
        match self {
            Matcher::Literal { query, .. } => query.is_empty(),
            Matcher::Regex(re) => re.as_str().is_empty(),
        }
    }

    /// 在 `text` 的 `from` 起查找下一次匹配，返回 `[起, 止)` 字节区间。
    ///
    /// 零宽匹配被跳过（推进一个字符后继续，避免死循环）。
    fn find_at(&self, text: &str, from: usize) -> Option<(usize, usize)> {
        if from > text.len() {
            return None;
        }
        match self {
            Matcher::Literal {
                query,
                fold,
                case_sensitive,
            } => {
                if *case_sensitive {
                    text[from..]
                        .find(query)
                        .map(|index| (from + index, from + index + query.len()))
                } else {
                    for (index, _ch) in text[from..].char_indices() {
                        let at = from + index;
                        if let Some(consumed) = ci_match(&text[at..], fold) {
                            return Some((at, at + consumed));
                        }
                    }
                    None
                }
            }
            Matcher::Regex(re) => {
                let mut cursor = from;
                while cursor <= text.len() {
                    // find_at 保持锚点相对完整 haystack 的语义（切片会伪造行首/串首锚点）
                    let found = re.find_at(text, cursor)?;
                    let start = found.start();
                    let end = found.end();
                    if end > start {
                        return Some((start, end));
                    }
                    // 零宽匹配：跳过当前字符继续
                    cursor = match text[start..].chars().next() {
                        Some(ch) => start + ch.len_utf8(),
                        None => return None,
                    };
                }
                None
            }
        }
    }

    /// 接续区起点：让跨块匹配可被检出，同时避免重复上报已处理的命中。
    fn carry_start(&self, text: &str) -> usize {
        let max_span = match self {
            Matcher::Literal {
                query,
                case_sensitive,
                ..
            } => {
                if *case_sensitive {
                    query.len().saturating_sub(1)
                } else {
                    query.len().saturating_mul(4) + 8
                }
            }
            Matcher::Regex(_) => REGEX_CARRY_BYTES,
        };
        if max_span == 0 || max_span >= text.len() {
            return if max_span == 0 { text.len() } else { 0 };
        }
        ceil_char_boundary(text, text.len() - max_span)
    }

    /// 展开替换文本：正则支持 `$1`/`${name}`（按匹配原文的捕获组）；字面模式原样返回。
    fn expand(&self, matched: &str, replacement: &str) -> String {
        match self {
            Matcher::Literal { .. } => replacement.to_string(),
            Matcher::Regex(re) => match re.captures(matched) {
                Some(caps) => {
                    let mut out = String::with_capacity(replacement.len());
                    caps.expand(replacement, &mut out);
                    out
                }
                None => replacement.to_string(),
            },
        }
    }
}

/// 大小写折叠（查询与文本共用；在 `char::to_lowercase` 之上补两个特例）：
/// - `ς`（希腊终结 sigma，U+03C2）→ `σ`：Unicode 简单折叠规定两者等价（CaseFolding 03C2→03C3），
///   但 `to_lowercase` 不会改写它，否则「νικος」(ς) 搜不到「νικοσ」(σ)；
/// - `İ`（土耳其带点大写 I，U+0130）→ `i`：`to_lowercase` 产生 `i`+组合点（U+0307），
///   会阻断后续字符比对（"İstanbul" 搜不到 "istanbul"）；搜索场景采用 Turkic 友好语义（丢弃组合点）。
fn fold_chars(ch: char) -> std::vec::IntoIter<char> {
    match ch {
        'ς' => vec!['σ'].into_iter(),
        'İ' => vec!['i'].into_iter(),
        _ => ch.to_lowercase().collect::<Vec<_>>().into_iter(),
    }
}

/// 大小写不敏感比对：`hay` 以 `pos` 起始是否匹配折叠后的查询。
///
/// 返回匹配消耗的字节数（按完整字符计）；语义 = 在「折叠文本」上做子串匹配
/// （折叠规则见 `fold_chars`；一个字符折叠为多个字符时按序列整体比对）。
fn ci_match(hay: &str, query_fold: &[char]) -> Option<usize> {
    let mut qi = 0usize;
    let mut consumed = 0usize;
    for ch in hay.chars() {
        for folded in fold_chars(ch) {
            if qi >= query_fold.len() || folded != query_fold[qi] {
                return None;
            }
            qi += 1;
        }
        consumed += ch.len_utf8();
        if qi == query_fold.len() {
            return Some(consumed);
        }
    }
    None
}

/// 截断到 `max_chars` 个字符（超长时追加省略号），用于预览载荷限长。
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

/// 返回 ≥ `index` 的最小字符边界（UTF-8 域）。
fn ceil_char_boundary(text: &str, index: usize) -> usize {
    let mut i = index.min(text.len());
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// 返回 ≥ `index` 的最小字符边界（字节切片级，供 UTF-8 新增片段分块）。
fn ceil_char_boundary_byte(bytes: &[u8], index: usize) -> usize {
    let mut i = index.min(bytes.len());
    while i < bytes.len() && (bytes[i] & 0xC0) == 0x80 {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textfile::encoding::FileEncoding;
    use std::path::PathBuf;

    /// 构造样本文件并打开编辑文档。
    fn open_doc(bytes: &[u8], encoding: Option<FileEncoding>) -> (tempfile::TempDir, EditDoc) {
        let dir = tempfile::tempdir().expect("临时目录失败");
        let path: PathBuf = dir.path().join("search-sample.txt");
        std::fs::write(&path, bytes).expect("写样本失败");
        let doc = EditDoc::open(&path, encoding, 100).expect("打开失败");
        (dir, doc)
    }

    /// 构造命中坐标的测试夹具。
    fn hit(sr: u64, su: u64, er: u64, eu: u64) -> FindHit {
        FindHit {
            start_row: sr,
            start_utf16: su,
            end_row: er,
            end_utf16: eu,
        }
    }

    /// 顺序查找下一个（标准模式）。
    fn find(doc: &EditDoc, query: &str, cs: bool, from: Option<(u64, u64)>) -> Option<FindHit> {
        doc.find(query, cs, SearchMode::Literal, from)
            .expect("查找失败")
    }

    /// 顺序查找下一个（正则模式）。
    fn find_regex(
        doc: &EditDoc,
        query: &str,
        cs: bool,
        from: Option<(u64, u64)>,
    ) -> Option<FindHit> {
        doc.find(query, cs, SearchMode::Regex, from)
            .expect("正则查找失败")
    }

    #[test]
    fn finds_next_occurrence_forward() {
        let (_dir, doc) = open_doc(b"hello world hello", None);
        assert_eq!(find(&doc, "hello", true, None), Some(hit(0, 0, 0, 5)));
        assert_eq!(
            find(&doc, "hello", true, Some((0, 6))),
            Some(hit(0, 12, 0, 17))
        );
        assert_eq!(find(&doc, "hello", true, Some((0, 17))), None);
        assert_eq!(find(&doc, "missing", true, None), None);
    }

    #[test]
    fn case_sensitive_toggle() {
        let (_dir, doc) = open_doc(b"Alpha alpha ALPHA", None);
        assert_eq!(find(&doc, "alpha", true, None), Some(hit(0, 6, 0, 11)));
        assert_eq!(find(&doc, "alpha", false, None), Some(hit(0, 0, 0, 5)));
        assert_eq!(
            find(&doc, "alpha", false, Some((0, 5))),
            Some(hit(0, 6, 0, 11))
        );
        assert_eq!(
            find(&doc, "alpha", false, Some((0, 11))),
            Some(hit(0, 12, 0, 17))
        );
        assert_eq!(find(&doc, "alpha", false, Some((0, 17))), None);
    }

    #[test]
    fn case_insensitive_unicode_fold() {
        let (_dir, doc) = open_doc("CAFÉ café".as_bytes(), None);
        assert_eq!(find(&doc, "café", true, None), Some(hit(0, 5, 0, 9)));
        assert_eq!(find(&doc, "café", false, None), Some(hit(0, 0, 0, 4)));
    }

    /// 西里尔字母大小写不敏感折叠（简单折叠覆盖非拉丁字母）。
    #[test]
    fn cyrillic_case_insensitive_fold() {
        let (_dir, doc) = open_doc("Москва\nмосква".as_bytes(), None);
        assert_eq!(find(&doc, "москва", true, None), Some(hit(1, 0, 1, 6)));
        assert_eq!(find(&doc, "МОСКВА", false, None), Some(hit(0, 0, 0, 6)));
        assert_eq!(
            find(&doc, "москва", false, Some((0, 6))),
            Some(hit(1, 0, 1, 6))
        );
    }

    /// 希腊终结 sigma：大小写不敏感搜索把 ς 与 σ 视为等价（Unicode 简单折叠语义）。
    #[test]
    fn greek_final_sigma_is_equivalent() {
        // 文本含终结 sigma（ς），查询用普通 sigma（σ）——两个方向均须命中。
        let (_dir, doc) = open_doc("νικοσ\u{3C2} τελος".as_bytes(), None);
        assert_eq!(
            find(&doc, "νικοσ\u{3C3}", false, None),
            Some(hit(0, 0, 0, 6))
        );
        let (_dir2, doc2) = open_doc("νικοσ\u{3C3} τελος".as_bytes(), None);
        assert_eq!(
            find(&doc2, "νικοσ\u{3C2}", false, None),
            Some(hit(0, 0, 0, 6))
        );
    }

    /// 土耳其带点大写 İ：大小写不敏感搜索折叠为 `i`（丢弃组合点），"İstanbul" 可匹配 "istanbul"。
    #[test]
    fn turkish_dotted_capital_i_folds_for_search() {
        let (_dir, doc) = open_doc("İSTANBUL".as_bytes(), None);
        assert_eq!(find(&doc, "İSTANBUL", true, None), Some(hit(0, 0, 0, 8)));
        assert_eq!(find(&doc, "istanbul", false, None), Some(hit(0, 0, 0, 8)));
    }

    /// 德语 ß：遵循 Unicode 简单折叠语义（ß 不等于 ss）；同字符的大小写形式正常匹配。
    #[test]
    fn sharp_s_follows_unicode_simple_folding() {
        let (_dir, doc) = open_doc("Straße".as_bytes(), None);
        assert_eq!(find(&doc, "straße", false, None), Some(hit(0, 0, 0, 6)));
        assert_eq!(find(&doc, "STRASSE", false, None), None);
    }

    /// 正则样式字符按字面匹配（本模式不是正则）：
    /// `. * + $ \ ( ) [ ] { } | ^` 全部是普通字符。
    #[test]
    fn regex_like_symbols_are_literal() {
        let (_dir, doc) = open_doc(
            "cost = a+b * (c) [d] {e} | f ^ g $ h \\ i .* j".as_bytes(),
            None,
        );
        assert_eq!(find(&doc, "a+b", true, None), Some(hit(0, 7, 0, 10)));
        assert!(find(&doc, ".*", true, None).is_some());
        assert!(find(&doc, "(c)", true, None).is_some());
        assert!(find(&doc, "\\ i", true, None).is_some());
        assert!(find(&doc, "$ h", true, None).is_some());
        assert!(find(&doc, "^ g", true, None).is_some());
    }

    /// emoji 组合序列（ZWJ）按完整序列字面匹配，UTF-16 坐标正确。
    #[test]
    fn emoji_zwj_sequence_matches_exactly() {
        let (_dir, doc) = open_doc("家人👨\u{200D}👩\u{200D}👧 和朋友".as_bytes(), None);
        let found = find(&doc, "👨\u{200D}👩\u{200D}👧", true, None).expect("ZWJ 序列命中");
        assert_eq!(found.start_row, 0);
        assert_eq!(found.start_utf16, 2);
        assert_eq!(found.end_utf16, 2 + 8); // 👨(2)+ZWJ(1)+👩(2)+ZWJ(1)+👧(2)
    }

    /// 零宽字符（ZWNJ/ZWJ）与双向控制符按字面精确匹配。
    #[test]
    fn zero_width_and_bidi_marks_are_literal() {
        let (_dir, doc) = open_doc("a\u{200C}b a\u{200E}b".as_bytes(), None);
        assert!(find(&doc, "a\u{200C}b", true, None).is_some());
        assert!(find(&doc, "a\u{200E}b", true, None).is_some());
        assert_eq!(find(&doc, "ab", true, None), None); // 中间有零宽字符，不命中
    }

    /// 阿拉伯语（RTL）文本精确匹配；阿拉伯字母无大小写，大小写开关不改变结果。
    #[test]
    fn arabic_rtl_text_matches() {
        let (_dir, doc) = open_doc("مرحبا بالعالم\nسطر ثان".as_bytes(), None);
        assert!(find(&doc, "بالعالم", true, None).is_some());
        assert!(find(&doc, "بالعالم", false, None).is_some());
        assert!(find(&doc, "سطر", true, None).is_some());
    }

    /// 组合附加符号按字节序列精确匹配：NFC 与 NFD 两种形式互不命中（与主流文本编辑器一致）。
    #[test]
    fn combining_mark_forms_match_exactly() {
        let (_dir, doc) = open_doc("café".as_bytes(), None); // NFC：é = U+00E9
        assert_eq!(find(&doc, "café", true, None), Some(hit(0, 0, 0, 4)));
        assert_eq!(find(&doc, "cafe\u{301}", true, None), None);
    }

    /// 全角与半角互为不同字符（不做宽度折叠）；全角的大小写折叠仍正常生效。
    #[test]
    fn fullwidth_and_halfwidth_are_distinct() {
        let (_dir, doc) = open_doc("ＡＢＣ ABC".as_bytes(), None);
        assert_eq!(find(&doc, "ABC", true, None), Some(hit(0, 4, 0, 7)));
        assert_eq!(find(&doc, "ＡＢＣ", true, None), Some(hit(0, 0, 0, 3)));
        assert_eq!(find(&doc, "abc", false, None), Some(hit(0, 4, 0, 7)));
        assert_eq!(find(&doc, "ａｂｃ", false, None), Some(hit(0, 0, 0, 3)));
    }

    /// 日文与韩文查询（含跨行坐标）。
    #[test]
    fn japanese_korean_queries() {
        let (_dir, doc) = open_doc("日本語のテキストです\n한국어 텍스트입니다".as_bytes(), None);
        assert_eq!(find(&doc, "テキスト", true, None), Some(hit(0, 4, 0, 8)));
        assert_eq!(find(&doc, "한국어", true, None), Some(hit(1, 0, 1, 3)));
        assert_eq!(
            find(&doc, "です\n한국어", true, None),
            Some(hit(0, 8, 1, 3))
        );
    }

    #[test]
    fn multiline_query_across_rows() {
        let (_dir, doc) = open_doc(b"aa\nbb\naa\nbb", None);
        assert_eq!(find(&doc, "aa\nbb", true, None), Some(hit(0, 0, 1, 2)));
        assert_eq!(
            find(&doc, "aa\nbb", true, Some((0, 1))),
            Some(hit(2, 0, 3, 2))
        );
    }

    #[test]
    fn crlf_query_extends_over_slash_r() {
        let (_dir, doc) = open_doc(b"a\r\nb", None);
        // 查询 "\nb"：命中从 CRLF 的 '\n' 起始 → 扩展包含 '\r'，坐标可精确解析
        assert_eq!(find(&doc, "\nb", true, None), Some(hit(0, 1, 1, 1)));
        // 普通命中：b 位于第二行行首
        assert_eq!(find(&doc, "b", true, None), Some(hit(1, 0, 1, 1)));
    }

    #[test]
    fn crosses_chunk_boundary() {
        let mut content = "x".repeat(SEARCH_CHUNK_BYTES - 2);
        content.push_str("ZZneedleZZ");
        let (_dir, doc) = open_doc(content.as_bytes(), None);
        let chunk = SEARCH_CHUNK_BYTES as u64;
        // 超长单行 → 显示分段：命中坐标为逻辑坐标经段映射后的显示坐标
        let (sr, su) = doc.seg_of_row_utf16(0, chunk);
        let (er, eu) = doc.seg_of_row_utf16(0, chunk + 6);
        assert_eq!(find(&doc, "needle", true, None), Some(hit(sr, su, er, eu)));
    }

    #[test]
    fn case_insensitive_spanning_chunk_boundary() {
        let mut content = "x".repeat(SEARCH_CHUNK_BYTES - 3);
        content.push_str("John john");
        let (_dir, doc) = open_doc(content.as_bytes(), None);
        let chunk = SEARCH_CHUNK_BYTES as u64;
        let (sr, su) = doc.seg_of_row_utf16(0, chunk - 3);
        let (er, eu) = doc.seg_of_row_utf16(0, chunk + 1);
        assert_eq!(find(&doc, "john", false, None), Some(hit(sr, su, er, eu)));
    }

    #[test]
    fn match_across_edit_pieces() {
        let (_dir, mut doc) = open_doc(b"AAA", None);
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 0,
            text: "nee".to_string(),
        }])
        .expect("首次插入失败");
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 6,
            text: "dle".to_string(),
        }])
        .expect("二次插入失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("neeAAAdle"));
        assert_eq!(find(&doc, "eAAAd", true, None), Some(hit(0, 2, 0, 7)));
    }

    #[test]
    fn replace_next_advances() {
        let (_dir, mut doc) = open_doc(b"foo bar foo", None);
        let outcome = doc
            .replace_next("foo", true, SearchMode::Literal, None, "FOO")
            .expect("替换失败")
            .expect("应有命中");
        assert!(outcome.applied.dirty);
        assert_eq!(doc.row_text(0).as_deref(), Some("FOO bar foo"));
        assert_eq!(outcome.next, Some(hit(0, 8, 0, 11)));
        let second = doc
            .replace_next(
                "foo",
                true,
                SearchMode::Literal,
                outcome.next.map(|h| (h.start_row, h.start_utf16)),
                "FOO",
            )
            .expect("二次替换失败")
            .expect("应有第二次命中");
        assert_eq!(doc.row_text(0).as_deref(), Some("FOO bar FOO"));
        assert_eq!(second.next, None);
        assert!(doc
            .replace_next("foo", true, SearchMode::Literal, None, "X")
            .expect("无命中替换失败")
            .is_none());
    }

    #[test]
    fn replace_all_is_single_undo_step() {
        let (_dir, mut doc) = open_doc(b"aXaXa", None);
        let outcome = doc
            .replace_all("a", true, SearchMode::Literal, "b")
            .expect("全部替换失败");
        assert_eq!(outcome.replaced, 3);
        assert!(outcome.applied.expect("应有编辑结果").dirty);
        assert_eq!(doc.row_text(0).as_deref(), Some("bXbXb"));
        doc.undo().expect("撤销失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("aXaXa"));
        assert!(!doc.is_dirty());
    }

    #[test]
    fn replace_all_no_match_keeps_document() {
        let (_dir, mut doc) = open_doc(b"abc", None);
        let outcome = doc
            .replace_all("z", true, SearchMode::Literal, "y")
            .expect("全部替换失败");
        assert_eq!(outcome.replaced, 0);
        assert!(outcome.applied.is_none());
        assert!(!doc.is_dirty());
    }

    #[test]
    fn replace_all_overflow_is_rejected() {
        let content = "a".repeat(REPLACE_ALL_LIMIT + 1);
        let (_dir, mut doc) = open_doc(content.as_bytes(), None);
        let err = doc
            .replace_all("a", true, SearchMode::Literal, "b")
            .expect_err("应拒绝");
        assert!(matches!(err, EditError::TooManyMatches { .. }));
        assert!(!doc.is_dirty());
        assert_eq!(doc.byte_len(), (REPLACE_ALL_LIMIT + 1) as u64);
    }

    #[test]
    fn utf16_file_search() {
        let text = "hello 世界 hello";
        let bytes: Vec<u8> = text
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect();
        let (_dir, doc) = open_doc(&bytes, Some(FileEncoding::Utf16Le));
        assert_eq!(find(&doc, "世界", true, None), Some(hit(0, 6, 0, 8)));
        assert_eq!(
            find(&doc, "hello", true, Some((0, 7))),
            Some(hit(0, 9, 0, 14))
        );
    }

    #[test]
    fn empty_query_is_noop() {
        let (_dir, mut doc) = open_doc(b"abc", None);
        assert_eq!(find(&doc, "", true, None), None);
        let outcome = doc
            .replace_all("", true, SearchMode::Literal, "x")
            .expect("空查询失败");
        assert_eq!(outcome.replaced, 0);
        assert!(outcome.applied.is_none());
    }

    #[test]
    fn from_row_out_of_range_is_error() {
        let (_dir, doc) = open_doc(b"abc", None);
        let err = doc
            .find("a", true, SearchMode::Literal, Some((99, 0)))
            .expect_err("应报越界");
        assert!(matches!(err, EditError::RowOutOfRange { .. }));
    }

    // ---------- 基础（标准模式）单测与边界单测见上；以下为正则模式 ----------

    /// 正则基础：数字序列匹配与坐标。
    #[test]
    fn regex_finds_number_spans() {
        let (_dir, doc) = open_doc("ab 12 cd 345 ef".as_bytes(), None);
        assert_eq!(find_regex(&doc, r"\d+", true, None), Some(hit(0, 3, 0, 5)));
        assert_eq!(
            find_regex(&doc, r"\d+", true, Some((0, 5))),
            Some(hit(0, 9, 0, 12))
        );
    }

    /// 正则大小写开关映射为 case_insensitive。
    #[test]
    fn regex_case_toggle() {
        let (_dir, doc) = open_doc("Alpha BETA".as_bytes(), None);
        // 敏感：仅小写串 "lpha"（"A"/"B" 均大写）
        assert_eq!(
            find_regex(&doc, "[a-z]+", true, None),
            Some(hit(0, 1, 0, 5))
        );
        // 不敏感：整词 "Alpha"
        assert_eq!(
            find_regex(&doc, "[a-z]+", false, None),
            Some(hit(0, 0, 0, 5))
        );
    }

    /// `^`/`$` 按行锚定（multi-line）。
    #[test]
    fn regex_anchors_are_per_line() {
        let (_dir, doc) = open_doc("aa\nbb\naa".as_bytes(), None);
        assert_eq!(find_regex(&doc, "^aa", true, None), Some(hit(0, 0, 0, 2)));
        assert_eq!(
            find_regex(&doc, "^aa", true, Some((0, 2))),
            Some(hit(2, 0, 2, 2))
        );
        assert_eq!(find_regex(&doc, "aa$", true, None), Some(hit(0, 0, 0, 2)));
    }

    /// 捕获组展开：`$2:$1` 交换顺序（替换一次）。
    #[test]
    fn regex_capture_expansion_in_replace_next() {
        let (_dir, mut doc) = open_doc("user=alice host=box".as_bytes(), None);
        let outcome = doc
            .replace_next(
                r"user=(\w+) host=(\w+)",
                true,
                SearchMode::Regex,
                None,
                "$2:$1",
            )
            .expect("替换失败")
            .expect("应有命中");
        assert!(outcome.applied.dirty);
        assert_eq!(doc.row_text(0).as_deref(), Some("box:alice"));
    }

    /// 无效正则：报 InvalidRegex 且文档不变。
    #[test]
    fn regex_invalid_pattern_is_error() {
        let (_dir, mut doc) = open_doc(b"abc", None);
        let err = doc
            .find("(", true, SearchMode::Regex, None)
            .expect_err("应报无效正则");
        assert!(matches!(err, EditError::InvalidRegex { .. }));
        let err2 = doc
            .replace_all("(", true, SearchMode::Regex, "x")
            .expect_err("应报无效正则");
        assert!(matches!(err2, EditError::InvalidRegex { .. }));
        assert!(!doc.is_dirty());
    }

    /// 零宽匹配被跳过：不会死循环，也不会产生命中。
    #[test]
    fn regex_zero_width_matches_are_skipped() {
        let (_dir, doc) = open_doc(b"bbb", None);
        assert_eq!(find_regex(&doc, r"a*", true, None), None);
        let (_dir2, doc2) = open_doc("aa b".as_bytes(), None);
        assert_eq!(find_regex(&doc2, r"a*", true, None), Some(hit(0, 0, 0, 2)));
    }

    /// 正则跨 1MB 解码块边界命中（固定 64KB 接续区）。
    /// 返回值为**显示坐标**：超长单行按 8KB 分段（段 127 尾 + 段 128 首）。
    #[test]
    fn regex_matches_across_chunk_boundary() {
        let padding = "x".repeat(1_048_576 - 4);
        let content = format!("{padding}NEEDLE_TARGET tail");
        let (_dir, doc) = open_doc(content.as_bytes(), None);
        let found = find_regex(&doc, "NEEDLE_[A-Z]+", true, None).expect("应命中跨块正则");
        assert_eq!(found.start_row, 127);
        assert_eq!(found.start_utf16, 8188);
        assert_eq!(found.end_row, 128);
        assert_eq!(found.end_utf16, 9);
    }

    /// 正则跨行匹配（显式 `\n`）：结束坐标落在下一行。
    #[test]
    fn regex_can_span_lines() {
        let (_dir, doc) = open_doc("foo\nbar".as_bytes(), None);
        assert_eq!(
            find_regex(&doc, "foo\nbar", true, None),
            Some(hit(0, 0, 1, 3))
        );
    }

    /// 预览：条目内容与截断标记。
    #[test]
    fn preview_lists_items_and_truncates() {
        let (_dir, doc) = open_doc("cat bat cat hat cat".as_bytes(), None);
        let preview = doc
            .preview_replace_all("cat", true, SearchMode::Literal, "dog", 2)
            .expect("预览失败");
        assert_eq!(preview.total, 3);
        assert!(preview.truncated);
        assert_eq!(preview.items.len(), 2);
        assert_eq!(preview.items[0].matched_text, "cat");
        assert_eq!(preview.items[0].replacement_text, "dog");
        assert_eq!(preview.items[0].line_text, "cat bat cat hat cat");
        assert_eq!(preview.state_id, doc.state_id());
    }

    /// 预览后按选中下标执行（剔除个别项）；单撤销步。
    #[test]
    fn replace_selected_subset_single_undo() {
        let (_dir, mut doc) = open_doc("a1 b2 c3".as_bytes(), None);
        let preview = doc
            .preview_replace_all(r"\d", true, SearchMode::Regex, "X", 10)
            .expect("预览失败");
        assert_eq!(preview.total, 3);
        let outcome = doc
            .replace_matches(
                r"\d",
                true,
                SearchMode::Regex,
                "X",
                Some(&[0, 2]),
                preview.state_id,
            )
            .expect("执行失败");
        assert_eq!(outcome.replaced, 2);
        assert_eq!(doc.row_text(0).as_deref(), Some("aX b2 cX"));
        doc.undo().expect("撤销失败");
        assert_eq!(doc.row_text(0).as_deref(), Some("a1 b2 c3"));
        assert!(!doc.is_dirty());
    }

    /// 预览后文档变化：state_id 不一致报 StaleSearch。
    #[test]
    fn replace_matches_stale_preview_is_rejected() {
        let (_dir, mut doc) = open_doc(b"abc", None);
        let preview = doc
            .preview_replace_all("a", true, SearchMode::Literal, "z", 10)
            .expect("预览失败");
        // 预览后修改文档（状态号推进）
        doc.apply_edits(&[EditOp::Insert {
            row: 0,
            utf16: 0,
            text: "!".to_string(),
        }])
        .expect("编辑失败");
        let err = doc
            .replace_matches("a", true, SearchMode::Literal, "z", None, preview.state_id)
            .expect_err("应报过期");
        assert!(matches!(err, EditError::StaleSearch));
    }

    /// 高亮窗口：只返回窗口内命中，窗口外扫描提前结束。
    #[test]
    fn match_window_returns_only_window_hits() {
        let content = (0..10)
            .map(|i| format!("line{i} x"))
            .collect::<Vec<_>>()
            .join("\n");
        let (_dir, doc) = open_doc(content.as_bytes(), None);
        let hits = doc
            .match_window("line", true, SearchMode::Literal, 3, 3)
            .expect("窗口查找失败");
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].start_row, 3);
        assert_eq!(hits[2].start_row, 5);
    }

    // ---------- P1-6：全词 / 计数 / 超时 / 行区间上界 ----------

    /// 构造请求的测试助手。
    fn req<'a>(
        query: &'a str,
        case_sensitive: bool,
        mode: SearchMode,
        whole_word: bool,
    ) -> SearchRequest<'a> {
        SearchRequest {
            query,
            case_sensitive,
            mode,
            whole_word,
            timeout_ms: None,
        }
    }

    /// 全词（字面）：词内子串不命中，独立词与标点相邻词命中。
    #[test]
    fn whole_word_literal_boundaries() {
        let (_dir, doc) = open_doc("cat concatenate cat! cat-cat 猫cat".as_bytes(), None);
        let request = req("cat", true, SearchMode::Literal, true);
        let first = doc.find_request(&request, None, None).expect("查找失败");
        assert_eq!(first, Some(hit(0, 0, 0, 3)));
        let second = doc
            .find_request(&request, Some((0, 3)), None)
            .expect("查找失败");
        assert_eq!(second, Some(hit(0, 16, 0, 19)));
        let third = doc
            .find_request(&request, Some((0, 19)), None)
            .expect("查找失败");
        assert_eq!(third, Some(hit(0, 21, 0, 24)));
        let fourth = doc
            .find_request(&request, Some((0, 24)), None)
            .expect("查找失败");
        assert_eq!(fourth, Some(hit(0, 25, 0, 28)));
        let fifth = doc
            .find_request(&request, Some((0, 28)), None)
            .expect("查找失败");
        assert_eq!(fifth, None);
    }

    /// 全词（正则）+ 大小写不敏感。
    #[test]
    fn whole_word_regex_case_insensitive() {
        let (_dir, doc) = open_doc("Word wordy WORD".as_bytes(), None);
        let request = req("word", false, SearchMode::Regex, true);
        let first = doc.find_request(&request, None, None).expect("查找失败");
        assert_eq!(first, Some(hit(0, 0, 0, 4)));
        let second = doc
            .find_request(&request, Some((0, 4)), None)
            .expect("查找失败");
        assert_eq!(second, Some(hit(0, 11, 0, 15)));
    }

    /// 计数：总数、上限截断、空查询。
    #[test]
    fn count_respects_limit_and_reports_truncation() {
        let (_dir, doc) = open_doc(b"a a a a", None);
        let request = req("a", true, SearchMode::Literal, false);
        let all = doc.count_request(&request).expect("计数失败");
        assert_eq!(all.total, 4);
        assert!(!all.truncated);
        let capped = doc.count_request_with(&request, 3).expect("计数失败");
        assert_eq!(capped.total, 3);
        assert!(capped.truncated);
        let empty = doc
            .count_request(&req("", true, SearchMode::Literal, false))
            .expect("空查询失败");
        assert_eq!(empty.total, 0);
        assert!(!empty.truncated);
    }

    /// 行区间上界：`until_row` 之后不再命中（含 until 行本身）。
    #[test]
    fn find_request_respects_until_row() {
        let content = "x1\nx2\nx3\nx4\nx5";
        let (_dir, doc) = open_doc(content.as_bytes(), None);
        let request = req("x", true, SearchMode::Literal, false);
        let found = doc
            .find_request(&request, Some((2, 0)), Some(3))
            .expect("查找失败");
        assert_eq!(found, Some(hit(2, 0, 2, 1)));
        let after = doc
            .find_request(&request, Some((3, 1)), Some(3))
            .expect("查找失败");
        assert_eq!(after, None, "until_row 之后不应命中（含 x5 在第 4 行）");
    }

    /// 超时：正则 + 极小超时 + 大文档 → RegexTimeout，且替换路径不修改内容。
    #[test]
    fn regex_timeout_interrupts_scan() {
        let content = "abcdefgh\n".repeat(500_000);
        let (_dir, mut doc) = open_doc(content.as_bytes(), None);
        let request = SearchRequest {
            query: "z9",
            case_sensitive: true,
            mode: SearchMode::Regex,
            whole_word: false,
            timeout_ms: Some(50),
        };
        let err = doc.find_request(&request, None, None).expect_err("应超时");
        assert!(matches!(err, EditError::RegexTimeout));
        let before = doc.byte_len();
        let replace_err = doc
            .replace_all_request(&request, "x")
            .expect_err("替换也应超时");
        assert!(matches!(replace_err, EditError::RegexTimeout));
        assert_eq!(doc.byte_len(), before);
        assert!(!doc.is_dirty());
    }
}
