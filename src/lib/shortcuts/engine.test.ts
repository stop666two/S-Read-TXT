// 快捷键引擎匹配逻辑单测（src/lib/shortcuts/engine.test.ts）

import { describe, expect, it } from 'vitest';

import { actionForCombo, decideShortcut, fixedTabIndex, type ShortcutContext } from './engine';
import type { ShortcutMap } from './types';

const NEUTRAL: ShortcutContext = {
  editorContext: false,
  modalOpen: false,
  defaultPrevented: false,
  composing: false,
};

const bindings: ShortcutMap = {
  openFile: 'Ctrl+O',
  closeTab: 'Ctrl+W',
  pageDown: 'PgDn',
  pageUp: 'PgUp',
  firstLine: 'Home',
  lastLine: 'End',
  nextTab: 'Ctrl+Tab',
  prevTab: 'Ctrl+Shift+Tab',
  fullscreen: 'F11',
};

describe('decideShortcut', () => {
  it('绑定命中：返回动作', () => {
    expect(decideShortcut('Ctrl+O', bindings, NEUTRAL)).toEqual({ kind: 'action', action: 'openFile' });
    expect(decideShortcut('Ctrl+Shift+Tab', bindings, NEUTRAL)).toEqual({
      kind: 'action',
      action: 'prevTab',
    });
  });

  it('未命中绑定：Ctrl+1~9 返回固定跳转（0 基）', () => {
    expect(decideShortcut('Ctrl+3', bindings, NEUTRAL)).toEqual({ kind: 'fixedTab', index: 2 });
  });

  it('无任何匹配返回 null', () => {
    expect(decideShortcut('Ctrl+Q', bindings, NEUTRAL)).toBeNull();
    expect(decideShortcut(null, bindings, NEUTRAL)).toBeNull();
  });

  it('弹窗打开：动作与固定跳转全部挂起', () => {
    const ctx = { ...NEUTRAL, modalOpen: true };
    expect(decideShortcut('Ctrl+W', bindings, ctx)).toBeNull();
    expect(decideShortcut('Ctrl+2', bindings, ctx)).toBeNull();
  });

  it('编辑上下文：翻页/首尾让位给编辑器，其他动作仍可用', () => {
    const ctx = { ...NEUTRAL, editorContext: true };
    expect(decideShortcut('PgDn', bindings, ctx)).toBeNull();
    expect(decideShortcut('PgUp', bindings, ctx)).toBeNull();
    expect(decideShortcut('Home', bindings, ctx)).toBeNull();
    expect(decideShortcut('End', bindings, ctx)).toBeNull();
    expect(decideShortcut('Ctrl+W', bindings, ctx)).toEqual({ kind: 'action', action: 'closeTab' });
    expect(decideShortcut('Ctrl+2', bindings, ctx)).toEqual({ kind: 'fixedTab', index: 1 });
  });

  it('默认已消费或 IME 组合中：忽略', () => {
    expect(decideShortcut('Ctrl+W', bindings, { ...NEUTRAL, defaultPrevented: true })).toBeNull();
    expect(decideShortcut('Ctrl+W', bindings, { ...NEUTRAL, composing: true })).toBeNull();
  });

  it('未绑定动作的裸键（无修饰）不触发固定跳转', () => {
    expect(decideShortcut('A', bindings, NEUTRAL)).toBeNull();
    expect(decideShortcut('F5', bindings, NEUTRAL)).toBeNull();
  });
});

describe('actionForCombo', () => {
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
