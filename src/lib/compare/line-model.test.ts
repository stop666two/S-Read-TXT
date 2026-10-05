import { describe, expect, it } from 'vitest';

import {
  buildSideModel,
  buildUnifiedModel,
  mergeRanges,
  nextHunkRow,
  type DiffHunkDto,
} from './line-model';

const hunks: DiffHunkDto[] = [
  { kind: 'equal', leftStart: 0, leftLen: 2, rightStart: 0, rightLen: 2 },
  { kind: 'change', leftStart: 2, leftLen: 1, rightStart: 2, rightLen: 2 },
  { kind: 'equal', leftStart: 3, leftLen: 1, rightStart: 4, rightLen: 1 },
  { kind: 'delete', leftStart: 4, leftLen: 2, rightStart: 5, rightLen: 0 },
  { kind: 'equal', leftStart: 6, leftLen: 1, rightStart: 5, rightLen: 1 },
  { kind: 'insert', leftStart: 7, leftLen: 0, rightStart: 6, rightLen: 2 },
];

describe('buildSideModel', () => {
  it('对齐两侧行号并保持顺序', () => {
    const model = buildSideModel(hunks);
    expect(model.rows[0]).toEqual({ left: 0, right: 0, kind: 'equal' });
    expect(model.rows[2]).toEqual({ left: 2, right: 2, kind: 'change' });
    expect(model.rows[3]).toEqual({ left: null, right: 3, kind: 'change' });
    expect(model.rows[5]).toEqual({ left: 4, right: null, kind: 'delete' });
    expect(model.rows[8]).toEqual({ left: null, right: 6, kind: 'insert' });
    expect(model.rows).toHaveLength(10);
  });

  it('hunkStarts 指向每个块首行', () => {
    const model = buildSideModel(hunks);
    expect(model.hunkStarts).toEqual([0, 2, 4, 5, 7, 8]);
  });

  it('空差异返回空模型', () => {
    const model = buildSideModel([]);
    expect(model.rows).toEqual([]);
    expect(model.hunkStarts).toEqual([]);
  });
});

describe('buildUnifiedModel', () => {
  it('删除行在前、新增行在后', () => {
    const model = buildUnifiedModel(hunks);
    expect(model.rows[2]).toEqual({ kind: 'delete', side: 'left', row: 2 });
    expect(model.rows[3]).toEqual({ kind: 'insert', side: 'right', row: 2 });
    expect(model.rows[4]).toEqual({ kind: 'insert', side: 'right', row: 3 });
    expect(model.rows[5]).toEqual({ kind: 'equal', side: 'left', row: 3 });
  });

  it('纯删除与纯插入块', () => {
    const model = buildUnifiedModel(hunks);
    const deletes = model.rows.filter((row) => row.kind === 'delete');
    expect(deletes.map((row) => row.row)).toEqual([2, 4, 5]);
    const inserts = model.rows.filter((row) => row.kind === 'insert');
    expect(inserts.map((row) => row.row)).toEqual([2, 3, 6, 7]);
  });
});

describe('nextHunkRow', () => {
  it('向前与向后导航并回绕', () => {
    const model = { hunkStarts: [0, 5, 9] };
    expect(nextHunkRow(model, 0, true)).toBe(5);
    expect(nextHunkRow(model, 9, true)).toBe(0);
    expect(nextHunkRow(model, 5, false)).toBe(0);
    expect(nextHunkRow(model, 0, false)).toBe(9);
  });

  it('无差异返回 null', () => {
    expect(nextHunkRow({ hunkStarts: [] }, 0, true)).toBeNull();
  });
});

describe('mergeRanges', () => {
  it('合并重叠与相邻之外保持分离', () => {
    expect(mergeRanges([[5, 10], [1, 3], [8, 12]])).toEqual([[1, 3], [5, 12]]);
    expect(mergeRanges([[0, 5], [6, 8]])).toEqual([[0, 5], [6, 8]]);
    expect(mergeRanges([])).toEqual([]);
  });
});
