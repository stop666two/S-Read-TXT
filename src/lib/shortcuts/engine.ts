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

/** 决策上下文（由调用方从真实事件与 DOM 提取；纯数据便于单测）。 */
export interface ShortcutContext {
  /** 事件目标是否处于编辑上下文 */
  editorContext: boolean;
  /** 是否有模态弹窗打开 */
  modalOpen: boolean;
  /** 事件已被更早的处理器消费 */
  defaultPrevented: boolean;
  /** IME 组合输入进行中 */
  composing: boolean;
}

/** 决策结果：动作、固定标签跳转或忽略（null）。 */
export type ShortcutDecision =
  | { kind: 'action'; action: ShortcutAction }
  | { kind: 'fixedTab'; index: number }
  | null;

/**
 * 快捷键决策（纯函数；App 的事件监听器只负责提取上下文与执行结果）。
 *
 * 规则优先级：
 * 1. 已被消费 / IME 组合中 / 无法识别的组合 → 忽略；
 * 2. 命中绑定动作：弹窗打开 → 挂起；编辑上下文且为编辑独占动作 → 让位；
 * 3. 未命中动作：`Ctrl+1~9` 固定跳转（弹窗打开时同样挂起）。
 */
export function decideShortcut(
  combo: string | null,
  bindings: ShortcutMap,
  context: ShortcutContext,
): ShortcutDecision {
  if (!combo || context.defaultPrevented || context.composing) return null;
  const action = actionForCombo(bindings, combo);
  if (!action) {
    const index = fixedTabIndex(combo);
    if (index !== null && !context.modalOpen) return { kind: 'fixedTab', index };
    return null;
  }
  if (context.modalOpen) return null;
  if (context.editorContext && EDITOR_OWNED.has(action)) return null;
  return { kind: 'action', action };
}

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
