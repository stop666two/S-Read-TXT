// 按键规范化与录制校验单测（src/lib/shortcuts/keys.test.ts）

import { describe, expect, it } from 'vitest';

import { comboFromEvent, comboFromParts, validateRecorded } from './keys';

describe('comboFromParts', () => {
  it('组装修饰键组合（修饰键顺序固定 Ctrl→Shift→Alt）', () => {
    expect(comboFromParts({ key: 'Tab', ctrl: true, shift: true, alt: false })).toBe('Ctrl+Shift+Tab');
    expect(comboFromParts({ key: 's', ctrl: true, shift: true, alt: false })).toBe('Ctrl+Shift+S');
    expect(comboFromParts({ key: 'o', ctrl: false, shift: false, alt: true })).toBe('Alt+O');
  });

  it('规范化特殊键名（PageDown→PgDn 等）与功能键', () => {
    expect(comboFromParts({ key: 'PageDown', ctrl: false, shift: false, alt: false })).toBe('PgDn');
    expect(comboFromParts({ key: 'PageUp', ctrl: false, shift: false, alt: false })).toBe('PgUp');
    expect(comboFromParts({ key: 'Home', ctrl: false, shift: false, alt: false })).toBe('Home');
    expect(comboFromParts({ key: 'F11', ctrl: false, shift: false, alt: false })).toBe('F11');
    expect(comboFromParts({ key: ' ', ctrl: true, shift: false, alt: false })).toBe('Ctrl+Space');
  });

  it('修饰键单独按下或未知键名返回 null', () => {
    expect(comboFromParts({ key: 'Control', ctrl: true, shift: false, alt: false })).toBeNull();
    expect(comboFromParts({ key: 'Shift', ctrl: false, shift: true, alt: false })).toBeNull();
    expect(comboFromParts({ key: 'Dead', ctrl: false, shift: false, alt: false })).toBeNull();
    expect(comboFromParts({ key: '', ctrl: false, shift: false, alt: false })).toBeNull();
  });

  it('comboFromEvent：Meta（macOS）归一为 Ctrl', () => {
    const event = { key: 'k', ctrlKey: false, metaKey: true, shiftKey: false, altKey: false };
    expect(comboFromEvent(event as KeyboardEvent)).toBe('Ctrl+K');
  });
});

describe('validateRecorded', () => {
  const effective = { openFile: 'Ctrl+O', closeTab: 'Ctrl+W', firstLine: 'Home' };

  it('合法组合：带修饰键、功能键单键均通过', () => {
    expect(validateRecorded('Ctrl+Q', 'closeTab', effective)).toBeNull();
    expect(validateRecorded('F5', 'openFile', effective)).toBeNull();
    expect(validateRecorded('PgDn', 'pageDown', effective)).toBeNull();
  });

  it('裸字母/数字需要修饰键', () => {
    expect(validateRecorded('A', 'openFile', effective)).toBe('needsModifier');
    expect(validateRecorded('1', 'openFile', effective)).toBe('needsModifier');
    expect(validateRecorded('Space', 'openFile', effective)).toBe('needsModifier');
  });

  it('Ctrl+1~9 为固定键，不可占用', () => {
    expect(validateRecorded('Ctrl+1', 'openFile', effective)).toBe('reserved');
  });

  it('与其他动作重复（大小写不敏感）被拒绝', () => {
    expect(validateRecorded('ctrl+w', 'openFile', effective)).toBe('duplicate');
    expect(validateRecorded('HOME', 'lastLine', effective)).toBe('duplicate');
    // 与自己相同不算冲突（允许重复录制同一组合）
    expect(validateRecorded('Ctrl+W', 'closeTab', effective)).toBeNull();
  });

  it('空组合返回 empty', () => {
    expect(validateRecorded(null, 'openFile', effective)).toBe('empty');
  });
});
