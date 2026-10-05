import { describe, expect, it } from 'vitest';

import type { TabInfo, TabsView } from '../ipc';
import { EMPTY_GROUP, locatePane, withTab, withView, type GroupState } from './tabs-groups';

function tab(tabId: number, name = `t${tabId}`): TabInfo {
  return {
    tabId,
    owner: 'main#1',
    path: `C:/${name}.txt`,
    name,
    encoding: 'UTF-8',
    encodingOverride: null,
    editing: false,
    dirty: false,
    color: null,
    readOnly: false,
    rowsTotal: 1,
    byteLen: 1,
    eol: 'lf',
  };
}

function view(tabs: TabInfo[], activeTabId: number | null): TabsView {
  return { tabs, activeTabId };
}

describe('栏位分组纯函数', () => {
  it('locatePane 按所属组定位，未找到返回 null', () => {
    const groups: Record<string, GroupState> = {
      'main#1': { tabs: [tab(1), tab(2)], activeId: 1 },
      'main#2': { tabs: [tab(3)], activeId: 3 },
    };
    expect(locatePane(groups, 3)).toBe('main#2');
    expect(locatePane(groups, 1)).toBe('main#1');
    expect(locatePane(groups, 99)).toBeNull();
  });

  it('withView 仅替换目标键，其他组引用保持', () => {
    const groups: Record<string, GroupState> = {
      'main#1': { tabs: [tab(1)], activeId: 1 },
      'main#2': { tabs: [tab(2)], activeId: 2 },
    };
    const next = withView(groups, 'main#2', view([tab(3), tab(4)], 4));
    expect(next['main#2'].tabs.map((t) => t.tabId)).toEqual([3, 4]);
    expect(next['main#2'].activeId).toBe(4);
    expect(next['main#1']).toBe(groups['main#1']);
    expect(groups['main#2'].tabs.map((t) => t.tabId)).toEqual([2]);
  });

  it('withTab 命中时原位替换，未命中时原表返回', () => {
    const groups: Record<string, GroupState> = {
      'main#1': { tabs: [tab(1), tab(2)], activeId: 2 },
    };
    const updated = { ...tab(2), dirty: true };
    const next = withTab(groups, updated);
    expect(next['main#1'].tabs[1].dirty).toBe(true);
    expect(next['main#1'].tabs[0]).toBe(groups['main#1'].tabs[0]);
    expect(next['main#1'].activeId).toBe(2);
    const missing = { ...tab(9), dirty: true };
    expect(withTab(groups, missing)).toBe(groups);
  });

  it('空组常量不因视图覆盖被篡改', () => {
    const next = withView({ 'main#1': EMPTY_GROUP }, 'main#2', view([tab(5)], 5));
    expect(EMPTY_GROUP.tabs).toEqual([]);
    expect(EMPTY_GROUP.activeId).toBeNull();
    expect(next['main#1']).toBe(EMPTY_GROUP);
  });
});
