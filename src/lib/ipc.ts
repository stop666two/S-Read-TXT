// IPC 封装：类型化命令包装 + 错误归一（Tauri 拒绝值即后端 IpcError 载荷）。
// 约定：组件与 store 一律经本模块访问后端，不直接 import invoke，便于类型与错误策略统一。

import { invoke } from '@tauri-apps/api/core';

import { t } from './i18n/runtime';

/** 应用信息（与 Rust commands.rs 的 AppInfo 对齐）。 */
export interface AppInfo {
  version: string;
  dataDir: string;
  dataDirOrigin: 'portable' | 'envOverride' | 'runtimeOverride';
}

/** 数据目录状态（data_dir_status / set_data_dir 返回体；与 Rust DataDirStatus 对齐）。 */
export interface DataDirStatus {
  /** 数据目录绝对路径 */
  dir: string;
  /** 是否可写（false 时 message 给出原因） */
  writable: boolean;
  /** 不可写原因（可写时为 null/缺省） */
  message?: string | null;
  /** 目录来源：便携 / 环境变量覆盖 / 会话级运行时覆盖 */
  origin: 'portable' | 'envOverride' | 'runtimeOverride';
}

/** 单项文本行（与 Rust textfile::window::RowText 对齐）。 */
export interface RowText {
  row: number;
  text: string;
  /** 逻辑行号（编辑态超长行分段时存在；普通行与 row 相同） */
  logicalRow?: number;
  /** 段首逻辑行内 UTF-16 偏移（编辑态超长行分段时存在；普通行为 0 可省略） */
  baseUtf16?: number;
}

/** 标签信息（与 Rust app_state::TabInfo 对齐）。 */
export interface TabInfo {
  tabId: number;
  path: string;
  name: string;
  encoding: string;
  encodingOverride: string | null;
  /** 是否处于编辑模式（编辑文档已创建且模式开关为开） */
  editing: boolean;
  /** 是否有未保存修改 */
  dirty: boolean;
  /** 是否只读（文件超过只读阈值：可浏览、不可进入编辑） */
  readOnly: boolean;
  rowsTotal: number;
  byteLen: number;
}

/** 取行载荷（与 Rust app_state::RowsPayload 对齐）。 */
export interface RowsPayload {
  tabId: number;
  startRow: number;
  rowsTotal: number;
  startPercent: number;
  rows: RowText[];
}

/** 标签视图（与 Rust commands::TabsView 对齐）。 */
export interface TabsView {
  tabs: TabInfo[];
  activeTabId: number | null;
}

/** 编辑操作（与 Rust textfile::editing::edit_doc::EditOp 对齐；kind 为外部标签）。 */
export type EditOp =
  | { kind: 'insert'; row: number; utf16: number; text: string }
  | { kind: 'delete'; startRow: number; startUtf16: number; endRow: number; endUtf16: number }
  | {
      kind: 'replace';
      startRow: number;
      startUtf16: number;
      endRow: number;
      endUtf16: number;
      text: string;
    };

/** 一次编辑应用结果（与 Rust EditApplied 对齐）。 */
export interface EditApplied {
  stateId: number;
  dirty: boolean;
  touchedRow: number;
  rowsTotal: number;
  byteLen: number;
  /** 应用后的光标显示行（后端换算，长行分段场景权威落点） */
  caretRow: number;
  /** 应用后的光标段内 UTF-16 偏移 */
  caretUtf16: number;
}

/** 查找命中（显示行坐标；超长行分段对前端透明）。 */
export interface FindHit {
  startRow: number;
  startUtf16: number;
  endRow: number;
  endUtf16: number;
}

/** 替换一次的结果（next = 后续命中，无则 null）。 */
export interface ReplaceNextOutcome {
  applied: EditApplied;
  next: FindHit | null;
}

/** 全部替换的结果（无命中时 applied 为 null）。 */
export interface ReplaceAllOutcome {
  replaced: number;
  applied: EditApplied | null;
}

