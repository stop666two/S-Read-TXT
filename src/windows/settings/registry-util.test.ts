// registry-util 单元测试：路径读取与补丁构造（含嵌套与边界）。
import { describe, expect, it } from 'vitest';

import type { SettingsSnapshot } from '../../lib/ipc';
import { buildPatch, getSettingValue } from './registry-util';

/** 仅含被测路径的最小快照（其余字段与断言无关，经断言桥接为完整类型）。 */
function snapshot(): SettingsSnapshot {
  return {
    app: {
      maxTabs: 20,
      showOnboarding: true,
      history: { maxEntries: 10_000, retentionDays: 365 },
      startup: { restoreSession: true, restoreWindow: false },
    },
    reader: {
      typography: { fontSize: 16, lineHeight: 1.8 },
      statusBar: { showFileName: true },
    },
  } as unknown as SettingsSnapshot;
}

describe('getSettingValue', () => {
  it('读取顶层字段（app.maxTabs）', () => {
    expect(getSettingValue(snapshot(), 'app.maxTabs')).toBe(20);
  });

  it('读取父级字段与嵌套字段', () => {
    expect(getSettingValue(snapshot(), 'app.showOnboarding')).toBe(true);
    expect(getSettingValue(snapshot(), 'reader.typography.fontSize')).toBe(16);
    expect(getSettingValue(snapshot(), 'app.history.maxEntries')).toBe(10_000);
  });

  it('未知根段与不存在的路径返回 undefined', () => {
    expect(getSettingValue(snapshot(), 'shortcuts.bindings')).toBeUndefined();
    expect(getSettingValue(snapshot(), 'app.noSuchField')).toBeUndefined();
    expect(getSettingValue(snapshot(), 'reader.typography.noSuchField')).toBeUndefined();
  });
});

describe('buildPatch', () => {
  it('顶层字段补丁不修改原对象', () => {
    const app = snapshot().app;
    const patch = buildPatch(app, 'app.maxTabs', 30);
    expect(patch.maxTabs).toBe(30);
    expect(app.maxTabs).toBe(20);
  });

  it('嵌套路径补丁保留同级字段', () => {
    const app = snapshot().app;
    const patch = buildPatch(app, 'app.history.maxEntries', 500);
    expect(patch.history.maxEntries).toBe(500);
    expect(patch.history.retentionDays).toBe(365);
    expect(patch.startup.restoreSession).toBe(true);
  });

  it('reader 段嵌套与顶层补丁', () => {
    const reader = snapshot().reader;
    const nested = buildPatch(reader, 'reader.typography.fontSize', 22);
    expect(nested.typography.fontSize).toBe(22);
    expect(nested.typography.lineHeight).toBe(1.8);
    const top = buildPatch(reader, 'reader.statusBar', { showFileName: false });
    expect(top.statusBar.showFileName).toBe(false);
  });
});
