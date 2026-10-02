import { describe, expect, it } from 'vitest';

import { collapsed } from './caret';
import { backspaceOp, deleteForwardOp, deleteOp, insertOp, replaceOp, selectionText } from './ops';

describe('ops 编辑操作构造', () => {
  it('插入与替换/删除：坐标原样传递', () => {
    expect(insertOp({ row: 1, utf16: 2 }, 'x')).toEqual({
      kind: 'insert',
      row: 1,
      utf16: 2,
      text: 'x',
    });
    const sel = { anchor: { row: 2, utf16: 3 }, head: { row: 0, utf16: 1 } };
    expect(deleteOp(sel)).toEqual({
      kind: 'delete',
      startRow: 0,
      startUtf16: 1,
      endRow: 2,
      endUtf16: 3,
    });
    expect(replaceOp(sel, '文')).toEqual({
      kind: 'replace',
      startRow: 0,
      startUtf16: 1,
      endRow: 2,
      endUtf16: 3,
      text: '文',
    });
  });

  it('退格：行内 / 跨行合并 / 文档首', () => {
    expect(backspaceOp({ row: 1, utf16: 2 }, 'abcd', 'x')).toEqual({
      kind: 'delete',
      startRow: 1,
      startUtf16: 1,
      endRow: 1,
      endUtf16: 2,
    });
    expect(backspaceOp({ row: 1, utf16: 0 }, 'abcd', 'xyz')).toEqual({
      kind: 'delete',
      startRow: 0,
      startUtf16: 3,
      endRow: 1,
      endUtf16: 0,
    });
    expect(backspaceOp({ row: 0, utf16: 0 }, 'abcd', undefined)).toBeNull();
  });

  it('退格：代理对整对删除', () => {
    const text = 'a\u{1F600}'; // 长度 3
    expect(backspaceOp({ row: 0, utf16: 3 }, text, undefined)).toEqual({
      kind: 'delete',
      startRow: 0,
      startUtf16: 1,
      endRow: 0,
      endUtf16: 3,
    });
  });

  it('前向删除：行内 / 跨行合并 / 文档末 / 代理对', () => {
    expect(deleteForwardOp({ row: 0, utf16: 1 }, 'abcd', true)).toEqual({
      kind: 'delete',
      startRow: 0,
      startUtf16: 1,
      endRow: 0,
      endUtf16: 2,
    });
    expect(deleteForwardOp({ row: 0, utf16: 4 }, 'abcd', true)).toEqual({
      kind: 'delete',
      startRow: 0,
      startUtf16: 4,
      endRow: 1,
      endUtf16: 0,
    });
    expect(deleteForwardOp({ row: 0, utf16: 4 }, 'abcd', false)).toBeNull();
    expect(deleteForwardOp({ row: 0, utf16: 1 }, 'a\u{1F600}', false)).toEqual({
      kind: 'delete',
      startRow: 0,
      startUtf16: 1,
      endRow: 0,
      endUtf16: 3,
    });
  });

  it('选区文本：单行/跨行/缺失行降级', () => {
    const texts = ['abcde', 'fghij', 'klmno'];
    const rowText = (row: number): string | undefined => texts[row];
    expect(selectionText(collapsed({ row: 0, utf16: 1 }), 3, rowText)).toBe('');
    expect(
      selectionText({ anchor: { row: 0, utf16: 1 }, head: { row: 0, utf16: 4 } }, 3, rowText),
    ).toBe('bcd');
    expect(
      selectionText({ anchor: { row: 0, utf16: 3 }, head: { row: 2, utf16: 2 } }, 3, rowText),
    ).toBe('de\nfghij\nkl');
  });

  it('选区文本：行未加载时截断（不阻塞）', () => {
    const rowText = (row: number): string | undefined => (row === 0 ? 'abc' : undefined);
    expect(
      selectionText({ anchor: { row: 0, utf16: 0 }, head: { row: 5, utf16: 0 } }, 6, rowText),
    ).toBe('abc');
  });

  it('选区文本：行尾哨兵收敛到真实行尾（全选场景）', () => {
    const texts = ['ab', '中c'];
    const rowText = (row: number): string | undefined => texts[row];
    expect(
      selectionText(
        { anchor: { row: 0, utf16: 0 }, head: { row: 1, utf16: Number.MAX_SAFE_INTEGER } },
        2,
        rowText,
      ),
    ).toBe('ab\n中c');
  });
});
