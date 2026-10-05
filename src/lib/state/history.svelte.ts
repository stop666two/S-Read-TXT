// 历史记录共享状态（面板 / 菜单最近打开 / 进度回写）。
// 职责：全量历史载入与变更（删除/清空）；从历史打开并恢复阅读进度；
// 关闭标签/退出前回写阅读进度（尽力而为，不阻塞关闭流程）。

import { describeIpcError, ipc, toIpcError, type HistoryEntry, type TabInfo } from '../ipc';

import { scrollMemory } from '../reader/scroll-memory';
import { tabs } from './tabs.svelte';
import { toasts } from './toasts.svelte';

class HistoryStore {
  /** 全量历史（后端已按最近打开时间倒序、去重、剪枝） */
  entries = $state<HistoryEntry[]>([]);

  /** 载入全部历史（面板打开 / 打开文件后 / 启动时调用）。 */
  async load(): Promise<void> {
    try {
      this.entries = await ipc.getHistory();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 删除单条（后端返回更新后的列表）。 */
  async remove(path: string): Promise<void> {
    try {
      this.entries = await ipc.removeHistory(path);
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 清空全部（含阅读进度，不可恢复；调用方负责二次确认）。 */
  async clear(): Promise<void> {
    try {
      await ipc.clearHistory();
      this.entries = [];
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 从历史打开文件并恢复阅读进度。
   *
   *  实现要点：先取回 TabInfo（打开后该标签会被激活），再 seed 滚动记忆——
   *  之后 `applyView` 触发的 ReaderView 挂载/切换才会读到恢复行；
   *  文件已在当前活动标签打开时不重复跳转（该场景视觉上无变化，可接受）。 */
  async openEntry(entry: HistoryEntry): Promise<void> {
    try {
      const info = await ipc.openFile(entry.path);
      if (entry.lastRow > 0) {
        scrollMemory.seed(info.tabId, entry.lastRow);
      }
      tabs.applyView(await ipc.listTabs());
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 回写单个标签的阅读进度（关闭标签前调用；失败静默——尽力而为）。 */
  flushTab(tab: TabInfo): void {
    const row = scrollMemory.get(tab.tabId) ?? 0;
    if (row <= 0) return;
    const percent = tab.rowsTotal > 0 ? Math.min(100, (row / tab.rowsTotal) * 100) : 0;
    void ipc.updateHistoryProgress(tab.path, row, percent).catch(() => {
      // 进度回写失败不影响关闭流程
    });
  }

  /** 回写全部打开标签的阅读进度（退出前调用）。 */
  flushAll(list: TabInfo[]): void {
    for (const tab of list) this.flushTab(tab);
  }
}

/** 全局单例。 */
export const historyStore = new HistoryStore();
