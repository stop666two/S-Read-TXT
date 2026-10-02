// 超长行（显示分段）坐标换算与边界编辑规划（纯函数，可单测）。
//
// 坐标：
// - 显示坐标 (segRow, segUtf16)：段号 = 虚拟滚动行号；segUtf16 = 段内 UTF-16 偏移；
// - 逻辑坐标 (logicalRow, baseUtf16 + segUtf16)：编辑引擎使用的坐标；
// - 普通行（未分段）两套坐标恒等（logicalRow = segRow、baseUtf16 = 0）。

/** 段的坐标元数据（与后端 RowText 载荷一致；普通行 baseUtf16 = 0）。 */
export interface SegMeta {
  /** 逻辑行号 */
  logicalRow: number;
  /** 段首在逻辑行内的 UTF-16 偏移 */
  baseUtf16: number;
  /** 段文本（用于相邻字符探测） */
  text: string;
}

/** 删除区间（逻辑坐标）。 */
export interface DeleteRange {
  startRow: number;
  startUtf16: number;
  endRow: number;
  endUtf16: number;
}

function isLowSurrogate(code: number): boolean {
  return code >= 0xdc00 && code <= 0xdfff;
}

function isHighSurrogate(code: number): boolean {
  return code >= 0xd800 && code <= 0xdbff;
}

/** 位置前一个字符的 UTF-16 单元数（代理对整体处理；越界防御为 1）。 */
export function unitBefore(text: string, offset: number): number {
  const last = offset - 1;
  if (
    last > 0 &&
    isLowSurrogate(text.charCodeAt(last)) &&
    isHighSurrogate(text.charCodeAt(last - 1))
  ) {
    return 2;
  }
  return 1;
}

/** 位置后一个字符的 UTF-16 单元数（代理对整体处理；越界防御为 1）。 */
export function unitAfter(text: string, offset: number): number {
  if (
    offset >= 0 &&
    offset + 1 < text.length &&
    isHighSurrogate(text.charCodeAt(offset)) &&
    isLowSurrogate(text.charCodeAt(offset + 1))
  ) {
    return 2;
  }
  return 1;
}

/** 段内偏移 → 逻辑坐标。 */
export function toLogical(meta: SegMeta, segUtf16: number): { row: number; utf16: number } {
  return { row: meta.logicalRow, utf16: meta.baseUtf16 + segUtf16 };
}

/** 退格：返回待删除的逻辑区间（无操作返回 null）。
 *  - 段内：删除段内前一个字符；
 *  - 段首且上一段属同一逻辑行：跨段删除该字符；
 *  - 逻辑行首（上一显示行属上一逻辑行）：删行间换行；
 *  - 文档首：null。 */
export function planBackspace(
  cur: SegMeta,
  segUtf16: number,
  prev: SegMeta | null,
): DeleteRange | null {
  if (segUtf16 > 0) {
    const k = unitBefore(cur.text, segUtf16);
    return {
      startRow: cur.logicalRow,
      startUtf16: cur.baseUtf16 + segUtf16 - k,
      endRow: cur.logicalRow,
      endUtf16: cur.baseUtf16 + segUtf16,
    };
  }
  if (!prev) return null;
  if (prev.logicalRow === cur.logicalRow) {
    const k = unitBefore(prev.text, prev.text.length);
    return {
      startRow: cur.logicalRow,
      startUtf16: cur.baseUtf16 - k,
      endRow: cur.logicalRow,
      endUtf16: cur.baseUtf16,
    };
  }
  return {
    startRow: prev.logicalRow,
    startUtf16: prev.baseUtf16 + prev.text.length,
    endRow: cur.logicalRow,
    endUtf16: cur.baseUtf16,
  };
}

/** 前向删除（Delete 键）：返回待删除的逻辑区间（无操作返回 null）。
 *  - 段内：删除段内后一个字符；
 *  - 段尾且下一段属同一逻辑行：跨段删除该字符；
 *  - 逻辑行尾：删行间换行；
 *  - 文档末：null。 */
export function planDeleteForward(
  cur: SegMeta,
  segUtf16: number,
  next: SegMeta | null,
): DeleteRange | null {
  if (segUtf16 < cur.text.length) {
    const k = unitAfter(cur.text, segUtf16);
    return {
      startRow: cur.logicalRow,
      startUtf16: cur.baseUtf16 + segUtf16,
      endRow: cur.logicalRow,
      endUtf16: cur.baseUtf16 + segUtf16 + k,
    };
  }
  if (!next) return null;
  const rowEnd = cur.baseUtf16 + cur.text.length;
  if (next.logicalRow === cur.logicalRow) {
    const k = unitAfter(next.text, 0);
    return {
      startRow: cur.logicalRow,
      startUtf16: rowEnd,
      endRow: cur.logicalRow,
      endUtf16: rowEnd + k,
    };
  }
  return {
    startRow: cur.logicalRow,
    startUtf16: rowEnd,
    endRow: next.logicalRow,
    endUtf16: next.baseUtf16,
  };
}
