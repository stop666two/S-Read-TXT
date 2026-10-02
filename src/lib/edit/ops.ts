// 编辑操作构造：把「光标动作」翻译为引擎的 EditOp（纯逻辑，可单测）。
//
// 坐标语义：EditOp 的位置是「应用前」的逻辑坐标（超长行分段场景由调用方
// 用 longline.ts 换算）；每次按键/输入只构造一个操作，引擎按批次原子应用
// （= 单个撤销步）。

import type { EditOp } from '../ipc';
import { orderedSelection, type CaretPos, type Selection } from './caret';
import type { DeleteRange } from './longline';

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

/** 按逻辑区间构造删除操作（退格/前删的规划结果）。 */
export function deleteRangeOp(range: DeleteRange): DeleteOp {
  return { kind: 'delete', ...range };
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
