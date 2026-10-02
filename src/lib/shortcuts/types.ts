// 快捷键动作类型与展示元数据（src/lib/shortcuts/types.ts）
// 作用：与 Rust 侧 `settings::defaults::DEFAULT_BINDINGS` 的动作 id 一一对应；
//       动作 id 是配置文件的稳定标识，禁止重命名（改名为破坏性变更）。

/** 可自定义快捷键的动作 id（与后端默认表保持一致，顺序即设置界面的展示顺序）。 */
export type ShortcutAction =
  | 'openFile'
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

/** 动作中文名称（设置界面展示）。 */
export const SHORTCUT_LABELS: Record<ShortcutAction, string> = {
  openFile: '打开文件',
  save: '保存',
  saveAs: '另存为',
  toggleEdit: '切换编辑模式',
  closeTab: '关闭当前标签',
  nextTab: '下一个标签',
  prevTab: '上一个标签',
  pageDown: '向下翻页',
  pageUp: '向上翻页',
  firstLine: '跳到文件开头',
  lastLine: '跳到文件结尾',
  fullscreen: '全屏',
  find: '查找（编辑态）',
  replace: '替换（编辑态）',
  historyPanel: '历史记录面板',
};

/** 动作 → 组合键字符串的映射（值形如 `Ctrl+Shift+Tab` / `PgDn` / `F11`）。 */
export type ShortcutMap = Partial<Record<ShortcutAction, string>>;
