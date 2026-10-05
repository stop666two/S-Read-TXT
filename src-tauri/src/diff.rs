//! 行级 diff 引擎：FNV-1a 行哈希 + Myers O(ND)（`D` 有界，超界报 `TooComplex`）。
//!
//! 适用范围：文件比较与三方合并。对超大且高度相异的输入（`D` 超出保护上限）
//! 不返回错误结果，而是显式失败，由上层降级处理。

/// 行哈希（FNV-1a 64 位；按 `\n` 分行，`\r` 参与哈希以区分 CRLF）。
pub fn hash_lines(bytes: &[u8]) -> Vec<u64> {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut lines = Vec::new();
    let mut hash = OFFSET;
    let mut has_content = false;
    for &byte in bytes {
        if byte == b'\n' {
            lines.push(hash);
            hash = OFFSET;
            has_content = false;
        } else {
            hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
            has_content = true;
        }
    }
    if has_content {
        lines.push(hash);
    }
    lines
}

/// 单行哈希（FNV-1a 64 位；空行返回偏移基值，与 [`hash_lines`] 的空行一致）。
pub fn hash_line(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for &byte in bytes {
        hash = (hash ^ u64::from(byte)).wrapping_mul(PRIME);
    }
    hash
}

/// diff 计算错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffError {
    /// 差异规模超出保护上限（上层应降级提示而非静默错误结果）。
    TooComplex,
}

impl std::fmt::Display for DiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiffError::TooComplex => write!(f, "差异规模过大，无法精细比较"),
        }
    }
}

/// 供命令层转 IpcError 的错误码。
pub const DIFF_TOO_COMPLEX: &str = "DIFF_TOO_COMPLEX";

/// 差异块类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HunkKind {
    /// 两侧一致。
    Equal,
    /// 两侧都有差异。
    Change,
    /// 仅左侧存在（右侧删除区）。
    Delete,
    /// 仅右侧存在（右侧新增区）。
    Insert,
}

/// 差异块（行号从 0 开始；Change 两侧长度均 > 0）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub kind: HunkKind,
    pub left_start: u64,
    pub left_len: u64,
    pub right_start: u64,
    pub right_len: u64,
}

/// 差异结果：全文件覆盖的块序列（含 Equal 块）+ 统计。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffResult {
    pub hunks: Vec<DiffHunk>,
    pub added: u64,
    pub removed: u64,
}

/// Myers 保护上限：单侧编辑距离超过该值即报 `TooComplex`（同步保护内存/时间）。
const D_CAP: usize = 2048;

enum RawOp {
    /// (a 局部起点, b 局部起点, 长度)
    Eq(usize, usize, usize),
    /// (a 局部起点, 长度)
    Del(usize, usize),
    /// (b 局部起点, 长度)
    Ins(usize, usize),
}

/// 计算左右行哈希序列的差异（含 Equal 块；无差异时返回空块列表）。
pub fn diff_lines(left: &[u64], right: &[u64]) -> Result<DiffResult, DiffError> {
    if left == right {
        return Ok(DiffResult {
            hunks: Vec::new(),
            added: 0,
            removed: 0,
        });
    }
    let n = left.len();
    let m = right.len();
    let mut prefix = 0usize;
    while prefix < n && prefix < m && left[prefix] == right[prefix] {
        prefix += 1;
    }
    let mut suffix = 0usize;
    while suffix < n - prefix
        && suffix < m - prefix
        && left[n - 1 - suffix] == right[m - 1 - suffix]
    {
        suffix += 1;
    }

    let mut ops: Vec<RawOp> = Vec::new();
    if prefix > 0 {
        ops.push(RawOp::Eq(0, 0, prefix));
    }
    let mid_left = &left[prefix..n - suffix];
    let mid_right = &right[prefix..m - suffix];
    if !mid_left.is_empty() || !mid_right.is_empty() {
        for op in myers(mid_left, mid_right)? {
            ops.push(match op {
                RawOp::Eq(i, j, len) => RawOp::Eq(i + prefix, j + prefix, len),
                RawOp::Del(i, len) => RawOp::Del(i + prefix, len),
                RawOp::Ins(j, len) => RawOp::Ins(j + prefix, len),
            });
        }
    }
    if suffix > 0 {
        ops.push(RawOp::Eq(n - suffix, m - suffix, suffix));
    }
    Ok(build_result(&ops, left.len(), right.len()))
}

