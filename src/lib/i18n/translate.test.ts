// i18n 纯函数与语言包单测（响应式层由 E2E/组件测试覆盖）。

import { describe, expect, it } from 'vitest';

import { en } from './en';
import { formatMessage, pickMessage } from './translate';
import { zhCN } from './zh-CN';

describe('i18n', () => {
  it('en 与 zh-CN 键集合完全一致（完备性运行期兜底）', () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(zhCN).sort());
  });

  it('插值替换命名参数（字符串与数字）', () => {
    expect(formatMessage('阅读 {percent}%', { percent: 42 })).toBe('阅读 42%');
    expect(formatMessage('编码：{value}', { value: 'GB18030' })).toBe('编码：GB18030');
  });

  it('缺失参数保留占位符（防御）', () => {
    expect(formatMessage('阅读 {percent}%', {})).toBe('阅读 {percent}%');
    expect(formatMessage('无参数模板')).toBe('无参数模板');
  });

  it('未知键回退为键名（运行期防御；类型层已禁止）', () => {
    expect(pickMessage(zhCN, 'no.such.key' as never)).toBe('no.such.key');
  });

  it('语言包取值均非空', () => {
    for (const [key, value] of Object.entries(zhCN)) {
      expect(value.length, key).toBeGreaterThan(0);
    }
    for (const [key, value] of Object.entries(en)) {
      expect(value.length, key).toBeGreaterThan(0);
    }
  });
});
