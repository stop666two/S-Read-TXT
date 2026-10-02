// IPC 封装：类型化命令包装 + 错误归一（Tauri 拒绝值即后端 IpcError 载荷）。
// 约定：组件与 store 一律经本模块访问后端，不直接 import invoke，便于类型与错误策略统一。

import { invoke } from '@tauri-apps/api/core';

/** 应用信息（与 Rust commands.rs 的 AppInfo 对齐）。 */
export interface AppInfo {
  version: string;
  dataDir: string;
  dataDirOrigin: 'portable' | 'envOverride';
}

/** 单项文本行（与 Rust textfile::window::RowText 对齐）。 */
export interface RowText {
  row: number;
  text: string;
}

/** 标签信息（与 Rust app_state::TabInfo 对齐）。 */
export interface TabInfo {
  tabId: number;
  path: string;
  name: string;
  encoding: string;
  encodingOverride: string | null;
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

/** 类型化 IPC 命令集合（参数名与 Tauri 的 camelCase 约定一致）。 */
export const ipc = {
  /** 应用信息（版本 / 数据目录）。 */
  getAppInfo: () => invoke<AppInfo>('get_app_info'),
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
};
