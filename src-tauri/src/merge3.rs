//! 三方合并引擎（标准 diff3）：
//! `diff(base, ours)` 与 `diff(base, theirs)` 的变更区间按 base 坐标对齐成集群，
//! 逐集群分类为：仅我方改 / 仅他方改 / 双方同改一致 / 冲突。
//!
//! 输出以「来源 + 行区间」表达合并内容（引擎只持有行哈希，文本由命令层切片）。

use crate::diff::{diff_lines, DiffError, DiffHunk, HunkKind};

/// 合并内容来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MergedSource {
    Base,
    Ours,
    Theirs,
}

/// 区域类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RegionKind {
    /// 三方一致。
    Stable,
    /// 仅我方修改。
    OursOnly,
    /// 仅他方修改。
    TheirsOnly,
    /// 双方修改且结果一致。
    SameChange,
    /// 双方修改不一致（需人工选择）。
    Conflict,
}

/// 行区间（`[start, end)`，行号为该文件内坐标）。
pub type LineSpan = (u64, u64);

/// 单个合并区域。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRegion {
    pub kind: RegionKind,
    /// base 侧区间。
    pub base_range: LineSpan,
    /// ours 侧区间（含映射；Stable/SameChange 亦有值）。
    pub ours_range: LineSpan,
    /// theirs 侧区间（含映射；Stable/SameChange 亦有值）。
    pub theirs_range: LineSpan,
    /// 非冲突区域的合并内容来源与区间；冲突为 `None`。
    pub merged: Option<(MergedSource, LineSpan)>,
}

/// 合并结果。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub regions: Vec<MergeRegion>,
    pub conflicts: u32,
    /// 非冲突区域的合并输出总行数（含 Stable）。
    pub auto_merged_lines: u64,
}

/// 对三个行哈希序列执行三方合并。
pub fn merge3(base: &[u64], ours: &[u64], theirs: &[u64]) -> Result<MergeResult, DiffError> {
    let diff_ours = diff_lines(base, ours)?;
    let diff_theirs = diff_lines(base, theirs)?;
    let ours_hunks = diff_ours.hunks;
    let theirs_hunks = diff_theirs.hunks;
    let ours_changes = collect_changes(&ours_hunks);
    let theirs_changes = collect_changes(&theirs_hunks);

    let mut regions: Vec<MergeRegion> = Vec::new();
    let mut conflicts = 0u32;
    let mut auto_lines = 0u64;
    let mut base_cursor = 0u64;
    let mut ours_index = 0usize;
    let mut theirs_index = 0usize;

    while ours_index < ours_changes.len() || theirs_index < theirs_changes.len() {
        let next_ours = ours_changes.get(ours_index).map(|item| item.base.0);
        let next_theirs = theirs_changes.get(theirs_index).map(|item| item.base.0);
        let cluster_start = match (next_ours, next_theirs) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => break,
        };
        if base_cursor < cluster_start {
            push_region(
                &mut regions,
                &mut auto_lines,
                RegionKind::Stable,
                (base_cursor, cluster_start),
                map_span(&ours_hunks, base_cursor, cluster_start),
                map_span(&theirs_hunks, base_cursor, cluster_start),
                Some((MergedSource::Base, (base_cursor, cluster_start))),
            );
            base_cursor = cluster_start;
        }

        let mut cluster_end = cluster_start;
        let mut ours_changed = false;
        let mut theirs_changed = false;
        loop {
            let mut progressed = false;
            while ours_index < ours_changes.len() && ours_changes[ours_index].base.0 <= cluster_end
            {
                cluster_end = cluster_end.max(ours_changes[ours_index].base.1);
                ours_index += 1;
                ours_changed = true;
                progressed = true;
            }
            while theirs_index < theirs_changes.len()
                && theirs_changes[theirs_index].base.0 <= cluster_end
            {
                cluster_end = cluster_end.max(theirs_changes[theirs_index].base.1);
                theirs_index += 1;
                theirs_changed = true;
                progressed = true;
            }
            if !progressed {
                break;
            }
        }

        let ours_span = map_span(&ours_hunks, cluster_start, cluster_end);
        let theirs_span = map_span(&theirs_hunks, cluster_start, cluster_end);
        let base_span = (cluster_start, cluster_end);
        if ours_changed && theirs_changed {
            let ours_slice = &ours[ours_span.0 as usize..ours_span.1 as usize];
            let theirs_slice = &theirs[theirs_span.0 as usize..theirs_span.1 as usize];
            if ours_slice == theirs_slice {
                push_region(
                    &mut regions,
                    &mut auto_lines,
                    RegionKind::SameChange,
                    base_span,
                    ours_span,
                    theirs_span,
                    Some((MergedSource::Ours, ours_span)),
                );
            } else {
                conflicts += 1;
                push_region(
                    &mut regions,
                    &mut auto_lines,
                    RegionKind::Conflict,
                    base_span,
                    ours_span,
                    theirs_span,
                    None,
                );
            }
        } else if ours_changed {
            push_region(
                &mut regions,
                &mut auto_lines,
                RegionKind::OursOnly,
                base_span,
                ours_span,
                theirs_span,
                Some((MergedSource::Ours, ours_span)),
            );
        } else if theirs_changed {
            push_region(
                &mut regions,
                &mut auto_lines,
                RegionKind::TheirsOnly,
                base_span,
                ours_span,
                theirs_span,
                Some((MergedSource::Theirs, theirs_span)),
            );
        }
        base_cursor = cluster_end;
    }
    if base_cursor < base.len() as u64 {
        let span = (base_cursor, base.len() as u64);
        push_region(
            &mut regions,
            &mut auto_lines,
            RegionKind::Stable,
            span,
            map_span(&ours_hunks, span.0, span.1),
            map_span(&theirs_hunks, span.0, span.1),
            Some((MergedSource::Base, span)),
        );
    }
    Ok(MergeResult {
        regions,
        conflicts,
        auto_merged_lines: auto_lines,
    })
}

