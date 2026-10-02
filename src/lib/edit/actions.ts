// 编辑动作信号：菜单/顶部工具栏与编辑层解耦的轻量通道。
// 约定：调用方自增 seq 并设置 type；编辑层 effect 检测到 seq 变化后执行一次。

/** 编辑动作类型（均要求处于编辑模式）。 */
export type EditActionType =
  | 'undo'
  | 'redo'
  | 'cut'
  | 'copy'
  | 'paste'
  | 'selectAll'
  | 'find'
  | 'replace';

/** 一次编辑动作（seq 单调递增，用于区分重复的同类型动作）。 */
export interface EditorAction {
  type: EditActionType;
  seq: number;
}
