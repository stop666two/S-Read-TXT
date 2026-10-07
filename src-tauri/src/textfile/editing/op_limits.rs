//! 单次操作数量上限（批量编号 / 行操作 / 全部替换共用）。
//!
//! 默认值与历史常量一致（200000）。启动加载设置与保存设置时经
//! [`set_max_items`] 同步 `tools.singleOpMaxRows`，使上限对用户可调；
//! 越限错误信息中的 `limit` 字段取运行时值，提示与实际一致。

use std::sync::atomic::{AtomicU64, Ordering};

/// 默认单次操作上限（行数或处数）。
pub const DEFAULT_MAX_ITEMS: u64 = 200_000;

static MAX_ITEMS: AtomicU64 = AtomicU64::new(DEFAULT_MAX_ITEMS);

/// 当前单次操作上限；最小 1（设置层已限定区间，这里仅防御 0）。
pub fn max_items() -> u64 {
    MAX_ITEMS.load(Ordering::Relaxed).max(1)
}

/// 更新单次操作上限；传入 0 表示回退默认值（防御异常配置）。
pub fn set_max_items(value: u64) {
    let next = if value == 0 { DEFAULT_MAX_ITEMS } else { value };
    MAX_ITEMS.store(next, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_override() {
        set_max_items(0);
        assert_eq!(max_items(), DEFAULT_MAX_ITEMS);
        set_max_items(50);
        assert_eq!(max_items(), 50);
        set_max_items(0);
        assert_eq!(max_items(), DEFAULT_MAX_ITEMS);
    }
}
