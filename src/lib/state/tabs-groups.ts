// 栏位分组纯函数：分组的定位与视图覆盖保持无副作用，便于单测与复用。

import type { TabInfo, TabsView } from '../ipc';

/** 单栏标签组（标签集合 + 活动标签）。 */
export interface GroupState {
  tabs: TabInfo[];
  activeId: number | null;
}

/** 空组常量：只读语义，调用方不得原地修改。 */
export const EMPTY_GROUP: GroupState = { tabs: [], activeId: null };

/** 定位标签所属栏位键（未找到返回 null）。 */
export function locatePane(groups: Record<string, GroupState>, tabId: number): string | null {
  for (const [pane, group] of Object.entries(groups)) {
    if (group.tabs.some((tab) => tab.tabId === tabId)) return pane;
  }
  return null;
}

/** 生成「栏位覆盖」后的新分组表（仅替换目标键）。 */
export function withView(
  groups: Record<string, GroupState>,
  pane: string,
  view: TabsView,
): Record<string, GroupState> {
  return { ...groups, [pane]: { tabs: view.tabs, activeId: view.activeTabId } };
}

/** 生成「单标签更新」后的新分组表（标签不属于任何组时原样返回）。 */
export function withTab(
  groups: Record<string, GroupState>,
  info: TabInfo,
): Record<string, GroupState> {
  const pane = locatePane(groups, info.tabId);
  if (pane === null) return groups;
  const group = groups[pane];
  return {
    ...groups,
    [pane]: {
      ...group,
      tabs: group.tabs.map((tab) => (tab.tabId === info.tabId ? info : tab)),
    },
  };
}