/** 查找模式：标准（字面）或正则（用户自写 Rust regex 语法）。 */
export type SearchMode = 'literal' | 'regex';

/** 「全部替换」预览中的单条命中（确认弹窗展示与逐条剔除用）。 */
export interface ReplacePreviewItem {
  index: number;
  startRow: number;
  startUtf16: number;
  endRow: number;
  endUtf16: number;
  lineText: string;
  matchedText: string;
  replacementText: string;
}

/** 「全部替换」预览结果（stateId 用于执行时校验文档未变化）。 */
export interface ReplacePreview {
  stateId: number;
  total: number;
  truncated: boolean;
  items: ReplacePreviewItem[];
}

/** 保存结果（与 Rust commands::SaveTabResult 对齐）。 */
export interface SaveTabResult {
  bytesWritten: number;
  backupPath: string | null;
  encoding: string;
  tab: TabInfo;
}

/** 后端统一错误载荷（与 Rust ipc_error::IpcError 对齐）。 */
export interface IpcErrorPayload {
  code: string;
  message: string;
}

/**
 * 把任意异常归一为 IpcError 载荷。
 * Tauri 命令拒绝值即后端序列化的 IpcError；其余异常（网络/JS 错误）兜底为 UNKNOWN。
 */
export function toIpcError(error: unknown): IpcErrorPayload {
  if (typeof error === 'object' && error !== null) {
    const candidate = error as Partial<IpcErrorPayload>;
    if (typeof candidate.code === 'string' && typeof candidate.message === 'string') {
      return { code: candidate.code, message: candidate.message };
    }
  }
  return {
    code: 'UNKNOWN',
    message: error instanceof Error ? error.message : String(error),
  };
}

/**
 * 错误载荷 → 用户文案。
 * 固定展示文案优先（如文件过大使用需求给定的逐字提示），其余透出后端消息。
 */
export function describeIpcError(error: IpcErrorPayload): string {
  switch (error.code) {
    case 'FILE_TOO_LARGE':
      return t('error.fileTooLarge');
    case 'MAX_TABS':
      return t('error.maxTabs');
    default:
      return error.message;
  }
}

/** 历史保留策略（与 Rust `HistorySettings` 对应）。 */
export interface HistorySettings {
  maxEntries: number;
  retentionDays: number;
}

/** 启动行为（与 Rust `StartupSettings` 对应）。 */
export interface StartupSettings {
  restoreSession: boolean;
  restoreWindow: boolean;
}

/** 主配置（与 Rust `AppSettings` 对应；字段名以 Rust 序列化为准）。 */
export interface AppSettings {
  schemaVersion: number;
  logLevel: string;
  maxFileSizeMB: number;
  hardLimitMB: number;
  maxTabs: number;
  history: HistorySettings;
  saveBackupEnabled: boolean;
  showOnboarding: boolean;
  locale: 'zh-CN' | 'en';
  startup: StartupSettings;
}

/** 排版配置（与 Rust `TypographySettings` 对应）。 */
export interface TypographySettings {
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  contentWidth: number;
  pagePadding: number;
  pagePaddingY: number;
  paragraphSpacing: number;
  firstLineIndent: number;
  textAlign: 'left' | 'justify';
  smoothScroll: boolean;
}

/** 状态栏元素显隐（与 Rust `StatusBarSettings` 对应）。 */
export interface StatusBarSettings {
  showFileName: boolean;
  showPercent: boolean;
  showSize: boolean;
  showEncoding: boolean;
}

/** 阅读配置（与 Rust `ReaderSettings` 对应）。 */
export interface ReaderSettings {
  schemaVersion: number;
  theme: string;
  typography: TypographySettings;
  statusBar: StatusBarSettings;
}

/** 自定义字体条目（与 Rust `FontEntry` 对应）。 */
export interface FontEntry {
  fileName: string;
  label: string;
  sizeBytes: number;
}

