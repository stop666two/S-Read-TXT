// row-cache.ts 单元测试（Vitest）。

import { describe, expect, it } from 'vitest';

import { RowCache, type CachedRow } from './row-cache';

/** 构造缓存值（普通行：logicalRow = row、baseUtf16 = 0；分段行可显式指定）。 */
function row(row: number, text: string, logicalRow = row, baseUtf16 = 0): CachedRow {
  return { text, logicalRow, baseUtf16 };
}

describe('RowCache', () => {
  it('写入/读取/命中判定（值携带分段元数据）', () => {
    const cache = new RowCache(100);
    cache.set(3, row(3, 'hello'));
    expect(cache.get(3)).toEqual({ text: 'hello', logicalRow: 3, baseUtf16: 0 });
    expect(cache.has(3)).toBe(true);
    expect(cache.get(4)).toBeUndefined();
    expect(cache.size).toBe(1);
  });

  it('分段行元数据原样保留', () => {
    const cache = new RowCache(100);
    cache.set(7, row(7, 'seg', 3, 8192));
    expect(cache.get(7)).toEqual({ text: 'seg', logicalRow: 3, baseUtf16: 8192 });
  });

  it('命中提升最近使用序，超预算时驱逐最旧', () => {
    const cache = new RowCache(9);
    cache.set(1, row(1, 'aaa'));
    cache.set(2, row(2, 'bbb'));
    cache.get(1); // 1 变为最近使用
    cache.set(3, row(3, 'cccc')); // 超预算（12 > 9）→ 驱逐最旧（2）
    expect(cache.has(2)).toBe(false);
    expect(cache.has(1)).toBe(true);
    expect(cache.has(3)).toBe(true);
  });

  it('单行超预算仍保留该行', () => {
    const cache = new RowCache(4);
    cache.set(9, row(9, 'x'.repeat(50)));
    expect(cache.has(9)).toBe(true);
    expect(cache.get(9)?.text.length).toBe(50);
  });

  it('覆盖写修正字符计数', () => {
    const cache = new RowCache(100);
    cache.set(1, row(1, 'abc'));
    cache.set(1, row(1, 'abcdef'));
    expect(cache.get(1)).toEqual(row(1, 'abcdef'));
    expect(cache.size).toBe(1);
  });

  it('clear 清空', () => {
    const cache = new RowCache(100);
    cache.set(1, row(1, 'a'));
    cache.clear();
    expect(cache.size).toBe(0);
    expect(cache.get(1)).toBeUndefined();
  });

  it('invalidateFrom：仅丢弃目标行及之后，字符预算同步', () => {
    const cache = new RowCache(100);
    cache.set(0, row(0, 'a'.repeat(10)));
    cache.set(1, row(1, 'b'.repeat(10)));
    cache.set(2, row(2, 'c'.repeat(10)));
    cache.invalidateFrom(1);
    expect(cache.size).toBe(1);
    expect(cache.has(0)).toBe(true);
    // 预算已归还：再写入 90 字符不应触发对旧行的驱逐
    cache.set(5, row(5, 'd'.repeat(90)));
    expect(cache.size).toBe(2);
    expect(cache.has(0)).toBe(true);
  });
});
