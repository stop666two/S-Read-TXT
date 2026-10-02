// row-cache.ts 单元测试（Vitest）。

import { describe, expect, it } from 'vitest';

import { RowCache } from './row-cache';

describe('RowCache', () => {
  it('写入/读取/命中判定', () => {
    const cache = new RowCache(100);
    cache.set(3, 'hello');
    expect(cache.get(3)).toBe('hello');
    expect(cache.has(3)).toBe(true);
    expect(cache.get(4)).toBeUndefined();
    expect(cache.size).toBe(1);
  });

  it('命中提升最近使用序，超预算时驱逐最旧', () => {
    const cache = new RowCache(9);
    cache.set(1, 'aaa');
    cache.set(2, 'bbb');
    cache.get(1); // 1 变为最近使用
    cache.set(3, 'cccc'); // 超预算（12 > 9）→ 驱逐最旧（2）
    expect(cache.has(2)).toBe(false);
    expect(cache.has(1)).toBe(true);
    expect(cache.has(3)).toBe(true);
  });

  it('单行超预算仍保留该行', () => {
    const cache = new RowCache(4);
    cache.set(9, 'x'.repeat(50));
    expect(cache.has(9)).toBe(true);
    expect(cache.get(9)?.length).toBe(50);
  });

  it('覆盖写修正字符计数', () => {
    const cache = new RowCache(100);
    cache.set(1, 'abc');
    cache.set(1, 'abcdef');
    expect(cache.get(1)).toBe('abcdef');
    expect(cache.size).toBe(1);
  });

  it('clear 清空', () => {
    const cache = new RowCache(100);
    cache.set(1, 'a');
    cache.clear();
    expect(cache.size).toBe(0);
    expect(cache.get(1)).toBeUndefined();
  });
});
