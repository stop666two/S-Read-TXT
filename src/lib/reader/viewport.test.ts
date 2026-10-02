// viewport.ts 单元测试（Vitest）。

import { describe, expect, it } from 'vitest';

import { HeightModel } from './heights';
import { computePercent, computeWindow, planBatches } from './viewport';

describe('computeWindow', () => {
  it('均匀估计行高下的窗口计算', () => {
    const model = new HeightModel(10); // 估计行高 10px
    const range = computeWindow(model, 1000, 1000, 200, 5);
    expect(range.anchorRow).toBe(100); // 1000 / 10
    expect(range.start).toBe(95); // 含上缓冲
    expect(range.end).toBe(121); // 95 + 20(可视) + 5(下缓冲) + 1
  });

  it('顶部与空内容边界', () => {
    const model = new HeightModel(10);
    expect(computeWindow(model, 1000, 0, 200, 5).start).toBe(0);
    const empty = computeWindow(model, 0, 0, 200, 5);
    expect(empty).toEqual({ start: 0, end: 0, anchorRow: 0 });
  });

  it('窗口不越过总行数', () => {
    const model = new HeightModel(10);
    const range = computeWindow(model, 50, 5000, 200, 5);
    expect(range.end).toBe(50);
    expect(range.start).toBeLessThanOrEqual(50);
  });
});

describe('computePercent', () => {
  it('首行 0%、末行接近 100%、空文件 0%', () => {
    const model = new HeightModel(10);
    expect(computePercent(model, 100, 0)).toBe(0);
    const last = computePercent(model, 100, 99);
    expect(last).toBeGreaterThan(98);
    expect(last).toBeLessThanOrEqual(100);
    expect(computePercent(model, 0, 0)).toBe(0);
  });

  it('中位行约 50%', () => {
    const model = new HeightModel(10);
    expect(computePercent(model, 101, 50)).toBeCloseTo(49.5, 0);
  });
});

describe('planBatches', () => {
  it('跳过已缓存行并按连续性合并', () => {
    const cached = new Set([2, 11]);
    const batches = planBatches([1, 2, 3, 10, 11, 20], (r) => cached.has(r), 16);
    expect(batches).toEqual([
      { start: 1, count: 1 },
      { start: 3, count: 1 },
      { start: 10, count: 1 },
      { start: 20, count: 1 },
    ]);
  });

  it('按 maxBatch 切分长区间', () => {
    const batches = planBatches(
      Array.from({ length: 10 }, (_, i) => i),
      () => false,
      4,
    );
    expect(batches).toEqual([
      { start: 0, count: 4 },
      { start: 4, count: 4 },
      { start: 8, count: 2 },
    ]);
  });

  it('去重与乱序输入归一', () => {
    const batches = planBatches([3, 1, 3, 2], () => false, 16);
    expect(batches).toEqual([{ start: 1, count: 3 }]);
  });

  it('全部已缓存时无批次', () => {
    expect(planBatches([1, 2, 3], () => true, 16)).toEqual([]);
  });
});
