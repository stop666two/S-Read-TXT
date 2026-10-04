// 折叠视图映射（P2-6b V-08）：纯函数，供 ReaderView 与单测共用。
//
// 坐标约定：全部为「显示行」（0 基，与渲染/跳转/标注一致）。
// 折叠区间为闭区间 [startRow, endRow]；折叠后隐藏 (startRow, endRow]。

/** 折叠区间（与后端 `FoldRegion` 对应）。 */
export interface FoldRegion {
  startRow: number;
  endRow: number;
}

/** 半开隐藏区间 [from, to)。 */
export interface HiddenInterval {
  from: number;
  to: number;
}

/**
 * 折叠集合 → 合并后的隐藏区间。
 *
 * 规则：仅 `folded` 集合内的区间参与；隐藏范围不含头部行本身
 * （即 [startRow+1, endRow+1)）；相邻/重叠区间合并。
 */
export function hiddenIntervals(
  regions: readonly FoldRegion[],
  folded: ReadonlySet<number>,
): HiddenInterval[] {
  const intervals: HiddenInterval[] = [];
  for (const region of regions) {
    if (!folded.has(region.startRow)) continue;
    if (region.endRow <= region.startRow) continue;
    intervals.push({ from: region.startRow + 1, to: region.endRow + 1 });
  }
  intervals.sort((a, b) => a.from - b.from || a.to - b.to);
  const merged: HiddenInterval[] = [];
  for (const interval of intervals) {
    const last = merged[merged.length - 1];
    if (last && interval.from <= last.to) {
      last.to = Math.max(last.to, interval.to);
    } else {
      merged.push({ ...interval });
    }
  }
  return merged;
}

/** 行是否落在任一隐藏区间内（二分）。 */
export function isHidden(intervals: readonly HiddenInterval[], row: number): boolean {
  let lo = 0;
  let hi = intervals.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const interval = intervals[mid]!;
    if (row < interval.from) hi = mid - 1;
    else if (row >= interval.to) lo = mid + 1;
    else return true;
  }
  return false;
}

/** 可见行集：`total` 行中剔除隐藏区间后的文件行号数组。 */
export function buildVisibleRows(total: number, intervals: readonly HiddenInterval[]): number[] {
  if (intervals.length === 0) {
    return Array.from({ length: Math.max(0, total) }, (_, index) => index);
  }
  const rows: number[] = [];
  let cursor = 0;
  for (const interval of intervals) {
    const from = Math.max(cursor, Math.min(interval.from, total));
    for (let row = cursor; row < from; row += 1) rows.push(row);
    cursor = Math.max(cursor, Math.min(interval.to, total));
  }
  for (let row = cursor; row < total; row += 1) rows.push(row);
  return rows;
}

/**
 * 面包屑（P2-6c V-10）：给定大纲条目与当前顶部行，返回层级路径。
 *
 * 语义：按行序扫描，维护层级栈——遇到条目先按 `level` 弹栈再压入；
 * 返回“最后一行不超过 `row` 的条目”所在的完整祖先链（含自身）。
 * 无任何条目命中时返回空数组。
 */
export function breadcrumbChain<T extends { row: number; level: number }>(
  items: readonly T[],
  row: number,
): T[] {
  const chain: T[] = [];
  for (const item of items) {
    if (item.row > row) break;
    while (chain.length > 0 && chain[chain.length - 1].level >= item.level) {
      chain.pop();
    }
    chain.push(item);
  }
  return chain;
}
