import { describe, expect, it } from 'vitest';

import {
  collapsed,
  clampPos,
  isCollapsed,
  moveEnd,
  moveHome,
  moveLeft,
  moveRight,
  moveVertical,
  orderedSelection,
  posCompare,
  posEquals,
  UTF16_END,
} from './caret';

describe('caret 位置模型', () => {
  it('比较与相等：先行后列', () => {
    expect(posCompare({ row: 0, utf16: 5 }, { row: 1, utf16: 0 })).toBeLessThan(0);
    expect(posCompare({ row: 1, utf16: 2 }, { row: 1, utf16: 3 })).toBeLessThan(0);
    expect(posCompare({ row: 1, utf16: 3 }, { row: 1, utf16: 3 })).toBe(0);
    expect(posEquals({ row: 2, utf16: 1 }, { row: 2, utf16: 1 })).toBe(true);
    expect(posEquals({ row: 2, utf16: 1 }, { row: 2, utf16: 2 })).toBe(false);
  });

  it('选区有序化与折叠判断', () => {
    const sel = { anchor: { row: 2, utf16: 1 }, head: { row: 0, utf16: 4 } };
    const { start, end } = orderedSelection(sel);
    expect(start).toEqual({ row: 0, utf16: 4 });
    expect(end).toEqual({ row: 2, utf16: 1 });
    expect(isCollapsed(collapsed({ row: 1, utf16: 0 }))).toBe(true);
    expect(isCollapsed(sel)).toBe(false);
  });

  it('clampPos：行号与列双向钳制', () => {
    const rowLength = (row: number): number => (row === 0 ? 3 : 5);
    expect(clampPos({ row: -1, utf16: 2 }, 2, rowLength)).toEqual({ row: 0, utf16: 2 });
    expect(clampPos({ row: 9, utf16: 2 }, 2, rowLength)).toEqual({ row: 1, utf16: 2 });
    expect(clampPos({ row: 0, utf16: 99 }, 2, rowLength)).toEqual({ row: 0, utf16: 3 });
    expect(clampPos({ row: 0, utf16: -1 }, 2, rowLength)).toEqual({ row: 0, utf16: 0 });
  });

  it('左右移动：行内、跨行与边界', () => {
    const texts = ['中a', 'bb'];
    const rowText = (row: number): string | undefined => texts[row];
    expect(moveLeft({ row: 0, utf16: 2 }, rowText)).toEqual({ row: 0, utf16: 1 });
    expect(moveLeft({ row: 0, utf16: 0 }, rowText)).toEqual({ row: 0, utf16: 0 });
    expect(moveLeft({ row: 1, utf16: 0 }, rowText)).toEqual({ row: 0, utf16: 2 });
    expect(moveRight({ row: 0, utf16: 1 }, 2, rowText)).toEqual({ row: 0, utf16: 2 });
    expect(moveRight({ row: 0, utf16: 2 }, 2, rowText)).toEqual({ row: 1, utf16: 0 });
    expect(moveRight({ row: 1, utf16: 2 }, 2, rowText)).toEqual({ row: 1, utf16: 2 });
  });

  it('左右移动：代理对不分割', () => {
    const text = 'a\u{1F600}b'; // 长度 4（a + 两个码元 + b）
    const rowText = (): string => text;
    expect(text.length).toBe(4);
    // 从 b 前（utf16=3）左移：跳到 emoji 前（1），而不是代理对中间（2）
    expect(moveLeft({ row: 0, utf16: 3 }, rowText)).toEqual({ row: 0, utf16: 1 });
    // 从 a 后（1）右移：跳过整个 emoji（3）
    expect(moveRight({ row: 0, utf16: 1 }, 1, rowText)).toEqual({ row: 0, utf16: 3 });
  });

  it('垂直移动与行首行尾', () => {
    expect(moveVertical({ row: 0, utf16: 4 }, 1, 3)).toEqual({ row: 1, utf16: 4 });
    expect(moveVertical({ row: 2, utf16: 4 }, 5, 3)).toEqual({ row: 2, utf16: 4 });
    expect(moveHome({ row: 1, utf16: 9 })).toEqual({ row: 1, utf16: 0 });
    expect(moveEnd({ row: 1, utf16: 0 })).toEqual({ row: 1, utf16: UTF16_END });
  });

  it('哨兵值经 clampPos 收敛到真实行尾', () => {
    const rowLength = (): number => 7;
    expect(clampPos({ row: 2, utf16: UTF16_END }, 3, rowLength)).toEqual({ row: 2, utf16: 7 });
    // 行号越界时同样收敛
    expect(clampPos({ row: 99, utf16: UTF16_END }, 3, rowLength)).toEqual({ row: 2, utf16: 7 });
  });
});