/// 合并原始操作为块序列并统计增删。
fn build_result(ops: &[RawOp], left_total: usize, right_total: usize) -> DiffResult {
    let mut hunks = Vec::new();
    let mut added = 0u64;
    let mut removed = 0u64;
    let mut la = 0u64;
    let mut ra = 0u64;
    let mut index = 0usize;
    while index < ops.len() {
        match ops[index] {
            RawOp::Eq(_, _, len) => {
                if len > 0 {
                    hunks.push(DiffHunk {
                        kind: HunkKind::Equal,
                        left_start: la,
                        left_len: len as u64,
                        right_start: ra,
                        right_len: len as u64,
                    });
                    la += len as u64;
                    ra += len as u64;
                }
                index += 1;
            }
            _ => {
                let start_la = la;
                let start_ra = ra;
                let mut left_len = 0u64;
                let mut right_len = 0u64;
                while index < ops.len() && !matches!(ops[index], RawOp::Eq(..)) {
                    match ops[index] {
                        RawOp::Del(_, len) => {
                            left_len += len as u64;
                            la += len as u64;
                        }
                        RawOp::Ins(_, len) => {
                            right_len += len as u64;
                            ra += len as u64;
                        }
                        RawOp::Eq(..) => unreachable!(),
                    }
                    index += 1;
                }
                let kind = if left_len > 0 && right_len > 0 {
                    HunkKind::Change
                } else if left_len > 0 {
                    HunkKind::Delete
                } else {
                    HunkKind::Insert
                };
                added += right_len;
                removed += left_len;
                hunks.push(DiffHunk {
                    kind,
                    left_start: start_la,
                    left_len,
                    right_start: start_ra,
                    right_len,
                });
            }
        }
    }
    debug_assert_eq!(la, left_total as u64);
    debug_assert_eq!(ra, right_total as u64);
    DiffResult {
        hunks,
        added,
        removed,
    }
}

/// Myers O(ND)：返回按序原始操作；`D` 超过保护上限报 `TooComplex`。
fn myers(a: &[u64], b: &[u64]) -> Result<Vec<RawOp>, DiffError> {
    let n = a.len() as i32;
    let m = b.len() as i32;
    if n == 0 && m == 0 {
        return Ok(Vec::new());
    }
    let max_d = n + m;
    let cap = max_d.min(D_CAP as i32) as usize;
    let offset = cap as i64;
    let size = 2 * cap + 1;
    let mut v = vec![0i32; size];
    let mut trace: Vec<Vec<i32>> = Vec::with_capacity(cap + 1);
    let mut found = None;
    'outer: for d in 0..=cap as i32 {
        let mut k = -d;
        while k <= d {
            let ki = (k as i64 + offset) as usize;
            let mut x;
            if k == -d || (k != d && v[ki - 1] < v[ki + 1]) {
                x = v[ki + 1];
            } else {
                x = v[ki - 1] + 1;
            }
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[ki] = x;
            if x >= n && y >= m {
                found = Some(d);
                trace.push(v.clone());
                break 'outer;
            }
            k += 2;
        }
        trace.push(v.clone());
    }
    let Some(found_d) = found else {
        return Err(DiffError::TooComplex);
    };
    Ok(backtrack(&trace, a, b, n, m, found_d, offset as i32))
}

