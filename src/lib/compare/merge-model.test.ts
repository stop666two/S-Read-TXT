import { describe, expect, it } from 'vitest';

import type { MergeRegionDto } from '../ipc';
import { buildOutputModel, countConflicts, type MergeChoice } from './merge-model';

const stable = (start: number, end: number): MergeRegionDto => ({
  kind: 'stable',
  baseRange: [start, end],
  oursRange: [start, end],
  theirsRange: [start, end],
  merged: { source: 'base', start, end },
});

const oursOnly = (start: number, end: number): MergeRegionDto => ({
  kind: 'oursOnly',
  baseRange: [start, end - (end - start - 1)],
  oursRange: [start, end],
  theirsRange: [start, start + 1],
  merged: { source: 'ours', start, end },
});

const conflict = (b0: number, b1: number): MergeRegionDto => ({
  kind: 'conflict',
  baseRange: [b0, b1],
  oursRange: [b0, b1],
  theirsRange: [b0, b1],
  merged: null,
});

const regions: MergeRegionDto[] = [
  stable(0, 2),
  conflict(2, 3),
  oursOnly(3, 5),
  conflict(5, 6),
];

describe('buildOutputModel', () => {
  it('默认冲突采用我方（choices 缺省）', () => {
    const model = buildOutputModel(regions, []);
    expect(model.totalRows).toBe(2 + 1 + 2 + 1);
    expect(model.conflictStarts).toEqual([2, 5]);
    expect(model.rows[2]).toEqual({ source: 'ours', row: 2, conflict: 0 });
    expect(model.rows[5]).toEqual({ source: 'ours', row: 5, conflict: 1 });
    expect(model.rows[3]).toEqual({ source: 'ours', row: 3, conflict: null });
  });

  it('他方 / 双方 / 不用 三种选择', () => {
    const choices: MergeChoice[] = ['theirs', 'both'];
    const model = buildOutputModel(regions, choices);
    expect(model.rows[2]).toEqual({ source: 'theirs', row: 2, conflict: 0 });
    expect(model.conflictStarts).toEqual([2, 5]);
    expect(model.rows[5]).toEqual({ source: 'ours', row: 5, conflict: 1 });
    expect(model.rows[6]).toEqual({ source: 'theirs', row: 5, conflict: 1 });
    const noneModel = buildOutputModel(regions, ['none', 'none']);
    expect(noneModel.totalRows).toBe(2 + 0 + 2 + 0);
    expect(noneModel.conflictStarts).toEqual([2, 4]);
  });

  it('非冲突区域按 merged 来源展开', () => {
    const model = buildOutputModel([oursOnly(0, 3)], []);
    expect(model.rows).toEqual([
      { source: 'ours', row: 0, conflict: null },
      { source: 'ours', row: 1, conflict: null },
      { source: 'ours', row: 2, conflict: null },
    ]);
  });
});

describe('countConflicts', () => {
  it('统计冲突区域数', () => {
    expect(countConflicts(regions)).toBe(2);
    expect(countConflicts([stable(0, 1)])).toBe(0);
  });
});
