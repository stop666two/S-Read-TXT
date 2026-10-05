// 标签存储：标签数据的唯一来源是后端（open/close/list 的返回视图），
// 前端仅维护按栏位分组的镜像与各栏当前选择；所有失败都经 Toast 显式反馈，不吞异常。

import { open as openFileDialog } from '@tauri-apps/plugin-dialog';

import { t } from '../i18n/runtime';

import { describeIpcError, ipc, toIpcError, type TabInfo, type TabsView } from '../ipc';
import { toasts } from './toasts.svelte';
import {
  EMPTY_GROUP,
  locatePane,
  withTab,
  withView,
  type GroupState,
} from './tabs-groups';

class TabStore {
  /** 各栏位分组（栏位键 → 标签集合与活动标签；栏位键格式 `窗口label#序号`）。 */
  groups = $state<Record<string, GroupState>>({});
  /** 当前激活栏位键（快捷键与状态栏的作用域；由 App 在栏位获得焦点时设置）。 */
  activePane = $state('');

  /** 当前激活栏的分组（未持有视图时为空组）。 */
  get group(): GroupState {
    return this.groups[this.activePane] ?? EMPTY_GROUP;
  }

  /** 当前激活栏的标签列表（与后端顺序一致）。 */
  get tabs(): TabInfo[] {
    return this.group.tabs;
  }

  /** 当前激活栏的活动标签 id（null = 无）。 */
  get activeId(): number | null {
    return this.group.activeId;
  }

  /** 当前激活栏的活动标签（派生只读）。 */
  get active(): TabInfo | null {
    return this.tabs.find((tab) => tab.tabId === this.activeId) ?? null;
  }

  /** 设置激活栏位（栏位获得焦点时由 App 调用）。 */
  setActivePane(pane: string): void {
    this.activePane = pane;
  }

  /** 应用后端返回的标签视图（唯一的状态写入口；按栏位键写入）。 */
  applyView(pane: string, view: TabsView): void {
    this.groups = withView(this.groups, pane, view);
  }

  /** 回读指定栏（默认激活栏）的完整视图。 */
  async refresh(pane: string = this.activePane): Promise<void> {
    if (pane === '') return;
    this.applyView(pane, await ipc.listTabs(pane));
  }

  /** 定位标签所属栏位键（未找到返回 null）。 */
  paneOf(tabId: number): string | null {
    return locatePane(this.groups, tabId);
  }

  /** 通过系统文件对话框打开（支持多选；取消不产生提示）。 */
  async openViaDialog(pane: string = this.activePane): Promise<void> {
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
      await this.openPath(path, pane);
    }
  }

  /** 打开单个路径到指定栏（默认激活栏）：打开成功后以该栏 list_tabs 视图校准状态。
   *  新打开的只读文件（超过只读阈值）给出一次性提示。 */
  async openPath(path: string, pane: string = this.activePane): Promise<void> {
    if (pane === '') return;
    const before = this.groups[pane]?.tabs ?? [];
    try {
      const info = await ipc.openFile(path, pane);
      const isNew = !before.some((tab) => tab.tabId === info.tabId);
      this.setActivePane(pane);
      this.applyView(pane, await ipc.listTabs(pane));
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
    const pane = this.paneOf(tabId) ?? this.activePane;
    if (pane === '') return;
    this.setActivePane(pane);
    const group = this.groups[pane];
    if (group) {
      this.groups = { ...this.groups, [pane]: { ...group, activeId: tabId } };
    }
    void ipc.setActiveTab(tabId).catch((error: unknown) => {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[tabs] 同步活动标签失败', payload);
      toasts.error(describeIpcError(payload));
      void this.refresh(pane).catch(() => {
        // 回读也失败时保持现状（下一次操作会再次校准）
      });
    });
  }

  /** 拖拽排序：本地立即生效（响应快），后端失败时回读视图纠正。
   *
   *  `toIndex` 语义 = 从列表移除后插入的下标（越界由后端收敛到末尾）。 */
  reorder(tabId: number, toIndex: number): void {
    const pane = this.paneOf(tabId);
    if (pane === null) return;
    const group = this.groups[pane];
    const from = group.tabs.findIndex((tab) => tab.tabId === tabId);
    if (from < 0) return;
    const next = [...group.tabs];
    const [moved] = next.splice(from, 1);
    next.splice(Math.max(0, Math.min(toIndex, next.length)), 0, moved);
    this.groups = { ...this.groups, [pane]: { ...group, tabs: next } };
    void ipc.reorderTab(tabId, toIndex).catch((error: unknown) => {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[tabs] 排序同步失败', payload);
      toasts.error(describeIpcError(payload));
      void this.refresh(pane).catch(() => {
        // 回读失败时保持现状（下一次操作会再次校准）
      });
    });
  }

  /** 关闭标签：关闭后回读其所属栏（幂等）。 */
  async close(tabId: number): Promise<void> {
    const pane = this.paneOf(tabId) ?? this.activePane;
    try {
      await ipc.closeTab(tabId);
      await this.refresh(pane);
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[tabs] 关闭失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 更新单个标签信息（如编码切换后；标签不属于任何组时不动作）。 */
  update(info: TabInfo): void {
    this.groups = withTab(this.groups, info);
  }

  /** 移除栏位分组镜像（栏位关闭后调用；活动栏被移除前应先切换）。 */
  dropGroup(pane: string): void {
    const next = { ...this.groups };
    delete next[pane];
    this.groups = next;
  }
}

/** 全局标签单例（每个窗口一个前端实例）。 */
export const tabs = new TabStore();
