// 会话采集与保存（src/lib/session.ts）
// 采集：窗口几何 + 分栏布局 + 各栏标签锚点（路径/编码覆盖/滚动行/编辑态/颜色）+ 聚焦栏。
// 说明：窗口几何的「应用」在 Rust 启动层完成（src-tauri/src/main.rs，显示之前应用避免跳动）；
// 标签恢复在 App 中经 IPC 逐个打开到对应栏位（惰性索引）。

import { getCurrentWindow } from '@tauri-apps/api/window';

import { ipc, type WindowSession, type WindowState } from './ipc';
import { collectLeaves, type PaneLayout } from './layout/pane-tree';
import { scrollMemory } from './reader/scroll-memory';
import { tabs } from './state/tabs.svelte';

/** 默认窗口几何（与 Rust 侧 DEFAULT_WINDOW 保持一致：1100×760） */
const DEFAULT_WINDOW: WindowState = { x: null, y: null, width: 1100, height: 760, maximized: false };

/** 当前布局与聚焦栏（由 App 在状态变化时同步；会话采集为异步、读最新值）。 */
let currentLayout: PaneLayout | null = null;
let currentFocusedPane = '';

/** 同步布局来源（App 的 effect 调用；layout 传 `$state.snapshot` 的纯对象）。 */
export function setSessionLayout(layout: PaneLayout, focusedPane: string): void {
  currentLayout = layout;
  currentFocusedPane = focusedPane;
}

/** 采集当前窗口的会话切片（窗口查询失败时使用默认几何，不影响保存） */
export async function collectSession(): Promise<WindowSession> {
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
  const layout = currentLayout ?? { type: 'leaf' as const, pane: `${window_.label}#1` };
  const focusedPane = currentFocusedPane || `${window_.label}#1`;
  const panes = collectLeaves(layout).map((pane) => {
    const group = tabs.groups[pane];
    const list = (group?.tabs ?? []).filter((tab) => tab.untitled == null);
    const activeIndex = list.findIndex((tab) => tab.tabId === group?.activeId);
    return {
      pane,
      activeTabIndex: activeIndex >= 0 ? activeIndex : 0,
      tabs: list.map((tab) => ({
        path: tab.path,
        encoding: tab.encodingOverride ?? null,
        scrollRow: scrollMemory.get(tab.tabId) ?? 0,
        editMode: tab.editing,
        color: tab.color ?? null,
      })),
    };
  });
  return {
    label: window_.label,
    window: windowState,
    panes,
    layout,
    focusedPane,
  };
}

/** 立即保存会话切片（静默：保存失败不阻塞退出流程；错误由后端日志记录） */
export async function saveSessionNow(): Promise<void> {
  try {
    await ipc.saveSession(await collectSession());
  } catch {
    // 忽略：会话保存属尽力而为（退出流程优先）
  }
}
