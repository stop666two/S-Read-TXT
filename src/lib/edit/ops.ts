// 编辑操作构造：把「光标动作」翻译为引擎的 EditOp 批次（纯逻辑，可单测）。
//
// 坐标语义：EditOp 的位置是「应用前」状态；每次按键/输入只构造一个操作，
// 引擎按批次原子应用（= 单个撤销步）。

import type { EditOp } from '../ipc';
import { orderedSelection, posEquals, type CaretPos, type Selection } from './caret';

/** 插入操作（收窄类型，便于调用方直接访问字段）。 */
export type InsertOp = Extract<EditOp, { kind: 'insert' }>;
/** 删除操作。 */
export type DeleteOp = Extract<EditOp, { kind: 'delete' }>;
/** 替换操作。 */
export type ReplaceOp = Extract<EditOp, { kind: 'replace' }>;

/** 光标处插入文本。 */
export function insertOp(pos: CaretPos, text: string): InsertOp {
  return { kind: 'insert', row: pos.row, utf16: pos.utf16, text };
}

/** 删除选区（用 Delete 单操作表达）。 */
export function deleteOp(sel: Selection): DeleteOp {
  const { start, end } = orderedSelection(sel);
  return {
    kind: 'delete',
    startRow: start.row,
    startUtf16: start.utf16,
    endRow: end.row,
    endUtf16: end.utf16,
  };
}

/** 选区替换为文本（折叠选区时等价于插入）。 */
export function replaceOp(sel: Selection, text: string): ReplaceOp {
  const { start, end } = orderedSelection(sel);
  return {
    kind: 'replace',
    startRow: start.row,
    startUtf16: start.utf16,
    endRow: end.row,
    endUtf16: end.utf16,
    text,
  };
}

/** 判断代理对码元（用于退格/删除时的整对跨越）。 */
function isLowSurrogate(code: number): boolean {
  return code >= 0xdc00 && code <= 0xdfff;
}

function isHighSurrogate(code: number): boolean {
  return code >= 0xd800 && code <= 0xdbff;
}

/**
 * 退格：返回删除操作（无操作时 null）。
 * - 行内：删除光标前一个字符（代理对整对删除）；
 * - 行首：与上一行合并（删除行间换行）；
 * - 文档首：null。
 */
export function backspaceOp(
  pos: CaretPos,
  currentRowText: string | undefined,
  prevRowText: string | undefined,
): DeleteOp | null {
  if (pos.utf16 > 0) {
    let start = pos.utf16 - 1;
    if (
      currentRowText !== undefined &&
      start < currentRowText.length &&
      isLowSurrogate(currentRowText.charCodeAt(start))
    ) {
      start -= 1;
    }
    return {
      kind: 'delete',
      startRow: pos.row,
      startUtf16: Math.max(0, start),
      endRow: pos.row,
      endUtf16: pos.utf16,
    };
  }
  if (pos.row > 0 && prevRowText !== undefined) {
    return {
      kind: 'delete',
      startRow: pos.row - 1,
      startUtf16: prevRowText.length,
      endRow: pos.row,
      endUtf16: 0,
    };
  }
  return null;
}

/**
 * 前向删除（Delete 键）：返回删除操作（无操作时 null）。
 * - 行内：删除光标后一个字符（代理对整对删除）；
 * - 行尾且有下一行：与下一行合并；
 * - 文档末：null。
 */
export function deleteForwardOp(
  pos: CaretPos,
  currentRowText: string | undefined,
  hasNextRow: boolean,
): DeleteOp | null {
  const length = currentRowText?.length ?? 0;
  if (pos.utf16 < length) {
    let end = pos.utf16 + 1;
    if (currentRowText !== undefined && isHighSurrogate(currentRowText.charCodeAt(pos.utf16))) {
      end += 1;
    }
    return {
      kind: 'delete',
      startRow: pos.row,
      startUtf16: pos.utf16,
      endRow: pos.row,
      endUtf16: end,
    };
  }
  if (hasNextRow) {
    return {
      kind: 'delete',
      startRow: pos.row,
      startUtf16: length,
      endRow: pos.row + 1,
      endUtf16: 0,
    };
  }
  return null;
}

/**
 * 选区文本（用于复制/剪切）。
 * 边界：行文本缺失时截断返回已拼接部分（调用方应确保行已加载）。
 */
export function selectionText(
  sel: Selection,
  rowsTotal: number,
  rowText: (row: number) => string | undefined,
): string {
  if (posEquals(sel.anchor, sel.head)) return '';
  const { start, end } = orderedSelection(sel);
  const lastRow = Math.max(0, Math.min(end.row, rowsTotal - 1));
  const parts: string[] = [];
  for (let row = start.row; row <= lastRow; row += 1) {
    const text = rowText(row);
    if (text === undefined) return parts.join('\n');
    const from = row === start.row ? Math.min(start.utf16, text.length) : 0;
    const to = row === end.row ? Math.min(end.utf16, text.length) : text.length;
    parts.push(text.slice(from, to));
  }
  return parts.join('\n');
}
