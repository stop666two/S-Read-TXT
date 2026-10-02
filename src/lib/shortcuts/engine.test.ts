// 快捷键引擎匹配逻辑单测（src/lib/shortcuts/engine.test.ts）

import { describe, expect, it } from 'vitest';

import { actionForCombo, fixedTabIndex } from './engine';
import type { ShortcutMap } from './types';

describe('actionForCombo', () => {
  const bindings: ShortcutMap = {
    openFile: 'Ctrl+O',
    closeTab: 'Ctrl+W',
    nextTab: 'Ctrl+Tab',
    prevTab: 'Ctrl+Shift+Tab',
    fullscreen: 'F11',
  };

  it('精确匹配动作', () => {
    expect(actionForCombo(bindings, 'Ctrl+O')).toBe('openFile');
    expect(actionForCombo(bindings, 'Ctrl+Shift+Tab')).toBe('prevTab');
    expect(actionForCombo(bindings, 'F11')).toBe('fullscreen');
  });

  it('大小写不敏感', () => {
    expect(actionForCombo(bindings, 'ctrl+o')).toBe('openFile');
  });

  it('无匹配返回 null', () => {
    expect(actionForCombo(bindings, 'Ctrl+Q')).toBeNull();
    expect(actionForCombo({}, 'Ctrl+O')).toBeNull();
  });

  it('空绑定值被忽略（防御损坏配置）', () => {
    expect(actionForCombo({ openFile: '' }, 'Ctrl+O')).toBeNull();
  });
});

describe('fixedTabIndex', () => {
  it('Ctrl+1~9 解析为 0 基下标', () => {
    expect(fixedTabIndex('Ctrl+1')).toBe(0);
    expect(fixedTabIndex('Ctrl+9')).toBe(8);
    expect(fixedTabIndex('Ctrl+0')).toBeNull();
    expect(fixedTabIndex('Ctrl+Shift+1')).toBeNull();
    expect(fixedTabIndex('Alt+1')).toBeNull();
  });
});
