import { describe, expect, it } from 'vitest';
import { filterCommands, nextIndex, type PaletteCommand } from './palette';
import type { MessageKey } from '../i18n/zh-CN';

const labels: Partial<Record<MessageKey, string>> = {
  'menu.file.open': '打开文件…',
  'menu.file.settings': '设置…',
  'menu.edit.undo': '撤销',
  'menu.tools': '工具',
  'menu.file': '文件',
  'menu.edit': '编辑',
};

const translate = (key: MessageKey): string => labels[key] ?? key;

const command = (id: string, titleKey: MessageKey, groupKey: MessageKey): PaletteCommand => ({
  id,
  titleKey,
  groupKey,
  shortcutHint: '',
  enabled: true,
  run: () => {},
});

const commands: PaletteCommand[] = [
  command('file.open', 'menu.file.open', 'menu.file'),
  command('file.settings', 'menu.file.settings', 'menu.file'),
  command('edit.undo', 'menu.edit.undo', 'menu.edit'),
  command('tools.split', 'menu.file.open', 'menu.tools'),
];

describe('filterCommands', () => {
  it('空查询返回全部（保持原顺序）', () => {
    expect(filterCommands(commands, '  ', translate).map((c) => c.id)).toEqual([
      'file.open',
      'file.settings',
      'edit.undo',
      'tools.split',
    ]);
  });

  it('按标题检索且大小写不敏感', () => {
    expect(filterCommands(commands, '撤销', translate).map((c) => c.id)).toEqual(['edit.undo']);
    expect(filterCommands(commands, 'EDIT.UNDO', translate).map((c) => c.id)).toEqual(['edit.undo']);
  });

  it('多词需全部命中（可跨标题与分组）', () => {
    expect(filterCommands(commands, '打开 工具', translate).map((c) => c.id)).toEqual(['tools.split']);
    expect(filterCommands(commands, '文件 设置', translate).map((c) => c.id)).toEqual(['file.settings']);
  });

  it('无命中返回空数组', () => {
    expect(filterCommands(commands, '不存在xyz', translate)).toEqual([]);
  });
});

describe('nextIndex', () => {
  it('递增递减并环绕', () => {
    expect(nextIndex(0, 3, 1)).toBe(1);
    expect(nextIndex(2, 3, 1)).toBe(0);
    expect(nextIndex(0, 3, -1)).toBe(2);
  });

  it('空列表返回 -1；初始 -1 从首项开始', () => {
    expect(nextIndex(0, 0, 1)).toBe(-1);
    expect(nextIndex(-1, 3, 1)).toBe(1);
  });
});
