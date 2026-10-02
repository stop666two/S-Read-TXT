// 视口计算（纯函数）：可视行窗口、阅读百分比、缺失行批次规划。
import type { HeightModel } from './heights';

/** 可视行窗口（[start, end) 半开区间；anchorRow 为顶部定位行） */
export interface WindowRange {
  start: number;
  end: number;
  anchorRow: number;
}

/**
 * 由「内容滚动位置 + 视口高度」计算渲染窗口。
 *
 * 参数：
 * - model：行高模型（提供估计行高与偏移换算）
 * - totalRows：总行数
 * - scrollTop：内容坐标系滚动位置（已扣除页面内边距）
 * - viewportHeight：视口像素高度
 * - overscan：窗口上下各额外渲染的行数（预取缓冲）
 *
 * 边界：totalRows ≤ 0 → 空窗口；scrollTop 为负按 0。
 */
export function computeWindow(
  model: HeightModel,
  totalRows: number,
  scrollTop: number,
  viewportHeight: number,
  overscan: number,
): WindowRange {
  if (totalRows <= 0) return { start: 0, end: 0, anchorRow: 0 };
  const safeTop = Math.max(0, scrollTop);
  const anchorRow = model.rowAtOffset(safeTop, totalRows);
  const start = Math.max(0, anchorRow - overscan);
  const viewRows = Math.ceil(Math.max(1, viewportHeight) / Math.max(1, model.estimated));
  const end = Math.min(totalRows, start + viewRows + overscan + 1);
  return { start, end, anchorRow };
}

/**
 * 阅读百分比（视觉进度）：顶部定位行的累计高度 / 内容总高度 × 100。
 * 边界：无行或总高为 0 → 0；结果钳制在 [0, 100]。
 */
export function computePercent(model: HeightModel, totalRows: number, row: number): number {
  if (totalRows <= 0) return 0;
  const total = model.totalHeight(totalRows);
  if (total <= 0) return 0;
  return Math.min(100, Math.max(0, (model.offsetOf(row, totalRows) / total) * 100));
}

/** 取行批次（连续行区间） */
export interface FetchBatch {
  start: number;
  count: number;
}

/**
 * 把缺失行列表规划为连续区间批次（供合并 IPC 请求）。
 * 规则：去重、升序；连续且未超 maxBatch 的缺失行合并为一批。
 */
export function planBatches(
  rows: number[],
  isCached: (row: number) => boolean,
  maxBatch: number,
): FetchBatch[] {
  const missing = [...new Set(rows)].filter((row) => !isCached(row)).sort((a, b) => a - b);
  const batches: FetchBatch[] = [];
  let start = -1;
  let previous = -2;
  let count = 0;
  for (const row of missing) {
    if (start < 0) {
      start = row;
      previous = row;
      count = 1;
      continue;
    }
    if (row === previous + 1 && count < Math.max(1, maxBatch)) {
      previous = row;
      count += 1;
      continue;
    }
    batches.push({ start, count });
    start = row;
    previous = row;
    count = 1;
  }
  if (start >= 0) batches.push({ start, count });
  return batches;
}
