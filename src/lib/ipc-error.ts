// IPC 错误码 → 语言包键映射（后端 message 仅作日志/排障，界面一律按当前语言取文案）。
// 独立模块便于单测（不依赖 Tauri 运行时）。

import type { MessageKey } from './i18n/zh-CN';
import { t } from './i18n/runtime';

/** 错误载荷最小形状（与 Rust `IpcError` 序列化一致）。 */
export interface IpcErrorPayload {
  code: string;
  message: string;
}

/** 稳定错误码 → 固定文案键（覆盖 Rust `ipc_error` 全部错误码与 diff 的 DIFF_TOO_COMPLEX）。 */
const ERROR_MESSAGE_KEYS: Record<string, MessageKey> = {
  FILE_NOT_FOUND: 'error.fileNotFound',
  FILE_TOO_LARGE: 'error.fileTooLarge',
  MAX_TABS: 'error.maxTabs',
  TAB_NOT_FOUND: 'error.tabNotFound',
  INVALID_COLOR: 'error.invalidColor',
  INVALID_ENCODING: 'error.invalidEncoding',
  IO: 'error.io',
  CONFIG_SAVE: 'error.configSave',
  SETTINGS_EXPORT: 'error.settingsExport',
  SETTINGS_IMPORT: 'error.settingsImport',
  SETTINGS_RESET: 'error.settingsReset',
  INVALID_SCOPE: 'error.invalidScope',
  MIGRATE_FAILED: 'error.migrateFailed',
  HISTORY_SAVE: 'error.historySave',
  SESSION_SAVE: 'error.sessionSave',
  INVALID_POSITION: 'error.invalidPosition',
  FILE_CONFLICT: 'error.fileConflict',
  ENCODING_UNREPRESENTABLE: 'error.encodingUnrepresentable',
  NOT_EDITING: 'error.notEditing',
  EDIT_DIRTY: 'error.editDirty',
  FILE_READ_ONLY: 'error.fileReadOnly',
  BATCH_INVALID: 'error.batchInvalid',
  FILTER_INVALID: 'error.filterInvalid',
  LINE_OP_INVALID: 'error.lineOpInvalid',
  QUERY_TOO_BROAD: 'error.queryTooBroad',
  INVALID_REGEX: 'error.invalidRegex',
  SEARCH_STALE: 'error.searchStale',
  REGEX_TIMEOUT: 'error.regexTimeout',
  EOL_CONVERT_TOO_LARGE: 'error.eolConvertTooLarge',
  INVALID_EOL: 'error.invalidEol',
  MULTIFILE_DISABLED: 'error.multifileDisabled',
  OUTLINE_INVALID: 'error.outlineInvalid',
  SNAPSHOT_INVALID: 'error.snapshotInvalid',
  UNTITLED_NEEDS_PATH: 'error.untitledNeedsPath',
  EXPORT_TOO_LARGE: 'error.exportTooLarge',
  PRINT_TOO_LARGE: 'error.printTooLarge',
  FONT_UNSUPPORTED: 'error.fontUnsupported',
  FONT_TOO_LARGE: 'error.fontTooLarge',
  FONT_NOT_FOUND: 'error.fontNotFound',
  FONT_INVALID_NAME: 'error.fontInvalidName',
  BACKGROUND_INVALID: 'error.backgroundInvalid',
  THEME_INVALID: 'error.themeInvalid',
  SPLIT_INVALID: 'error.splitInvalid',
  RENAME_INVALID: 'error.renameInvalid',
  DIFF_TOO_COMPLEX: 'error.diffTooComplex',
  INTERNAL: 'error.internal',
};

/** 错误码是否已有固定文案（探测/测试用）。 */
export function hasFixedErrorMessage(code: string): boolean {
  return code in ERROR_MESSAGE_KEYS;
}

/** 把 IPC 错误描述为当前语言的可展示文案（未知码回退「操作失败（码）」）。 */
export function describeIpcError(error: IpcErrorPayload): string {
  const key = ERROR_MESSAGE_KEYS[error.code];
  return key ? t(key) : t('error.unknown', { code: error.code });
}