/// 按区域合并结果物化输出（行哈希）。
pub fn materialized(result: &MergeResult, base: &[u64], ours: &[u64], theirs: &[u64]) -> Vec<u64> {
    let mut out = Vec::new();
    for region in &result.regions {
        if let Some((source, (start, end))) = region.merged {
            let lines = match source {
                MergedSource::Base => base,
                MergedSource::Ours => ours,
                MergedSource::Theirs => theirs,
            };
            out.extend_from_slice(&lines[start as usize..end as usize]);
        }
    }
    out
}

fn push_region(
    regions: &mut Vec<MergeRegion>,
    auto_lines: &mut u64,
    kind: RegionKind,
    base_range: LineSpan,
    ours_range: LineSpan,
    theirs_range: LineSpan,
    merged: Option<(MergedSource, LineSpan)>,
) {
    if let Some((_, (start, end))) = merged {
        *auto_lines += end - start;
    }
    regions.push(MergeRegion {
        kind,
        base_range,
        ours_range,
        theirs_range,
        merged,
    });
}

/// 变更区间（base 与源侧坐标）。
struct Change {
    base: LineSpan,
    source: LineSpan,
}

fn collect_changes(diff: &[DiffHunk]) -> Vec<Change> {
    diff.iter()
        .filter(|hunk| hunk.kind != HunkKind::Equal)
        .map(|hunk| Change {
            base: (hunk.left_start, hunk.left_start + hunk.left_len),
            source: (hunk.right_start, hunk.right_start + hunk.right_len),
        })
        .collect()
}

/// 将 base 区间 `[cs, ce)` 映射到源侧区间。
fn map_span(diff: &[DiffHunk], cs: u64, ce: u64) -> LineSpan {
    (map_boundary(diff, cs, false), map_boundary(diff, ce, true))
}

