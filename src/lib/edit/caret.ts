// 光标与选区模型：纯逻辑（不触 DOM），供编辑层与单测使用。
//
// 坐标约定与编辑引擎一致：位置 = (行号, 行内 UTF-16 偏移)，且偏移位于
// 「字符边界」上（代理对不可分割；移动时会跨越成对代理项）。

/** 光标位置（行号 + 行内 UTF-16 偏移）。 */
export interface CaretPos {
  /** 逻辑行号（0 起） */
  row: number;
  /** 行内 UTF-16 偏移（0 起，位于字符边界） */
  utf16: number;
}

/** 选区：anchor 固定端，head 活动端（拖选/Shift 移动改变 head）。 */
export interface Selection {
  /** 选区锚点（开始拖选时的位置） */
  anchor: CaretPos;
  /** 活动端（随输入/方向键移动） */
  head: CaretPos;
}

/** 行尾哨兵：需要「行末之后」语义时使用（使用前必须 clamp）。 */
export const UTF16_END = Number.MAX_SAFE_INTEGER;

/** 位置比较：a 在前返回负数，相等 0，在后正数。 */
export function posCompare(a: CaretPos, b: CaretPos): number {
  if (a.row !== b.row) return a.row < b.row ? -1 : 1;
  if (a.utf16 !== b.utf16) return a.utf16 < b.utf16 ? -1 : 1;
  return 0;
}

/** 位置相等判断。 */
export function posEquals(a: CaretPos, b: CaretPos): boolean {
  return a.row === b.row && a.utf16 === b.utf16;
}

/** 选区有序化：返回文档顺序的 [start, end)。 */
export function orderedSelection(sel: Selection): { start: CaretPos; end: CaretPos } {
  return posCompare(sel.anchor, sel.head) <= 0
    ? { start: sel.anchor, end: sel.head }
    : { start: sel.head, end: sel.anchor };
}

/** 选区是否折叠（无选中内容）。 */
export function isCollapsed(sel: Selection): boolean {
  return posEquals(sel.anchor, sel.head);
}

/** 折叠为单光标。 */
export function collapsed(pos: CaretPos): Selection {
  return { anchor: pos, head: pos };
}

/** 把位置钳制到文档范围内（行号与行内长度）。 */
export function clampPos(
  pos: CaretPos,
  rowsTotal: number,
  rowLength: (row: number) => number,
): CaretPos {
  const row = Math.min(Math.max(0, pos.row), Math.max(0, rowsTotal - 1));
  const utf16 = Math.min(Math.max(0, pos.utf16), rowLength(row));
  return { row, utf16 };
}

/** 代理项判断（UTF-16 码元）。 */
function isLowSurrogate(code: number): boolean {
  return code >= 0xdc00 && code <= 0xdfff;
}

function isHighSurrogate(code: number): boolean {
  return code >= 0xd800 && code <= 0xdbff;
}

/**
 * 左移一列（跨行到上一行行尾）；代理对中间自动跨越成对码元。
 * `rowText` 取不到文本时按码元步进（极端降级，不阻塞输入）。
 */
export function moveLeft(pos: CaretPos, rowText: (row: number) => string | undefined): CaretPos {
  if (pos.utf16 > 0) {
    let utf16 = pos.utf16 - 1;
    const text = rowText(pos.row);
    if (text !== undefined && utf16 < text.length && isLowSurrogate(text.charCodeAt(utf16))) {
      utf16 -= 1;
    }
    return { row: pos.row, utf16: Math.max(0, utf16) };
  }
  if (pos.row > 0) {
    const prev = pos.row - 1;
    const text = rowText(prev);
    return { row: prev, utf16: text !== undefined ? text.length : UTF16_END };
  }
  return pos;
}

/** 右移一列（跨行到下一行行首）；代理对中间自动跨越成对码元。 */
export function moveRight(
  pos: CaretPos,
  rowsTotal: number,
  rowText: (row: number) => string | undefined,
): CaretPos {
  const text = rowText(pos.row);
  const length = text !== undefined ? text.length : UTF16_END;
  if (pos.utf16 < length) {
    let utf16 = pos.utf16 + 1;
    if (text !== undefined && isHighSurrogate(text.charCodeAt(pos.utf16))) {
      utf16 += 1;
    }
    return { row: pos.row, utf16 };
  }
  if (pos.row + 1 < rowsTotal) {
    return { row: pos.row + 1, utf16: 0 };
  }
  return pos;
}

/** 垂直移动一行（列偏移原样保留；目标行钳制由调用方/引擎处理）。 */
export function moveVertical(pos: CaretPos, delta: number, rowsTotal: number): CaretPos {
  const row = Math.min(Math.max(0, pos.row + delta), Math.max(0, rowsTotal - 1));
  return { row, utf16: pos.utf16 };
}

/** 行首。 */
export function moveHome(pos: CaretPos): CaretPos {
  return { row: pos.row, utf16: 0 };
}

/** 行尾（使用哨兵，由调用方 clamp 到真实长度）。 */
export function moveEnd(pos: CaretPos): CaretPos {
  return { row: pos.row, utf16: UTF16_END };
}