/** 快捷键配置（与 Rust `ShortcutSettings` 对应；bindings 为「生效绑定」）。 */
export interface ShortcutSettings {
  schemaVersion: number;
  bindings: Record<string, string>;
}

/** 历史记录条目（与 Rust `HistoryEntry` 对应）。 */
export interface HistoryEntry {
  path: string;
  name: string;
  size: number;
  encoding: string;
  openedAt: string;
  lastRow: number;
  lastPercent: number;
}

/** 窗口状态（会话恢复用；x/y 可为空表示未定位）。 */
export interface WindowState {
  x: number | null;
  y: number | null;
  width: number;
  height: number;
  maximized: boolean;
}

/** 会话中的单个标签（恢复用；scrollRow = 顶部定位行）。 */
export interface SessionTab {
  path: string;
  encoding: string | null;
  scrollRow: number;
  editMode: boolean;
}

/** 会话状态（与 Rust `SessionState` 对应）。 */
export interface SessionState {
  schemaVersion: number;
  window: WindowState;
  activeTabIndex: number;
  tabs: SessionTab[];
}

/** 设置项类型（与 Rust `SettingKind` 对应；tag = type）。 */
export type SettingKind =
  | { type: 'number'; min: number; max: number; integer: boolean }
  | { type: 'bool' }
  | { type: 'enum'; values: string[] }
  | { type: 'text'; maxLen: number }
  | { type: 'shortcuts' };

/** 设置项元数据（与 Rust `SettingSpec` 对应；标签/描述由前端按 `setting.<id>` 解析语言包）。 */
export interface SettingSpec {
  id: string;
  group: string;
  kind: SettingKind;
}

/** 重置作用域（与 Rust `ResetScope` 对应）。 */
export type ResetScope =
  | { kind: 'all' }
  | { kind: 'group'; name: string }
  | { kind: 'field'; id: string };

/** 磁盘占用分项（与 Rust `DiskUsageItem` 对应）。 */
export interface DiskUsageItem {
  key: 'logs' | 'webview' | 'fonts' | 'backups' | 'files' | 'others';
  bytes: number;
  files: number;
}

/** 磁盘占用报告（与 Rust `DiskUsageReport` 对应）。 */
export interface DiskUsageReport {
  items: DiskUsageItem[];
  totalBytes: number;
}

/** 清理结果（与 Rust `ClearResult` 对应）。 */
export interface ClearResult {
  clearedBytes: number;
  skipped: number;
}

/** 配置聚合快照（`get_settings` 返回体）。 */
export interface SettingsSnapshot {
  app: AppSettings;
  reader: ReaderSettings;
  shortcuts: ShortcutSettings;
}

/** 配置保存请求（`save_settings` 入参；shortcuts 直接传「生效绑定」扁平表）。 */
export interface SettingsSaveRequest {
  app: AppSettings;
  reader: ReaderSettings;
  shortcuts: Record<string, string>;
}

