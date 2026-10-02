// 会话采集与应用（src/lib/session.ts）
// 采集：窗口几何 + 标签表（路径/编码覆盖/滚动行/编辑态）+ 活动标签索引。
// 应用：启动时恢复窗口几何（在显示之前）；标签恢复在 App 中经 IPC 逐个打开。

import { PhysicalPosition, PhysicalSize } from '@tauri-apps/api/dpi';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { ipc, type SessionState, type WindowState } from './ipc';
import { scrollMemory } from './reader/scroll-memory';
import { tabs } from './state/tabs.svelte';

/** 默认窗口几何（与 Rust 侧 DEFAULT_WINDOW 保持一致：1100×760） */
const DEFAULT_WINDOW: WindowState = { x: null, y: null, width: 1100, height: 760, maximized: false };

/** 采集当前会话快照（窗口查询失败时使用默认几何，不影响保存） */
export async function collectSession(): Promise<SessionState> {
  const window_ = getCurrentWindow();
  let windowState: WindowState = { ...DEFAULT_WINDOW };
  try {
    if (await window_.isMaximized()) {
      windowState = { ...DEFAULT_WINDOW, maximized: true };
    } else {
      const position = await window_.outerPosition();
      const size = await window_.innerSize();
      windowState = {
        x: position.x,
        y: position.y,
        width: Math.round(size.width),
        height: Math.round(size.height),
        maximized: false,
      };
    }
  } catch {
    // 窗口查询失败：保留默认几何（会话其余字段仍有效）
  }
  const activeIndex = tabs.tabs.findIndex((tab) => tab.tabId === tabs.activeId);
  return {
    schemaVersion: 1,
    window: windowState,
    activeTabIndex: activeIndex >= 0 ? activeIndex : 0,
    tabs: tabs.tabs.map((tab) => ({
      path: tab.path,
      encoding: tab.encodingOverride ?? null,
      scrollRow: scrollMemory.get(tab.tabId) ?? 0,
      editMode: tab.editing,
    })),
  };
}

/** 立即保存会话（静默：保存失败不阻塞退出流程；错误由后端日志记录） */
export async function saveSessionNow(): Promise<void> {
  try {
    await ipc.saveSession(await collectSession());
  } catch {
    // 忽略：会话保存属尽力而为（退出流程优先）
  }
}

/** 应用窗口几何（启动、显示之前调用；失败保持默认） */
export async function applyWindowState(state: WindowState): Promise<void> {
  const window_ = getCurrentWindow();
  try {
    if (state.maximized) {
      await window_.maximize();
      return;
    }
    if (state.x !== null && state.y !== null) {
      await window_.setPosition(new PhysicalPosition(state.x, state.y));
    }
    await window_.setSize(new PhysicalSize(state.width, state.height));
  } catch {
    // 忽略：窗口恢复失败时使用默认几何
  }
}
