// 快捷键引擎：组合键 → 动作匹配与上下文判定（src/lib/shortcuts/engine.ts）
// 纯匹配逻辑与 DOM 上下文判定分离；DOM 判定仅在真实窗口环境调用，单测只覆盖纯函数。

import type { ShortcutAction, ShortcutMap } from './types';

/** 编辑上下文选择器：焦点落在这些元素上时，编辑专属按键（PgUp/PgDn/Home/End）让位给编辑器。 */
const EDITOR_CONTEXT_SELECTOR = '.input-proxy, .find-bar, textarea, input, [role="search"]';

/** 编辑态独占的动作：由 EditLayer 处理（翻页=移动光标、首尾=文档首尾），引擎不得抢占。 */
export const EDITOR_OWNED: ReadonlySet<ShortcutAction> = new Set<ShortcutAction>([
  'pageDown',
  'pageUp',
  'firstLine',
  'lastLine',
]);

/** 弹窗（保存/冲突/未保存/重载确认）打开时的模态选择器。 */
const MODAL_SELECTOR = '[role="dialog"],[role="alertdialog"]';

/** 在生效绑定表中查找组合键对应的动作（大小写不敏感；无匹配返回 null）。 */
export function actionForCombo(bindings: ShortcutMap, combo: string): ShortcutAction | null {
  const upper = combo.toUpperCase();
  for (const [action, bound] of Object.entries(bindings)) {
    if (bound && bound.toUpperCase() === upper) return action as ShortcutAction;
  }
  return null;
}

/** 固定标签跳转键（`Ctrl+1`~`Ctrl+9`，不参与自定义）；返回 0 基下标或 null。 */
export function fixedTabIndex(combo: string): number | null {
  const match = /^Ctrl\+([1-9])$/.exec(combo);
  return match ? Number(match[1]) - 1 : null;
}

/** 事件目标是否处于编辑上下文（编辑区/查找条/输入框）。 */
export function isEditorContext(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(EDITOR_CONTEXT_SELECTOR) !== null;
}

/** 是否有模态弹窗打开（打开时引擎整体挂起，避免隔层触发动作）。 */
export function modalOpen(): boolean {
  return document.querySelector(MODAL_SELECTOR) !== null;
}
