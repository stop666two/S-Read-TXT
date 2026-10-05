// 比较视图的行对齐模型（纯函数）：
// 将后端差异块展开为「并排对齐行」与「统一视图行」，供虚拟列表渲染。

/** 差异块类型（与 Rust 侧序列化一致）。 */
export type HunkKind = 'equal' | 'change' | 'delete' | 'insert';

/** 差异块。 */
export interface DiffHunkDto {
  kind: HunkKind;
  leftStart: number;
  leftLen: number;
  rightStart: number;
  rightLen: number;
}

/** 并排对齐行：两侧行号（null = 该侧无对应行）。 */
export interface AlignRow {
  left: number | null;
  right: number | null;
  kind: HunkKind;
}

/** 并排视图模型。 */
export interface SideModel {
  rows: AlignRow[];
  /** 每个差异块起始的行下标（用于「上一处/下一处」导航）。 */
  hunkStarts: number[];
}

/** 统一视图行：删除行（左）/ 新增行（右）/ 相同行。 */
export interface UnifiedRow {
  kind: HunkKind;
  side: 'left' | 'right';
  row: number;
}

/** 统一视图模型。 */
export interface UnifiedModel {
  rows: UnifiedRow[];
  hunkStarts: number[];
}

/** 展开为并排对齐行。 */
export function buildSideModel(hunks: DiffHunkDto[]): SideModel {
  const rows: AlignRow[] = [];
  const hunkStarts: number[] = [];
  for (const hunk of hunks) {
    hunkStarts.push(rows.length);
    if (hunk.kind === 'equal') {
      for (let i = 0; i < hunk.leftLen; i += 1) {
        rows.push({ left: hunk.leftStart + i, right: hunk.rightStart + i, kind: 'equal' });
      }
      continue;
    }
    const span = Math.max(hunk.leftLen, hunk.rightLen);
    for (let i = 0; i < span; i += 1) {
      rows.push({
        left: i < hunk.leftLen ? hunk.leftStart + i : null,
        right: i < hunk.rightLen ? hunk.rightStart + i : null,
        kind: hunk.kind,
      });
    }
  }
  return { rows, hunkStarts };
}

/** 展开为统一视图行。 */
export function buildUnifiedModel(hunks: DiffHunkDto[]): UnifiedModel {
  const rows: UnifiedRow[] = [];
  const hunkStarts: number[] = [];
  for (const hunk of hunks) {
    hunkStarts.push(rows.length);
    if (hunk.kind === 'equal') {
      for (let i = 0; i < hunk.leftLen; i += 1) {
        rows.push({ kind: 'equal', side: 'left', row: hunk.leftStart + i });
      }
      continue;
    }
    for (let i = 0; i < hunk.leftLen; i += 1) {
      rows.push({ kind: 'delete', side: 'left', row: hunk.leftStart + i });
    }
    for (let i = 0; i < hunk.rightLen; i += 1) {
      rows.push({ kind: 'insert', side: 'right', row: hunk.rightStart + i });
    }
  }
  return { rows, hunkStarts };
}

/** 由当前滚动位置找「下一处差异」的显示行（`from` 之后；无则回绕到首处）。 */
export function nextHunkRow(model: { hunkStarts: number[] }, from: number, forward: boolean): number | null {
  if (model.hunkStarts.length === 0) return null;
  const starts = model.hunkStarts;
  if (forward) {
    const hit = starts.find((value) => value > from);
    return hit ?? starts[0];
  }
  for (let i = starts.length - 1; i >= 0; i -= 1) {
    if (starts[i] < from) return starts[i];
  }
  return starts[starts.length - 1];
}

/** 行窗口合并：把需要显示的行区间并集压平为连续区间（减少 IPC 次数）。 */
export function mergeRanges(ranges: Array<[number, number]>): Array<[number, number]> {
  if (ranges.length === 0) return [];
  const sorted = [...ranges].sort((a, b) => a[0] - b[0]);
  const out: Array<[number, number]> = [[sorted[0][0], sorted[0][1]]];
  for (let i = 1; i < sorted.length; i += 1) {
    const last = out[out.length - 1];
    const [start, end] = sorted[i];
    if (start <= last[1]) {
      last[1] = Math.max(last[1], end);
    } else {
      out.push([start, end]);
    }
  }
  return out;
}
