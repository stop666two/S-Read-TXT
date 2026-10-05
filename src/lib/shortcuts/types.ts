// 快捷键动作类型与展示元数据（src/lib/shortcuts/types.ts）
// 作用：与 Rust 侧 `settings::defaults::DEFAULT_BINDINGS` 的动作 id 一一对应；
//       动作 id 是配置文件的稳定标识，禁止重命名（改名为破坏性变更）。

/** 可自定义快捷键的动作 id（与后端默认表保持一致，顺序即设置界面的展示顺序）。 */
export type ShortcutAction =
  | 'openFile'
  | 'newWindow'
  | 'save'
  | 'saveAs'
  | 'toggleEdit'
  | 'closeTab'
  | 'nextTab'
  | 'prevTab'
  | 'pageDown'
  | 'pageUp'
  | 'firstLine'
  | 'lastLine'
  | 'fullscreen'
  | 'find'
  | 'replace'
  | 'historyPanel';

/** 动作展示顺序（设置界面行顺序）。 */
export const SHORTCUT_ACTIONS: readonly ShortcutAction[] = [
  'openFile',
  'newWindow',
  'save',
  'saveAs',
  'toggleEdit',
  'closeTab',
  'nextTab',
  'prevTab',
  'pageDown',
  'pageUp',
  'firstLine',
  'lastLine',
  'fullscreen',
  'find',
  'replace',
  'historyPanel',
];

import type { MessageKey } from '../i18n/zh-CN';

/** 动作名称消息键（设置界面经 t() 展示；语言切换即时生效）。 */
export const SHORTCUT_LABEL_KEYS: Record<ShortcutAction, MessageKey> = {
  openFile: 'shortcut.openFile',
  newWindow: 'shortcut.newWindow',
  save: 'shortcut.save',
  saveAs: 'shortcut.saveAs',
  toggleEdit: 'shortcut.toggleEdit',
  closeTab: 'shortcut.closeTab',
  nextTab: 'shortcut.nextTab',
  prevTab: 'shortcut.prevTab',
  pageDown: 'shortcut.pageDown',
  pageUp: 'shortcut.pageUp',
  firstLine: 'shortcut.firstLine',
  lastLine: 'shortcut.lastLine',
  fullscreen: 'shortcut.fullscreen',
  find: 'shortcut.find',
  replace: 'shortcut.replace',
  historyPanel: 'shortcut.historyPanel',
};

/** 动作 → 组合键字符串的映射（值形如 `Ctrl+Shift+Tab` / `PgDn` / `F11`）。 */
export type ShortcutMap = Partial<Record<ShortcutAction, string>>;
