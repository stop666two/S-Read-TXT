// longline.ts 单测：坐标换算 + 段边界/逻辑行边界退格与前删规划。
import { describe, expect, it } from 'vitest';

import {
  planBackspace,
  planDeleteForward,
  toLogical,
  unitAfter,
  unitBefore,
  type SegMeta,
} from './longline';

function seg(logicalRow: number, baseUtf16: number, text: string): SegMeta {
  return { logicalRow, baseUtf16, text };
}

describe('toLogical', () => {
  it('普通行（基 0）恒等', () => {
    expect(toLogical(seg(3, 0, 'abc'), 2)).toEqual({ row: 3, utf16: 2 });
  });

  it('分段行叠加段基偏移', () => {
    expect(toLogical(seg(3, 8192, 'xyz'), 1)).toEqual({ row: 3, utf16: 8193 });
  });
});

describe('unitBefore / unitAfter（代理对整体处理）', () => {
  it('代理对计 2 个 UTF-16 单元', () => {
    const text = 'a😀b';
    expect(unitBefore(text, 3)).toBe(2);
    expect(unitAfter(text, 1)).toBe(2);
  });

  it('普通字符计 1 个单元', () => {
    expect(unitBefore('ab', 1)).toBe(1);
    expect(unitAfter('ab', 1)).toBe(1);
  });
});

describe('planBackspace', () => {
  it('段内：删除前一个字符', () => {
    expect(planBackspace(seg(0, 0, 'abc'), 2, null)).toEqual({
      startRow: 0,
      startUtf16: 1,
      endRow: 0,
      endUtf16: 2,
    });
  });

  it('段内：代理对整体删除', () => {
    expect(planBackspace(seg(0, 0, '😀x'), 2, null)).toEqual({
      startRow: 0,
      startUtf16: 0,
      endRow: 0,
      endUtf16: 2,
    });
  });

  it('段首同一逻辑行：跨段删除上一段末字符', () => {
    expect(planBackspace(seg(0, 2, 'zw'), 0, seg(0, 0, 'xy'))).toEqual({
      startRow: 0,
      startUtf16: 1,
      endRow: 0,
      endUtf16: 2,
    });
  });

  it('逻辑行首：删除行间换行', () => {
    expect(planBackspace(seg(1, 0, 'zw'), 0, seg(0, 0, 'xy'))).toEqual({
      startRow: 0,
      startUtf16: 2,
      endRow: 1,
      endUtf16: 0,
    });
  });

  it('文档首：无操作', () => {
    expect(planBackspace(seg(0, 0, 'abc'), 0, null)).toBeNull();
  });
});

describe('planDeleteForward', () => {
  it('段内：删除后一个字符', () => {
    expect(planDeleteForward(seg(0, 0, 'abc'), 1, null)).toEqual({
      startRow: 0,
      startUtf16: 1,
      endRow: 0,
      endUtf16: 2,
    });
  });

  it('段尾同一逻辑行：跨段删除下一段首字符', () => {
    expect(planDeleteForward(seg(0, 0, 'xy'), 2, seg(0, 2, 'zw'))).toEqual({
      startRow: 0,
      startUtf16: 2,
      endRow: 0,
      endUtf16: 3,
    });
  });

  it('逻辑行尾：删除行间换行', () => {
    expect(planDeleteForward(seg(0, 0, 'xy'), 2, seg(1, 0, 'zw'))).toEqual({
      startRow: 0,
      startUtf16: 2,
      endRow: 1,
      endUtf16: 0,
    });
  });

  it('文档末：无操作', () => {
    expect(planDeleteForward(seg(0, 0, 'abc'), 3, null)).toBeNull();
  });
});