/** 类型化 IPC 命令集合（参数名与 Tauri 的 camelCase 约定一致）。 */
export const ipc = {
  /** 应用信息（版本 / 数据目录）。 */
  getAppInfo: () => invoke<AppInfo>('get_app_info'),
  /** 数据目录可写性状态（启动引导的数据源）。 */
  dataDirStatus: () => invoke<DataDirStatus>('data_dir_status'),
  /** 设置会话级数据目录（不可写引导；所有后续读写改路至新目录）。 */
  setDataDir: (dir: string) => invoke<DataDirStatus>('set_data_dir', { dir }),
  /** 打开文件（重复打开由后端复用标签）。 */
  openFile: (path: string) => invoke<TabInfo>('open_file', { path }),
  /** 取文本窗口（count 上限 2048）。 */
  getRows: (tabId: number, startRow: number, count: number) =>
    invoke<RowsPayload>('get_rows', { tabId, startRow, count }),
  /** 切换编码（null = 恢复自动检测）。 */
  setEncoding: (tabId: number, encoding: string | null) =>
    invoke<TabInfo>('set_encoding', { tabId, encoding }),
  /** 支持的编码列表。 */
  listEncodings: () => invoke<string[]>('list_encodings'),
  /** 全部标签视图。 */
  listTabs: () => invoke<TabsView>('list_tabs'),
  /** 关闭标签（返回剩余视图）。 */
  closeTab: (tabId: number) => invoke<TabsView>('close_tab', { tabId }),
  /** 同步活动标签到后端（点击/快捷键选择后调用）。 */
  setActiveTab: (tabId: number) => invoke<void>('set_active_tab', { tabId }),
  /** 调整标签展示顺序（拖拽排序；下标记「移除后再插入」语义）。 */
  reorderTab: (tabId: number, toIndex: number) => invoke<void>('reorder_tab', { tabId, toIndex }),
  /** 打开设置窗口（已存在则聚焦；按需创建；tab 指定初始页签）。 */
  openSettings: (tab?: string) => invoke<void>('open_settings', { tab: tab ?? null }),
  /** 取走设置窗口待打开页签（读取即清空；无待办返回 null）。 */
  takeSettingsTab: () => invoke<string | null>('take_settings_tab'),
  /** 列出已导入的自定义字体。 */
  listFonts: () => invoke<FontEntry[]>('list_fonts'),
  /** 导入字体文件（复制到数据目录 fonts/；重名自动唯一化）。 */
  importFont: (path: string) => invoke<FontEntry>('import_font', { path }),
  /** 删除已导入字体。 */
  removeFont: (fileName: string) => invoke<void>('remove_font', { fileName }),
  /** 读取字体字节（Base64；前端经 FontFace 动态注册）。 */
  readFontData: (fileName: string) => invoke<string>('read_font_data', { fileName }),
  /** 切换编辑模式（首次进入创建编辑文档）。 */
  toggleEdit: (tabId: number) => invoke<TabInfo>('toggle_edit', { tabId }),
  /** 应用编辑批次（批次 = 单个撤销步）。 */
  applyEdits: (tabId: number, ops: EditOp[]) =>
    invoke<EditApplied>('apply_edits', { tabId, ops }),
  /** 撤销一步（无可撤销内容返回 null）。 */
  undoEdit: (tabId: number) => invoke<EditApplied | null>('undo_edit', { tabId }),
  /** 重做一步（无可重做内容返回 null）。 */
  redoEdit: (tabId: number) => invoke<EditApplied | null>('redo_edit', { tabId }),
  /** 保存（targetEncoding = null 表示保持当前编码；force = 冲突时覆盖）。 */
  saveTab: (tabId: number, targetEncoding: string | null, makeBackup: boolean, force: boolean) =>
    invoke<SaveTabResult>('save_tab', { tabId, targetEncoding, makeBackup, force }),
  /** 另存为（成功后标签重定向到新路径）。 */
  saveTabAs: (tabId: number, newPath: string, targetEncoding: string | null, makeBackup: boolean) =>
    invoke<SaveTabResult>('save_tab_as', { tabId, newPath, targetEncoding, makeBackup }),
  /** 从磁盘重载（丢弃未保存修改）。 */
  reloadTab: (tabId: number) => invoke<TabInfo>('reload_tab', { tabId }),
  /** 查找下一个（from = 显示坐标；不环绕，由前端在文末后从头重试）。 */
  findInEdit: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    from: [number, number] | null,
  ) => invoke<FindHit | null>('find_in_edit', { tabId, query, caseSensitive, mode, from }),
  /** 替换一次（从 from 起）并返回后续命中；正则替换支持 $1 捕获展开。 */
  replaceInEdit: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    from: [number, number] | null,
    replacement: string,
  ) =>
    invoke<ReplaceNextOutcome | null>('replace_in_edit', {
      tabId,
      query,
      caseSensitive,
      mode,
      from,
      replacement,
    }),
  /** 全部替换（单撤销步；正常流程请用预览 + applyReplaceAll）。 */
  replaceAllInEdit: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    replacement: string,
  ) =>
    invoke<ReplaceAllOutcome>('replace_all_in_edit', {
      tabId,
      query,
      caseSensitive,
      mode,
      replacement,
    }),
  /** 生成「全部替换」预览（二次确认弹窗数据源；命中过多时报 QUERY_TOO_BROAD）。 */
  previewReplaceAll: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    replacement: string,
  ) =>
    invoke<ReplacePreview>('preview_replace_all_in_edit', {
      tabId,
      query,
      caseSensitive,
      mode,
      replacement,
    }),
  /** 执行「全部替换」：selected=null 全部；数组 = 仅替换所列序号（预览剔除后）。 */
  applyReplaceAll: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    replacement: string,
    selected: number[] | null,
    expectStateId: number,
  ) =>
    invoke<ReplaceAllOutcome>('apply_replace_all_in_edit', {
      tabId,
      query,
      caseSensitive,
      mode,
      replacement,
      selected,
      expectStateId,
    }),
  /** 显示行窗口内的命中（文档高亮用；扫描越过窗口即停止）。 */
  matchWindow: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    startRow: number,
    count: number,
  ) =>
    invoke<FindHit[]>('match_window_in_edit', {
      tabId,
      query,
      caseSensitive,
      mode,
      startRow,
      count,
    }),
  /** 配置快照（快捷键等；后端为唯一真源）。 */
  getSettings: () => invoke<SettingsSnapshot>('get_settings'),
  /** 保存配置（返回保存后的快照）。 */
  saveSettings: (request: SettingsSaveRequest) =>
    invoke<SettingsSnapshot>('save_settings', { request }),
  /** 设置项注册表（设置界面动态生成 / 导入校验 / 重置作用域的唯一元数据源）。 */
  getSettingsRegistry: () => invoke<SettingSpec[]>('get_settings_registry'),
  /** 导出全部设置到指定路径（返回写入字节数）。 */
  exportSettings: (path: string) => invoke<number>('export_settings', { path }),
  /** 从导出文件导入设置（强校验；返回导入后的快照）。 */
  importSettings: (path: string) => invoke<SettingsSnapshot>('import_settings', { path }),
  exportShortcuts: (path: string) => invoke<number>('export_shortcuts', { path }),
  importShortcuts: (path: string) => invoke<SettingsSnapshot>('import_shortcuts', { path }),
  /** 重置设置（全部 / 分组 / 单项；返回重置后的快照）。 */
  resetSettings: (scope: ResetScope) => invoke<SettingsSnapshot>('reset_settings', { scope }),
  getDiskUsage: () => invoke<DiskUsageReport>('get_disk_usage'),
  clearCache: (scope: 'logs' | 'webview' | 'backups') => invoke<ClearResult>('clear_cache', { scope }),
  /** 默认快捷键表（设置界面「恢复默认」用）。 */
  getDefaultShortcuts: () => invoke<Record<string, string>>('get_default_shortcuts'),
  /** 历史记录（去重剪枝后、时间倒序）。 */
  getHistory: () => invoke<HistoryEntry[]>('get_history'),
  /** 删除单条历史（以文件路径为键；返回更新后的列表）。 */
  removeHistory: (filePath: string) => invoke<HistoryEntry[]>('remove_history', { filePath }),
  /** 清空历史记录。 */
  clearHistory: () => invoke<void>('clear_history'),
  /** 更新历史条目阅读进度（关闭标签/退出前调用；尽力而为，幂等）。 */
  updateHistoryProgress: (filePath: string, lastRow: number, lastPercent: number) =>
    invoke<void>('update_history_progress', { path: filePath, lastRow, lastPercent }),
  /** 读取会话（窗口/标签/滚动；无会话返回默认值）。 */
  getSession: () => invoke<SessionState>('get_session'),
  /** 保存会话（返回归一化后的结果）。 */
  saveSession: (session: SessionState) => invoke<SessionState>('save_session', { session }),
};