/// 从 trace 回溯出原始操作序列（正序返回）。
fn backtrack(
    trace: &[Vec<i32>],
    _a: &[u64],
    _b: &[u64],
    n: i32,
    m: i32,
    found_d: i32,
    offset: i32,
) -> Vec<RawOp> {
    let mut ops: Vec<RawOp> = Vec::new();
    let mut x = n;
    let mut y = m;
    for d in (0..=found_d).rev() {
        let v = &trace[d as usize];
        let k = x - y;
        let prev_k =
            if k == -d || (k != d && v[(k - 1 + offset) as usize] < v[(k + 1 + offset) as usize]) {
                k + 1
            } else {
                k - 1
            };
        let prev_x = v[(prev_k + offset) as usize];
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            x -= 1;
            y -= 1;
            ops.push(RawOp::Eq(x as usize, y as usize, 1));
        }
        if d > 0 {
            if x == prev_x {
                y -= 1;
                ops.push(RawOp::Ins(y as usize, 1));
            } else {
                x -= 1;
                ops.push(RawOp::Del(x as usize, 1));
            }
        }
    }
    ops.reverse();
    ops.shrink_to_fit();
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(text: &str) -> Vec<u64> {
        hash_lines(text.as_bytes())
    }

    fn apply(hunks: &[DiffHunk], left: &[u64], right: &[u64], kind: HunkKind) -> bool {
        hunks.iter().any(|hunk| hunk.kind == kind)
            && hunks
                .iter()
                .filter(|hunk| hunk.kind != HunkKind::Equal)
                .all(|hunk| {
                    hunk.right_start + hunk.right_len <= right.len() as u64
                        && hunk.left_start + hunk.left_len <= left.len() as u64
                })
    }

    /// 由块序列重建右侧序列（用于随机对拍的一致性校验）。
    fn rebuild_right(hunks: &[DiffHunk], left: &[u64], right: &[u64]) -> Vec<u64> {
        let mut out = Vec::new();
        let mut entries: Vec<_> = hunks.iter().collect();
        entries.sort_by_key(|hunk| hunk.left_start);
        for hunk in entries {
            match hunk.kind {
                HunkKind::Equal => {
                    out.extend_from_slice(
                        &left[hunk.left_start as usize..(hunk.left_start + hunk.left_len) as usize],
                    );
                }
                HunkKind::Delete => {}
                HunkKind::Insert | HunkKind::Change => {
                    out.extend_from_slice(
                        &right[hunk.right_start as usize
                            ..(hunk.right_start + hunk.right_len) as usize],
                    );
                }
            }
        }
        out
    }

    fn naive_lcs(a: &[u64], b: &[u64]) -> usize {
        let n = a.len();
        let m = b.len();
        let mut dp = vec![vec![0usize; m + 1]; n + 1];
        for i in 1..=n {
            for j in 1..=m {
                dp[i][j] = if a[i - 1] == b[j - 1] {
                    dp[i - 1][j - 1] + 1
                } else {
                    dp[i - 1][j].max(dp[i][j - 1])
                };
            }
        }
        dp[n][m]
    }

    #[test]
    fn hash_lines_handles_trailing_and_crlf() {
        assert_eq!(h("a\nb").len(), 2);
        assert_eq!(h("a\nb\n").len(), 2);
        assert_eq!(h("").len(), 0);
        assert_eq!(h("\n").len(), 1);
        assert_ne!(h("a\r\n"), h("a\n"));
        assert_eq!(h("a\nb\n"), h("a\nb"));
        assert_eq!(h("a\n").len(), 1);
    }

    #[test]
    fn identical_returns_empty_hunks() {
        let a = h("x\ny\nz\n");
        let result = diff_lines(&a, &a).unwrap();
        assert!(result.hunks.is_empty());
        assert_eq!(result.added, 0);
        assert_eq!(result.removed, 0);
    }

    #[test]
    fn single_change_and_edges() {
        let left = h("1\n2\n3\n4\n");
        let right = h("1\n9\n3\n4\n");
        let result = diff_lines(&left, &right).unwrap();
        assert_eq!(result.added, 1);
        assert_eq!(result.removed, 1);
        let kinds: Vec<HunkKind> = result.hunks.iter().map(|hunk| hunk.kind).collect();
        assert_eq!(
            kinds,
            vec![HunkKind::Equal, HunkKind::Change, HunkKind::Equal]
        );
        assert_eq!(result.hunks[1].left_start, 1);
        assert_eq!(result.hunks[1].right_start, 1);
        assert_eq!(result.hunks[2].left_len, 2);
    }

    #[test]
    fn pure_insert_delete_and_empty_sides() {
        let empty: Vec<u64> = Vec::new();
        let right = h("a\nb\n");
        let result = diff_lines(&empty, &right).unwrap();
        assert_eq!(result.hunks.len(), 1);
        assert_eq!(result.hunks[0].kind, HunkKind::Insert);
        assert_eq!(result.added, 2);

        let left = h("a\nb\n");
        let result = diff_lines(&left, &empty).unwrap();
        assert_eq!(result.hunks[0].kind, HunkKind::Delete);
        assert_eq!(result.removed, 2);
    }

    #[test]
    fn all_different_is_change() {
        let left = h("a\n");
        let right = h("b\n");
        let result = diff_lines(&left, &right).unwrap();
        assert_eq!(result.hunks.len(), 1);
        assert_eq!(result.hunks[0].kind, HunkKind::Change);
        assert!(apply(&result.hunks, &left, &right, HunkKind::Change));
    }

    #[test]
    fn random_cross_check_against_naive_lcs() {
        let mut seed = 0x9e37_79b9_u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..200 {
            let n = (next() % 61) as usize;
            let m = (next() % 61) as usize;
            let a: Vec<u64> = (0..n).map(|_| next() % 5).collect();
            let b: Vec<u64> = (0..m).map(|_| next() % 5).collect();
            let result = diff_lines(&a, &b).unwrap();
            let lcs = naive_lcs(&a, &b);
            assert_eq!(
                result.added,
                (m - lcs) as u64,
                "added 不一致：a={a:?} b={b:?}"
            );
            assert_eq!(
                result.removed,
                (n - lcs) as u64,
                "removed 不一致：a={a:?} b={b:?}"
            );
            assert_eq!(rebuild_right(&result.hunks, &a, &b), b, "重建不一致");
        }
    }

    #[test]
    fn too_complex_beyond_cap() {
        let a: Vec<u64> = (0..3000).map(|value| value * 2).collect();
        let b: Vec<u64> = (0..3000).map(|value| value * 2 + 1).collect();
        let result = diff_lines(&a, &b);
        assert!(matches!(result, Err(DiffError::TooComplex)));
    }

    #[test]
    fn large_identical_with_small_change() {
        let mut a: Vec<u64> = (0..10_000).collect();
        let mut b = a.clone();
        b[5000] = 999_999;
        let result = diff_lines(&a, &b).unwrap();
        assert_eq!(result.added, 1);
        assert_eq!(result.removed, 1);
        a[5000] = 999_999;
        assert_eq!(rebuild_right(&result.hunks, &a, &b), b);
    }
}
