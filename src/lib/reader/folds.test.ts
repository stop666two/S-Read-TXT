import { describe, expect, it } from 'vitest';

import { buildVisibleRows, hiddenIntervals, isHidden, type FoldRegion } from './folds';

const regions: FoldRegion[] = [
  { startRow: 2, endRow: 5 },
  { startRow: 4, endRow: 8 },
  { startRow: 10, endRow: 12 },
];

describe('hiddenIntervals', () => {
  it('空集合无隐藏', () => {
    expect(hiddenIntervals(regions, new Set())).toEqual([]);
  });

  it('单个区间隐藏头行之后的部分', () => {
    expect(hiddenIntervals(regions, new Set([2]))).toEqual([{ from: 3, to: 6 }]);
  });

  it('嵌套区间合并', () => {
    expect(hiddenIntervals(regions, new Set([2, 4]))).toEqual([{ from: 3, to: 9 }]);
  });

  it('相邻区间合并', () => {
    const adjacent: FoldRegion[] = [
      { startRow: 0, endRow: 2 },
      { startRow: 2, endRow: 4 },
    ];
    // 0 折叠隐藏 [1,3)；2 行本身是第二个区间头部（不隐藏），折叠 2 隐藏 [3,5)
    expect(hiddenIntervals(adjacent, new Set([0, 2]))).toEqual([{ from: 1, to: 5 }]);
  });

  it('天然无效区间（end<=start）被忽略', () => {
    expect(hiddenIntervals([{ startRow: 7, endRow: 7 }], new Set([7]))).toEqual([]);
  });
});

describe('isHidden', () => {
  it('边界判定（含/不含）', () => {
    const hidden = hiddenIntervals(regions, new Set([2]));
    expect(isHidden(hidden, 2)).toBe(false); // 头部行可见
    expect(isHidden(hidden, 3)).toBe(true);
    expect(isHidden(hidden, 5)).toBe(true);
    expect(isHidden(hidden, 6)).toBe(false);
  });
});

describe('buildVisibleRows', () => {
  it('无隐藏时恒等', () => {
    expect(buildVisibleRows(4, [])).toEqual([0, 1, 2, 3]);
  });

  it('剔除隐藏并跳过区间', () => {
    const visible = buildVisibleRows(14, hiddenIntervals(regions, new Set([2, 10])));
    expect(visible).toEqual([0, 1, 2, 6, 7, 8, 9, 10, 13]);
  });

  it('区间越界收缩到总行数', () => {
    expect(buildVisibleRows(3, [{ from: 1, to: 99 }])).toEqual([0]);
  });
});
