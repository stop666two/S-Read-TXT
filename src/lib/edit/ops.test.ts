import { describe, expect, it } from 'vitest';

import { deleteOp, deleteRangeOp, insertOp, replaceOp } from './ops';

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

  it('按逻辑区间构造删除操作（退格/前删规划结果）', () => {
    expect(deleteRangeOp({ startRow: 1, startUtf16: 0, endRow: 1, endUtf16: 2 })).toEqual({
      kind: 'delete',
      startRow: 1,
      startUtf16: 0,
      endRow: 1,
      endUtf16: 2,
    });
  });
});
