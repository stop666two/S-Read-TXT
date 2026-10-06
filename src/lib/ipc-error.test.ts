import { beforeEach, describe, expect, it } from 'vitest';
import { describeIpcError, hasFixedErrorMessage } from './ipc-error';
import { setRuntimeLocale } from './i18n/runtime';

// 与 Rust `ipc_error.rs` 全部稳定错误码 + diff 的 DIFF_TOO_COMPLEX 对齐；
// 后端新增错误码时必须同步映射表（本测试会随即失败提醒）。
const BACKEND_CODES = [
  'FILE_NOT_FOUND',
  'FILE_TOO_LARGE',
  'MAX_TABS',
  'TAB_NOT_FOUND',
  'INVALID_COLOR',
  'INVALID_ENCODING',
  'IO',
  'CONFIG_SAVE',
  'SETTINGS_EXPORT',
  'SETTINGS_IMPORT',
  'SETTINGS_RESET',
  'INVALID_SCOPE',
  'MIGRATE_FAILED',
  'HISTORY_SAVE',
  'SESSION_SAVE',
  'INVALID_POSITION',
  'FILE_CONFLICT',
  'ENCODING_UNREPRESENTABLE',
  'NOT_EDITING',
  'EDIT_DIRTY',
  'FILE_READ_ONLY',
  'BATCH_INVALID',
  'FILTER_INVALID',
  'LINE_OP_INVALID',
  'QUERY_TOO_BROAD',
  'INVALID_REGEX',
  'SEARCH_STALE',
  'REGEX_TIMEOUT',
  'EOL_CONVERT_TOO_LARGE',
  'INVALID_EOL',
  'MULTIFILE_DISABLED',
  'OUTLINE_INVALID',
  'SNAPSHOT_INVALID',
  'UNTITLED_NEEDS_PATH',
  'EXPORT_TOO_LARGE',
  'PRINT_TOO_LARGE',
  'FONT_UNSUPPORTED',
  'FONT_TOO_LARGE',
  'FONT_NOT_FOUND',
  'FONT_INVALID_NAME',
  'BACKGROUND_INVALID',
  'THEME_INVALID',
  'SPLIT_INVALID',
  'RENAME_INVALID',
  'DIFF_TOO_COMPLEX',
  'INTERNAL',
];

describe('ipc-error', () => {
  beforeEach(() => setRuntimeLocale('zh-CN'));

  it('覆盖全部后端错误码', () => {
    for (const code of BACKEND_CODES) {
      expect(hasFixedErrorMessage(code), `错误码 ${code} 未映射文案`).toBe(true);
    }
  });

  it('按当前语言返回固定文案（切换语言即时生效）', () => {
    const zh = describeIpcError({ code: 'EXPORT_TOO_LARGE', message: '内部中文消息' });
    expect(zh).toContain('导出');
    expect(zh).not.toContain('内部中文消息');
    setRuntimeLocale('en');
    const en = describeIpcError({ code: 'EXPORT_TOO_LARGE', message: '内部中文消息' });
    expect(en.toLowerCase()).toContain('export');
    expect(en).not.toContain('内部中文消息');
  });

  it('未知错误码回退「操作失败（码）」且不透出后端原文', () => {
    const text = describeIpcError({ code: 'SOMETHING_NEW', message: '后端中文详情' });
    expect(text).toContain('SOMETHING_NEW');
    expect(text).not.toContain('后端中文详情');
  });
});
