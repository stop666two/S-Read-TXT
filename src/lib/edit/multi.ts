/**
 * 多光标与矩形选择的纯逻辑（显示坐标；与组件解耦，便于单测）。
 *
 * 坐标约定：`CaretPos` = (显示行号, 行内 UTF-16 偏移)，与 EditLayer/编辑引擎一致；
 * 矩形选择的列区间为 `[fromCol, toCol)`（行内偏移，跨行统一列）。
 */
import type { CaretPos } from './caret';

/** 附加光标上限的保底值（真实值来自设置 `editor.multiCursor.maxCount`）。 */
export const MULTI_FALLBACK_MAX = 1000;

/** 规格化矩形：行列都取 min/max，与拖动方向无关。 */
export interface RectRange {
  fromRow: number;
  toRow: number;
  fromCol: number;
  toCol: number;
}

/** 某一行内的操作跨度 `[from, to)`。 */
export interface Span {
  row: number;
  from: number;
  to: number;
}

/** 同一位置判定（行号 + 行内 UTF-16）。 */
export function samePos(a: CaretPos, b: CaretPos): boolean {
  return a.row === b.row && a.utf16 === b.utf16;
}

/** 追加/移除一个附加光标（含主光标的总数不超过 maxCount；重复位置移除）。 */
export function toggleCaret(list: CaretPos[], pos: CaretPos, maxCount: number): CaretPos[] {
  const index = list.findIndex((item) => samePos(item, pos));
  if (index >= 0) {
    return list.filter((_, i) => i !== index);
  }
  if (list.length + 1 >= Math.max(2, maxCount)) {
    return list;
  }
  return [...list, pos];
}

/** 由锚点与终点规格化矩形。 */
export function normalizeRect(anchor: CaretPos, head: CaretPos): RectRange {
  return {
    fromRow: Math.min(anchor.row, head.row),
    toRow: Math.max(anchor.row, head.row),
    fromCol: Math.min(anchor.utf16, head.utf16),
    toCol: Math.max(anchor.utf16, head.utf16),
  };
}

/** 矩形是否退化为单点（此时按普通光标处理）。 */
export function rectIsCollapsed(rect: RectRange): boolean {
  return rect.fromRow === rect.toRow && rect.fromCol === rect.toCol;
}

/** 限制矩形行数（不改变起点；用于多光标数量上限保护）。 */
export function clampRectRows(rect: RectRange, maxRows: number): RectRange {
  const limit = Math.max(1, maxRows);
  const toRow = Math.min(rect.toRow, rect.fromRow + limit - 1);
  return toRow === rect.toRow ? rect : { ...rect, toRow };
}

/** 主光标 + 附加光标去重（保持输入顺序）。 */
export function uniquePositions(primary: CaretPos, extras: CaretPos[]): CaretPos[] {
  const seen = new Set<string>();
  const out: CaretPos[] = [];
  for (const pos of [primary, ...extras]) {
    const key = `${pos.row}:${pos.utf16}`;
    if (!seen.has(key)) {
      seen.add(key);
      out.push(pos);
    }
  }
  return out;
}

/** 矩形内每行的目标跨度（按各行长度钳制；空跨度表示插入点）。 */
export function rectSpans(lengthAt: (row: number) => number, rect: RectRange): Span[] {
  const spans: Span[] = [];
  for (let row = rect.fromRow; row <= rect.toRow; row += 1) {
    const length = lengthAt(row);
    spans.push({
      row,
      from: Math.min(rect.fromCol, length),
      to: Math.min(rect.toCol, length),
    });
  }
  return spans;
}

/** 矩形退格：每行删除列光标左侧 1 个 UTF-16 单元（列 0 行无操作）。 */
export function rectBackspaceSpans(lengthAt: (row: number) => number, rect: RectRange): Span[] {
  const spans: Span[] = [];
  for (let row = rect.fromRow; row <= rect.toRow; row += 1) {
    const length = lengthAt(row);
    const to = Math.min(rect.fromCol, length);
    const from = to - 1;
    if (from >= 0 && to > from) {
      spans.push({ row, from, to });
    }
  }
  return spans;
}

/** 矩形前向删除：每行删除列光标处 1 个 UTF-16 单元（行尾无操作）。 */
export function rectDeleteSpans(lengthAt: (row: number) => number, rect: RectRange): Span[] {
  const spans: Span[] = [];
  for (let row = rect.fromRow; row <= rect.toRow; row += 1) {
    const length = lengthAt(row);
    const from = Math.min(rect.fromCol, length);
    const to = Math.min(from + 1, length);
    if (to > from) {
      spans.push({ row, from, to });
    }
  }
  return spans;
}
