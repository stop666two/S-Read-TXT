// 三方合并输出模型（纯函数）：
// 依据每个区域（含冲突的人工选择）展开为输出行序列，供虚拟列表渲染。

import type { MergeRegionDto } from '../ipc';

/** 冲突区域的人工选择。 */
export type MergeChoice = 'ours' | 'theirs' | 'both' | 'none';

/** 输出行（来源 + 该文件内行号）。 */
export interface OutputRow {
  source: 'base' | 'ours' | 'theirs';
  row: number;
  /** 所属冲突区域下标（0 起；非冲突为 null）。 */
  conflict: number | null;
}

/** 输出模型。 */
export interface MergeOutputModel {
  rows: OutputRow[];
  /** 每个冲突区域在输出中的起始行下标。 */
  conflictStarts: number[];
  /** 输出总行数。 */
  totalRows: number;
}

function spanRows(source: OutputRow['source'], start: number, end: number): OutputRow[] {
  const rows: OutputRow[] = [];
  for (let row = start; row < end; row += 1) rows.push({ source, row, conflict: null });
  return rows;
}

/** 展开输出行；`choices` 与冲突出现顺序一一对应（缺省视为我方）。 */
export function buildOutputModel(
  regions: MergeRegionDto[],
  choices: MergeChoice[],
): MergeOutputModel {
  const rows: OutputRow[] = [];
  const conflictStarts: number[] = [];
  let conflictIndex = 0;
  for (const region of regions) {
    if (region.kind === 'conflict') {
      const choice = choices[conflictIndex] ?? 'ours';
      conflictStarts.push(rows.length);
      const picked: OutputRow[] = [];
      if (choice === 'ours' || choice === 'both') {
        picked.push(...spanRows('ours', region.oursRange[0], region.oursRange[1]));
      }
      if (choice === 'theirs' || choice === 'both') {
        picked.push(...spanRows('theirs', region.theirsRange[0], region.theirsRange[1]));
      }
      picked.forEach((row) => {
        rows.push({ ...row, conflict: conflictIndex });
      });
      conflictIndex += 1;
      continue;
    }
    if (region.merged) {
      rows.push(
        ...spanRows(region.merged.source, region.merged.start, region.merged.end),
      );
    }
  }
  return { rows, conflictStarts, totalRows: rows.length };
}

/** 冲突总数（用于展示与写回提示）。 */
export function countConflicts(regions: MergeRegionDto[]): number {
  return regions.filter((region) => region.kind === 'conflict').length;
}
