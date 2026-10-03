// 简体中文语言包（**类型源**：MessageKey 由此推导；en.ts 必须覆盖全部键）。
// 命名约定：`域.元素[.变体]`，域与组件/功能区对应（如 toolbar.*、status.*）。
// 维护规则：新增文案必须先加在本文件，再由 TS 类型强制英文包补齐。

export const zhCN = {
  // ---------- 标题栏 ----------
  'titlebar.settings': '打开设置',
  'titlebar.minimize': '最小化',
  'titlebar.maximize': '最大化',
  'titlebar.restore': '还原',
  'titlebar.close': '关闭',

  // ---------- 工具栏 ----------
  'toolbar.open': '打开文件',
  'toolbar.openHint': '打开文件（Ctrl+O）',
  'toolbar.history': '历史记录',
  'toolbar.historyHint': '历史记录（Ctrl+Shift+H）',
  'toolbar.toggleEdit': '切换编辑模式',
  'toolbar.editEnableHint': '启用编辑模式（Ctrl+E）',
  'toolbar.editDisableHint': '退出编辑模式（Ctrl+E）',
  'toolbar.editReadOnlyHint': '文件超过只读阈值，不可编辑（可在设置中调整）',
  'toolbar.save': '保存',
  'toolbar.saveHint': '保存（Ctrl+S）',
  'toolbar.encoding': '编码：{value}',
  'toolbar.encodingAuto': '自动',
  'toolbar.theme': '主题：{name}（点击循环切换）',
  'toolbar.themeCycle': '切换主题',
  'toolbar.settings': '设置',

  // ---------- 主题名 ----------
  'theme.light': '浅色',
  'theme.dark': '深色',
  'theme.eye': '护眼',
  'theme.system': '跟随系统',

  // ---------- 状态栏 ----------
  'status.reading': '阅读 {percent}%',
  'status.readOnly': '只读',
  'status.readOnlyHint': '文件超过只读阈值，不可编辑（可在设置中调整）',
  'status.encodingHint': '切换编码',
  'status.noFile': '未打开文件',

  // ---------- 空状态 ----------
  'empty.title': '未打开任何文件',
  'empty.hint': '拖拽 TXT 文件到窗口，或点击下方按钮打开',
  'empty.open': '打开文件',
  'empty.shortcutHint': '快捷键 Ctrl+O · 支持同时打开多个文件',

  // ---------- 拖拽遮罩 ----------
  'drop.release': '松开以打开 TXT 文件',

  // ---------- 首启引导 ----------
  'onboarding.aria': '使用向导',
  'onboarding.title': '欢迎使用 S-Read-TXT',
  'onboarding.openFile': '打开文件',
  'onboarding.openFileDesc': '：工具栏「打开」或直接把 TXT 文件拖进窗口（Ctrl+O）',
  'onboarding.tabs': '多标签',
  'onboarding.tabsDesc': '：Ctrl+W 关闭、Ctrl+Tab 切换、Ctrl+1~9 直达第 N 个标签',
  'onboarding.history': '历史记录',
  'onboarding.historyDesc': '：自动记录打开过的文件，可在设置 → 历史记录中管理',
  'onboarding.shortcuts': '快捷键',
  'onboarding.shortcutsDesc': '：设置 → 快捷键，15 个动作全部可自定义',
  'onboarding.dontShow': '不再显示',
  'onboarding.start': '开始使用',
} as const;

/** 消息键（由 zh-CN 推导；所有语言包必须一字不差地覆盖）。 */
export type MessageKey = keyof typeof zhCN;
