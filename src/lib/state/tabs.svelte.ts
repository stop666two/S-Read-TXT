// 标签存储：标签数据的唯一来源是后端（open/close/list 的返回视图），
// 前端仅维护镜像与当前选择；所有失败都经 Toast 显式反馈，不吞异常。

import { open as openFileDialog } from '@tauri-apps/plugin-dialog';

import { t } from '../i18n/runtime';

import { describeIpcError, ipc, toIpcError, type TabInfo, type TabsView } from '../ipc';
import { toasts } from './toasts.svelte';

class TabStore {
  /** 全部标签（与后端顺序一致） */
  tabs = $state<TabInfo[]>([]);
  /** 当前活动标签 id（null = 无） */
  activeId = $state<number | null>(null);

  /** 当前活动标签（派生只读）。 */
  get active(): TabInfo | null {
    return this.tabs.find((tab) => tab.tabId === this.activeId) ?? null;
  }

  /** 应用后端返回的标签视图（唯一的状态写入口）。 */
  applyView(view: TabsView): void {
    this.tabs = view.tabs;
    this.activeId = view.activeTabId;
  }

  /** 通过系统文件对话框打开（支持多选；取消不产生提示）。 */
  async openViaDialog(): Promise<void> {
    let selected: string | string[] | null;
    try {
      selected = await openFileDialog({
        multiple: true,
        filters: [
      { name: t('app.filter.text'), extensions: ['txt', 'log'] },
      { name: t('app.filter.all'), extensions: ['*'] },
        ],
      });
    } catch (error) {
      // 对话框本身失败（极少见）：同样走统一错误反馈
      toasts.error(describeIpcError(toIpcError(error)));
      return;
    }
    if (selected === null) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    for (const path of paths) {
      await this.openPath(path);
    }
  }

  /** 打开单个路径：打开成功后以 list_tabs 返回的完整视图校准状态。
   *  新打开的只读文件（超过只读阈值）给出一次性提示。 */
  async openPath(path: string): Promise<void> {
    try {
      const info = await ipc.openFile(path);
      const isNew = !this.tabs.some((tab) => tab.tabId === info.tabId);
      this.applyView(await ipc.listTabs());
      if (isNew && info.readOnly) {
        toasts.show(t('app.openReadOnlyHint'), 'warn', 5000);
      }
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[tabs] 打开失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 选择活动标签（本地即时生效 + 同步后端；失败回读视图纠正）。
   *
   *  说明：后端活动标签决定关闭回落方向与会话语义，必须保持一致；
   *  同步失败（如标签已被并发关闭）时以 list_tabs 回读校准，避免镜像漂移。 */
  select(tabId: number): void {
    this.activeId = tabId;
    void ipc.setActiveTab(tabId).catch((error: unknown) => {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[tabs] 同步活动标签失败', payload);
      toasts.error(describeIpcError(payload));
      void ipc
        .listTabs()
        .then((view) => this.applyView(view))
        .catch(() => {
          // 回读也失败时保持现状（下一次操作会再次校准）
        });
    });
  }

  /** 拖拽排序：本地立即生效（响应快），后端失败时回读视图纠正。
   *
   *  `toIndex` 语义 = 从列表移除后插入的下标（越界由后端收敛到末尾）。 */
  reorder(tabId: number, toIndex: number): void {
    const from = this.tabs.findIndex((tab) => tab.tabId === tabId);
    if (from < 0) return;
    const next = [...this.tabs];
    const [moved] = next.splice(from, 1);
    next.splice(Math.max(0, Math.min(toIndex, next.length)), 0, moved);
    this.tabs = next;
    void ipc.reorderTab(tabId, toIndex).catch((error: unknown) => {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[tabs] 排序同步失败', payload);
      toasts.error(describeIpcError(payload));
      void ipc
        .listTabs()
        .then((view) => this.applyView(view))
        .catch(() => {
          // 回读失败时保持现状（下一次操作会再次校准）
        });
    });
  }

  /** 关闭标签：以返回值重建视图（幂等）。 */
  async close(tabId: number): Promise<void> {
    try {
      this.applyView(await ipc.closeTab(tabId));
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[tabs] 关闭失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 更新单个标签信息（如编码切换后）。 */
  update(info: TabInfo): void {
    this.tabs = this.tabs.map((tab) => (tab.tabId === info.tabId ? info : tab));
  }
}

/** 全局标签单例。 */
export const tabs = new TabStore();