/// 边界映射：`after` 决定零长度插入点取插入前还是插入后位置。
fn map_boundary(diff: &[DiffHunk], pos: u64, after: bool) -> u64 {
    let mut delta: i64 = 0;
    for hunk in diff {
        let b_start = hunk.left_start;
        let b_end = hunk.left_start + hunk.left_len;
        if hunk.kind == HunkKind::Equal {
            if pos >= b_start && pos < b_end {
                return (b_start as i64 + delta + (pos - b_start) as i64) as u64;
            }
            if pos < b_start {
                return (pos as i64 + delta) as u64;
            }
            delta += hunk.right_len as i64 - hunk.left_len as i64;
        } else {
            if pos < b_start {
                return (pos as i64 + delta) as u64;
            }
            if pos == b_start {
                let base = b_start as i64 + delta;
                return if after && hunk.left_len == 0 {
                    (base + hunk.right_len as i64) as u64
                } else {
                    base as u64
                };
            }
            if pos < b_end {
                return (b_start as i64 + delta + hunk.right_len as i64) as u64;
            }
            if pos == b_end {
                return (b_start as i64 + delta + hunk.right_len as i64) as u64;
            }
            delta += hunk.right_len as i64 - hunk.left_len as i64;
        }
    }
    (pos as i64 + delta) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::hash_lines;

    fn h(text: &str) -> Vec<u64> {
        hash_lines(text.as_bytes())
    }

    fn kinds(result: &MergeResult) -> Vec<RegionKind> {
        result.regions.iter().map(|region| region.kind).collect()
    }

    #[test]
    fn all_identical_single_stable() {
        let base = h("a\nb\nc\n");
        let result = merge3(&base, &base, &base).unwrap();
        assert_eq!(kinds(&result), vec![RegionKind::Stable]);
        assert_eq!(result.conflicts, 0);
        assert_eq!(result.auto_merged_lines, 3);
        assert_eq!(materialized(&result, &base, &base, &base), base);
    }

    #[test]
    fn ours_only_change_wins() {
        let base = h("a\nb\nc\n");
        let ours = h("a\nB\nc\n");
        let result = merge3(&base, &ours, &base).unwrap();
        assert_eq!(
            kinds(&result),
            vec![RegionKind::Stable, RegionKind::OursOnly, RegionKind::Stable]
        );
        assert_eq!(result.conflicts, 0);
        assert_eq!(materialized(&result, &base, &ours, &base), ours);
    }

    #[test]
    fn both_sides_different_regions_auto_merge() {
        let base = h("1\n2\n3\n4\n5\n");
        let ours = h("1\nX\n3\n4\n5\n");
        let theirs = h("1\n2\n3\n4\nY\n");
        let result = merge3(&base, &ours, &theirs).unwrap();
        assert_eq!(result.conflicts, 0);
        let merged = materialized(&result, &base, &ours, &theirs);
        assert_eq!(merged, h("1\nX\n3\n4\nY\n"));
    }

    #[test]
    fn same_change_on_both_sides_is_same_change() {
        let base = h("a\nb\nc\n");
        let ours = h("a\nZ\nc\n");
        let theirs = h("a\nZ\nc\n");
        let result = merge3(&base, &ours, &theirs).unwrap();
        assert_eq!(
            kinds(&result),
            vec![
                RegionKind::Stable,
                RegionKind::SameChange,
                RegionKind::Stable
            ]
        );
        assert_eq!(result.conflicts, 0);
        assert_eq!(materialized(&result, &base, &ours, &theirs), ours);
    }

    #[test]
    fn conflicting_change_marks_conflict() {
        let base = h("a\nb\nc\n");
        let ours = h("a\nX\nc\n");
        let theirs = h("a\nY\nc\n");
        let result = merge3(&base, &ours, &theirs).unwrap();
        assert_eq!(result.conflicts, 1);
        let conflict = result
            .regions
            .iter()
            .find(|region| region.kind == RegionKind::Conflict)
            .unwrap();
        assert_eq!(conflict.merged, None);
        assert_eq!(conflict.base_range, (1, 2));
        assert_eq!(conflict.ours_range, (1, 2));
        assert_eq!(conflict.theirs_range, (1, 2));
        assert_eq!(result.auto_merged_lines, 2);
    }

    #[test]
    fn adjacent_insert_and_delete_touch_conflict() {
        let base = h("1\n2\n3\n");
        let ours = h("1\nnew\n2\n3\n");
        let theirs = h("1\n3\n");
        let result = merge3(&base, &ours, &theirs).unwrap();
        assert_eq!(result.conflicts, 1);
        let conflict = result
            .regions
            .iter()
            .find(|region| region.kind == RegionKind::Conflict)
            .unwrap();
        assert_eq!(conflict.ours_range, (1, 3));
        assert_eq!(conflict.theirs_range, (1, 1));
    }

    #[test]
    fn empty_base_takes_both_sides() {
        let empty: Vec<u64> = Vec::new();
        let ours = h("a\n");
        let theirs = h("a\n");
        let result = merge3(&empty, &ours, &theirs).unwrap();
        assert_eq!(result.conflicts, 0);
        assert_eq!(kinds(&result), vec![RegionKind::SameChange]);
        assert_eq!(materialized(&result, &empty, &ours, &theirs), ours);

        let theirs_diff = h("b\n");
        let result = merge3(&empty, &ours, &theirs_diff).unwrap();
        assert_eq!(result.conflicts, 1);
        assert_eq!(materialized(&result, &empty, &ours, &theirs_diff).len(), 0);
    }

    #[test]
    fn deletion_only_on_one_side() {
        let base = h("a\nb\nc\n");
        let ours = h("a\nc\n");
        let result = merge3(&base, &ours, &base).unwrap();
        assert_eq!(result.conflicts, 0);
        assert_eq!(materialized(&result, &base, &ours, &base), ours);
    }

    #[test]
    fn randomized_merge_reconstructs_when_one_side_untouched() {
        // 一侧未改时，合并结果必须逐行等于另一侧
        let mut seed = 0x1234_5678_9abc_def0_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..60 {
            let n = (next() % 40) as usize;
            let base: Vec<u64> = (0..n).map(|_| next() % 4).collect();
            let m = (next() % 40) as usize;
            let ours: Vec<u64> = (0..m).map(|_| next() % 4).collect();
            let result = merge3(&base, &ours, &base).unwrap();
            assert_eq!(result.conflicts, 0, "单侧修改不应产生冲突");
            let merged = materialized(&result, &base, &ours, &base);
            assert_eq!(
                merged, ours,
                "单侧未改时应完整保留修改侧；base={base:?} regions={:?}",
                result.regions
            );
        }
    }
}
