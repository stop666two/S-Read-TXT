// IPC 封装：类型化命令包装 + 错误归一（Tauri 拒绝值即后端 IpcError 载荷）。
// 约定：组件与 store 一律经本模块访问后端，不直接 import invoke，便于类型与错误策略统一。

import { invoke } from '@tauri-apps/api/core';

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
      return '很抱歉，文件过大无法打开，可以在设置里面调整。';
    case 'MAX_TABS':
      return '标签数量已达上限，请先关闭部分标签（上限可在设置中调整）。';
    default:
      return error.message;
  }
}

/** 历史保留策略（与 Rust `HistorySettings` 对应）。 */
export interface HistorySettings {
  maxEntries: number;
  retentionDays: number;
}

/** 主配置（与 Rust `AppSettings` 对应；字段名以 Rust 序列化为准）。 */
export interface AppSettings {
  schemaVersion: number;
  logLevel: string;
  maxFileSizeMB: number;
  maxTabs: number;
  history: HistorySettings;
  saveBackupEnabled: boolean;
  showOnboarding: boolean;
}

/** 排版配置（与 Rust `TypographySettings` 对应）。 */
export interface TypographySettings {
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  contentWidth: number;
  pagePadding: number;
}

/** 阅读配置（与 Rust `ReaderSettings` 对应）。 */
export interface ReaderSettings {
  schemaVersion: number;
  theme: string;
  typography: TypographySettings;
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
  /** 打开设置窗口（已存在则聚焦；按需创建；tab 指定初始页签）。 */
  openSettings: (tab?: string) => invoke<void>('open_settings', { tab: tab ?? null }),
  /** 取走设置窗口待打开页签（读取即清空；无待办返回 null）。 */
  takeSettingsTab: () => invoke<string | null>('take_settings_tab'),
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
  findInEdit: (tabId: number, query: string, caseSensitive: boolean, from: [number, number] | null) =>
    invoke<FindHit | null>('find_in_edit', { tabId, query, caseSensitive, from }),
  /** 替换一次（从 from 起）并返回后续命中。 */
  replaceInEdit: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    from: [number, number] | null,
    replacement: string,
  ) =>
    invoke<ReplaceNextOutcome | null>('replace_in_edit', {
      tabId,
      query,
      caseSensitive,
      from,
      replacement,
    }),
  /** 全部替换（单撤销步）。 */
  replaceAllInEdit: (tabId: number, query: string, caseSensitive: boolean, replacement: string) =>
    invoke<ReplaceAllOutcome>('replace_all_in_edit', { tabId, query, caseSensitive, replacement }),
  /** 配置快照（快捷键等；后端为唯一真源）。 */
  getSettings: () => invoke<SettingsSnapshot>('get_settings'),
  /** 保存配置（返回保存后的快照）。 */
  saveSettings: (request: SettingsSaveRequest) =>
    invoke<SettingsSnapshot>('save_settings', { request }),
  /** 默认快捷键表（设置界面「恢复默认」用）。 */
  getDefaultShortcuts: () => invoke<Record<string, string>>('get_default_shortcuts'),
  /** 历史记录（去重剪枝后、时间倒序）。 */
  getHistory: () => invoke<HistoryEntry[]>('get_history'),
  /** 删除单条历史（以文件路径为键；返回更新后的列表）。 */
  removeHistory: (filePath: string) => invoke<HistoryEntry[]>('remove_history', { filePath }),
  /** 清空历史记录。 */
  clearHistory: () => invoke<void>('clear_history'),
  /** 读取会话（窗口/标签/滚动；无会话返回默认值）。 */
  getSession: () => invoke<SessionState>('get_session'),
  /** 保存会话（返回归一化后的结果）。 */
  saveSession: (session: SessionState) => invoke<SessionState>('save_session', { session }),
};
