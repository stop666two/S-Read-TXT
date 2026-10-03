import { describe, expect, it } from 'vitest';

import {
  clampRectRows,
  MULTI_FALLBACK_MAX,
  normalizeRect,
  rectBackspaceSpans,
  rectDeleteSpans,
  rectIsCollapsed,
  rectSpans,
  samePos,
  toggleCaret,
  uniquePositions,
} from './multi';

describe('toggleCaret', () => {
  it('追加与移除（重复位置移除）', () => {
    const list = toggleCaret([], { row: 1, utf16: 2 }, 10);
    expect(list).toEqual([{ row: 1, utf16: 2 }]);
    const removed = toggleCaret(list, { row: 1, utf16: 2 }, 10);
    expect(removed).toEqual([]);
  });

  it('达到上限后忽略新增（总数含主光标）', () => {
    let list = toggleCaret([], { row: 1, utf16: 0 }, 2);
    expect(list).toHaveLength(1);
    list = toggleCaret(list, { row: 2, utf16: 0 }, 2);
    expect(list).toHaveLength(1);
    expect(MULTI_FALLBACK_MAX).toBe(1000);
  });
});

describe('rect 规格化与钳制', () => {
  it('任意拖动方向都规格化为 min/max', () => {
    const rect = normalizeRect({ row: 5, utf16: 9 }, { row: 2, utf16: 3 });
    expect(rect).toEqual({ fromRow: 2, toRow: 5, fromCol: 3, toCol: 9 });
  });

  it('单点矩形判定与行数钳制', () => {
    const rect = normalizeRect({ row: 1, utf16: 1 }, { row: 1, utf16: 1 });
    expect(rectIsCollapsed(rect)).toBe(true);
    const limited = clampRectRows({ fromRow: 10, toRow: 1000, fromCol: 0, toCol: 1 }, 5);
    expect(limited.toRow).toBe(14);
  });
});

describe('positions 去重', () => {
  it('主光标与附加光标重复时只保留一个', () => {
    const out = uniquePositions(
      { row: 0, utf16: 0 },
      [
        { row: 0, utf16: 0 },
        { row: 1, utf16: 2 },
        { row: 1, utf16: 2 },
      ],
    );
    expect(out).toEqual([
      { row: 0, utf16: 0 },
      { row: 1, utf16: 2 },
    ]);
    expect(samePos(out[0], { row: 0, utf16: 0 })).toBe(true);
  });
});

describe('矩形跨度', () => {
  const lengthAt = (row: number): number => [3, 5, 0, 2][row] ?? 0;

  it('按行钳制列（含零宽插入点）', () => {
    expect(rectSpans(lengthAt, { fromRow: 1, toRow: 3, fromCol: 1, toCol: 4 })).toEqual([
      { row: 1, from: 1, to: 4 },
      { row: 2, from: 0, to: 0 },
      { row: 3, from: 1, to: 2 },
    ]);
  });

  it('退格：列 0 行无操作', () => {
    expect(rectBackspaceSpans(lengthAt, { fromRow: 1, toRow: 3, fromCol: 0, toCol: 0 })).toEqual(
      [],
    );
    expect(rectBackspaceSpans(lengthAt, { fromRow: 1, toRow: 3, fromCol: 2, toCol: 2 })).toEqual([
      { row: 1, from: 1, to: 2 },
      { row: 3, from: 1, to: 2 },
    ]);
  });

  it('前向删除：行尾无操作', () => {
    expect(rectDeleteSpans(lengthAt, { fromRow: 0, toRow: 3, fromCol: 2, toCol: 2 })).toEqual([
      { row: 0, from: 2, to: 3 },
      { row: 1, from: 2, to: 3 },
    ]);
  });
});
