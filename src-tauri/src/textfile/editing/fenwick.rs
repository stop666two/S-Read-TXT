//! Fenwick 树（树状数组）：片段前缀和查询。
//!
//! 用途（编辑引擎）：
//! - 字节维度：按文档字节偏移定位片段；
//! - 行维度：按换行单元序号定位片段。
//!
//! 复杂度：构建 O(n)、前缀和 O(log n)、下界查询 O(log n)。
//! 片段列表在每次编辑批次后整体重建（片段数经合并后保持很小），
//! 因此本结构不提供单点更新接口，只提供 `rebuild`。

/// `u64` 值域的树状数组（1-based 内部存储）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fenwick {
    /// `tree[0]` 未使用；`tree[i]` 覆盖窗口大小 = lowbit(i)
    tree: Vec<u64>,
    /// 逻辑长度（值个数）
    len: usize,
}

impl Fenwick {
    /// 由值序列构建（O(n)）。
    pub fn build_from(values: &[u64]) -> Self {
        let len = values.len();
        let mut tree = Vec::with_capacity(len + 1);
        tree.push(0);
        tree.extend_from_slice(values);
        for index in 1..=len {
            let parent = index + (index & index.wrapping_neg());
            if parent <= len {
                let value = tree[index];
                tree[parent] += value;
            }
        }
        Self { tree, len }
    }

    /// 值个数。
    pub fn len(&self) -> usize {
        self.len
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// `[0, index)` 的前缀和（`index` 越界收敛到 `len`）。
    pub fn prefix(&self, index: usize) -> u64 {
        let mut index = index.min(self.len);
        let mut sum = 0;
        while index > 0 {
            sum += self.tree[index];
            index -= index & index.wrapping_neg();
        }
        sum
    }

    /// 全部值之和。
    pub fn total(&self) -> u64 {
        self.prefix(self.len)
    }

    /// 整体重建（片段列表变化后调用）。
    pub fn rebuild(&mut self, values: &[u64]) {
        *self = Self::build_from(values);
    }

    /// 下界查询：返回最大的 `index`（0..=len），满足 `prefix(index) <= target` 且
    /// `prefix(index + 1) > target`；即 `target` 落在第 `index` 个值内。
    ///
    /// 语义细节：
    /// - `target` 恰好等于某段起始时返回该段下标（段内第 0 字节）；
    /// - `target >= total()` 时返回 `len()`（表示"末尾之后"）；
    /// - 空树返回 0。
    ///
    /// 前提：值序列不含 0（编辑引擎不产生零长度片段），否则可能返回零长度段。
    pub fn lower_bound(&self, target: u64) -> usize {
        let mut rest = target;
        let mut index = 0usize;
        let mut bit = 1usize;
        while bit << 1 <= self.len {
            bit <<= 1;
        }
        while bit > 0 {
            let next = index + bit;
            if next <= self.len && self.tree[next] <= rest {
                rest -= self.tree[next];
                index = next;
            }
            bit >>= 1;
        }
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Fenwick {
        Fenwick::build_from(&[3, 2, 4])
    }

    /// 前缀和（含边界收敛）。
    #[test]
    fn prefix_sums() {
        let tree = sample();
        assert_eq!(tree.prefix(0), 0);
        assert_eq!(tree.prefix(1), 3);
        assert_eq!(tree.prefix(2), 5);
        assert_eq!(tree.prefix(3), 9);
        assert_eq!(tree.prefix(99), 9);
        assert_eq!(tree.total(), 9);
    }

    /// 下界查询：段内任意偏移、段起始、末尾之后。
    #[test]
    fn lower_bound_positions() {
        let tree = sample();
        assert_eq!(tree.lower_bound(0), 0);
        assert_eq!(tree.lower_bound(2), 0);
        assert_eq!(tree.lower_bound(3), 1);
        assert_eq!(tree.lower_bound(4), 1);
        assert_eq!(tree.lower_bound(5), 2);
        assert_eq!(tree.lower_bound(8), 2);
        assert_eq!(tree.lower_bound(9), 3);
        assert_eq!(tree.lower_bound(u64::MAX), 3);
    }

    /// 空树安全。
    #[test]
    fn empty_tree() {
        let tree = Fenwick::build_from(&[]);
        assert!(tree.is_empty());
        assert_eq!(tree.total(), 0);
        assert_eq!(tree.lower_bound(0), 0);
    }

    /// 重建后查询随之更新。
    #[test]
    fn rebuild_updates_queries() {
        let mut tree = sample();
        tree.rebuild(&[1, 1]);
        assert_eq!(tree.total(), 2);
        assert_eq!(tree.lower_bound(0), 0);
        assert_eq!(tree.lower_bound(1), 1);
        assert_eq!(tree.lower_bound(2), 2);
    }
}
