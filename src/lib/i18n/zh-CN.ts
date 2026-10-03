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

  // ---------- 通用 ----------
  'common.cancel': '取消',
  'common.save': '保存',

  // ---------- 查找条 ----------
  'find.aria': '查找与替换',
  'find.query': '查找内容',
  'find.regex': '正则表达式',
  'find.regexHint': '正则表达式（Rust regex 语法；替换支持 $1 捕获展开）',
  'find.caseSensitive': '区分大小写',
  'find.next': '下一个',
  'find.close': '关闭查找',
  'find.closeHint': '关闭（Esc）',
  'find.replacement': '替换为',
  'find.replace': '替换',
  'find.replaceAll': '全部替换',

  // ---------- 保存对话框 ----------
  'save.title': '保存文件',
  'save.targetEncoding': '目标编码',
  'save.keepCurrent': '保持当前编码（{encoding}）',
  'save.backup': '写入 .bak 备份（覆盖磁盘上前一次内容）',

  // ---------- 未保存对话框 ----------
  'unsaved.discard': '不保存',

  // ---------- 数据目录对话框 ----------
  'dataDir.title': '数据目录不可写',
  'dataDir.line': '程序目录无法写入，历史记录、设置与会话将无法保存：',
  'dataDir.chooseHint1': '请选择一个可写目录（',
  'dataDir.chooseHintStrong': '仅本次运行有效',
  'dataDir.chooseHint2': '；若程序目录恢复可写，下次启动会自动回到便携模式）。',
  'dataDir.busy': '处理中…',
  'dataDir.choose': '选择可写目录（推荐）',
  'dataDir.skip': '仅本次只读运行',

  // ---------- 全部替换预览 ----------
  'replacePreview.aria': '全部替换预览',
  'replacePreview.title': '全部替换确认',
  'replacePreview.summarySelected': '共命中 {total} 处，已勾选 {selected} 处。',
  'replacePreview.summaryTruncated':
    '共命中 {total} 处。命中过多，仅列出前 {shown} 条；未列出的命中将一并替换。',
  'replacePreview.checkAria': '替换第 {row} 行',
  'replacePreview.lineNo': '第 {row} 行',
  'replacePreview.replacementLabel': '替换为',
  'replacePreview.selectAll': '全选',
  'replacePreview.selectNone': '全不选',
  'replacePreview.confirmAll': '全部替换 {total} 处',
  'replacePreview.confirmSelected': '替换已选 {selected} 处',

  // ---------- 历史面板 ----------
  'history.title': '历史记录',
  'history.search': '搜索文件名或路径',
  'history.searchAria': '搜索历史记录',
  'history.close': '关闭历史面板',
  'history.empty': '暂无历史记录',
  'history.noMatch': '没有匹配的记录',
  'history.deleteAria': '删除历史：{name}',
  'history.deleteHint': '删除这条记录',
  'history.progress': '读到 {percent}%',
  'history.noProgress': '未记录进度',
  'history.count': '共 {filtered} 条（显示 {total} 条中）',
  'history.clear': '清空历史',
  'history.clearTitle': '清空历史记录',
  'history.clearMessage': '将删除全部历史记录与阅读进度，且不可恢复。继续吗？',
  'history.clearConfirm': '清空',

  // ---------- 菜单栏 ----------
  'menu.aria': '主菜单',
  'menu.file': '文件',
  'menu.edit': '编辑',
  'menu.view': '查看',
  'menu.help': '帮助',
  'menu.file.open': '打开文件…',
  'menu.file.reload': '重新加载',
  'menu.file.recent': '最近打开',
  'menu.file.recentAria': '最近打开',
  'menu.file.history': '历史记录',
  'menu.file.settings': '设置…',
  'menu.file.quit': '退出',
  'menu.edit.undo': '撤销',
  'menu.edit.redo': '重做',
  'menu.edit.cut': '剪切',
  'menu.edit.copy': '复制',
  'menu.edit.paste': '粘贴',
  'menu.edit.selectAll': '全选',
  'menu.edit.find': '查找…',
  'menu.edit.replace': '替换…',
  'menu.edit.enableEdit': '启用编辑模式',
  'menu.edit.disableEdit': '退出编辑模式',
  'menu.edit.save': '保存',
  'menu.edit.saveAs': '另存为…',
  'menu.view.fontIncrease': '字号增大',
  'menu.view.fontDecrease': '字号减小',
  'menu.view.fontReset': '重置字号',
  'menu.view.fullscreen': '全屏',
  'menu.help.shortcuts': '快捷键…',
  'menu.help.about': '关于 S-Read-TXT',

  // ---------- 标签栏 ----------
  'tabBar.aria': '打开的文件',
  'tabBar.closeHint': '关闭标签（Ctrl+W）',
  'tabBar.closeAria': '关闭 {name}',

  // ---------- 标签右键菜单 ----------
  'tabMenu.aria': '标签操作',
  'tabMenu.close': '关闭',
  'tabMenu.closeOthers': '关闭其他',
  'tabMenu.closeAll': '关闭全部',

  // ---------- 编码菜单 ----------
  'encoding.switch': '切换编码',
  'encoding.auto': '自动检测',
} as const;

/** 消息键（由 zh-CN 推导；所有语言包必须一字不差地覆盖）。 */
export type MessageKey = keyof typeof zhCN;
