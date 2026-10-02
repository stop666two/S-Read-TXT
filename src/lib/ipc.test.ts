// ipc.ts 错误策略单元测试（Vitest；不触碰 Tauri 运行时）。

import { describe, expect, it } from 'vitest';

import { describeIpcError, toIpcError } from './ipc';

describe('toIpcError', () => {
  it('透传后端 IpcError 载荷', () => {
    const payload = { code: 'FILE_TOO_LARGE', message: '文件过大：101.0 MB，超过上限 100 MB' };
    expect(toIpcError(payload)).toEqual(payload);
  });

  it('未知异常兜底为 UNKNOWN', () => {
    expect(toIpcError(new Error('boom')).code).toBe('UNKNOWN');
    expect(toIpcError('raw-string').code).toBe('UNKNOWN');
    expect(toIpcError(null).code).toBe('UNKNOWN');
    expect(toIpcError({ code: 42 }).code).toBe('UNKNOWN');
  });
});

describe('describeIpcError', () => {
  it('文件过大使用需求给定的逐字提示', () => {
    expect(describeIpcError({ code: 'FILE_TOO_LARGE', message: '任意详情' })).toBe(
      '很抱歉，文件过大无法打开，可以在设置里面调整。',
    );
  });

  it('标签上限给出可操作提示', () => {
    expect(describeIpcError({ code: 'MAX_TABS', message: '任意详情' })).toContain('关闭部分标签');
  });

  it('其余错误透出后端消息', () => {
    expect(describeIpcError({ code: 'IO', message: '读取文件失败：拒绝访问' })).toBe(
      '读取文件失败：拒绝访问',
    );
  });
});
