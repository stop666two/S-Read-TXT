// heights.ts 单元测试（Vitest）。

import { describe, expect, it } from 'vitest';

import { CHUNK_SIZE, HeightModel } from './heights';

describe('HeightModel', () => {
  it('空模型按基准估计行高累计', () => {
    const model = new HeightModel(20);
    expect(model.offsetOf(0, 10)).toBe(0);
    expect(model.offsetOf(5, 10)).toBe(100);
    expect(model.totalHeight(10)).toBe(200);
  });

  it('实测更新估计平均值', () => {
    const model = new HeightModel(20);
    model.measure(0, 60);
    expect(model.estimated).toBe(60);
    model.measure(1, 30);
    expect(model.estimated).toBe(45);
    expect(model.offsetOf(2, 10)).toBe(90);
  });

  it('重复测量覆盖且均值正确', () => {
    const model = new HeightModel(20);
    model.measure(0, 10);
    model.measure(0, 30);
    expect(model.measuredRows).toBe(1);
    expect(model.estimated).toBe(30);
  });

  it('非正高度被忽略', () => {
    const model = new HeightModel(20);
    model.measure(0, 0);
    model.measure(1, -5);
    expect(model.measuredRows).toBe(0);
    expect(model.estimated).toBe(20);
  });

  it('跨块偏移与反向定位互为逆运算（全实测均匀行高）', () => {
    const rows = CHUNK_SIZE * 3 + 17;
    const model = new HeightModel(10);
    for (let r = 0; r < rows; r += 1) model.measure(r, 25);
    for (const row of [0, 1, CHUNK_SIZE - 1, CHUNK_SIZE, CHUNK_SIZE + 1, rows - 1]) {
      const offset = model.offsetOf(row, rows);
      expect(model.rowAtOffset(offset, rows)).toBe(row);
    }
    expect(model.totalHeight(rows)).toBe(rows * 25);
  });

  it('反向定位边界：负偏移取 0；越界取末行', () => {
    const model = new HeightModel(10);
    expect(model.rowAtOffset(-100, 100)).toBe(0);
    expect(model.rowAtOffset(model.totalHeight(100) + 999, 100)).toBe(99);
    expect(model.rowAtOffset(50, 0)).toBe(0);
  });

  it('clear 恢复基准估计', () => {
    const model = new HeightModel(20);
    model.measure(0, 100);
    model.clear();
    expect(model.estimated).toBe(20);
    expect(model.measuredRows).toBe(0);
  });

  it('invalidateFrom：仅失效目标行及之后并重算估计', () => {
    const model = new HeightModel(20);
    model.measure(0, 100);
    model.measure(1, 40);
    model.measure(3, 60);
    model.invalidateFrom(1);
    expect(model.measuredRows).toBe(1);
    expect(model.heightOf(0)).toBe(100);
    expect(model.heightOf(1)).toBe(100); // 估计值 = 剩余实测均值
    expect(model.heightOf(3)).toBe(100);
  });

  it('invalidateFrom：全部失效后恢复基准', () => {
    const model = new HeightModel(25);
    model.measure(0, 50);
    model.measure(1, 70);
    model.invalidateFrom(0);
    expect(model.measuredRows).toBe(0);
    expect(model.estimated).toBe(25);
  });
});
