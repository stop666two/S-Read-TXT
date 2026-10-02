// 按键规范化与录制校验（src/lib/shortcuts/keys.ts）
// 纯函数实现（不接触 DOM/窗口），便于单元测试与在设置窗口中复用。
// 组合键字符串格式与 Rust 侧默认表一致：修饰键按 Ctrl+Shift+Alt 顺序，主键大写。

/** 浏览器 `KeyboardEvent.key` → 规范主键名（与默认表字面量一致）。 */
const KEY_ALIASES: Record<string, string> = {
  PageDown: 'PgDn',
  PageUp: 'PgUp',
  ' ': 'Space',
  Escape: 'Esc',
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
};

/** 仅修饰键/锁定键的按键名（单独按下不构成组合键）。 */
const MODIFIER_KEYS = new Set(['Control', 'Shift', 'Alt', 'Meta', 'CapsLock', 'NumLock', 'ScrollLock']);

/** 拆分后的按键信息（便于测试直接构造对象，无需真实 KeyboardEvent）。 */
export interface KeyParts {
  key: string;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
}

/**
 * 组装规范组合键字符串。
 * 返回 null 表示该按键不可绑定（修饰键本身、无法识别键、空键）。
 *
 * 示例：`{key:'Tab',ctrl:true,shift:true,alt:false}` → `"Ctrl+Shift+Tab"`；
 *       `{key:'F11',…}` → `"F11"`；`{key:'o',ctrl:true,…}` → `"Ctrl+O"`。
 */
export function comboFromParts(parts: KeyParts): string | null {
  const raw = parts.key;
  if (!raw || MODIFIER_KEYS.has(raw)) return null;
  // 别名优先于单字符大写：`' '`（空格）必须映射为 `Space` 而非原字符
  const base = KEY_ALIASES[raw] ?? (raw.length === 1 ? raw.toUpperCase() : raw);
  // 多字符键名必须是已知可用的字面量（防把 "Dead"/"Process" 等怪键写入配置）
  if (base.length > 1 && !KNOWN_NAMED_KEYS.has(base)) return null;
  const combo: string[] = [];
  if (parts.ctrl) combo.push('Ctrl');
  if (parts.shift) combo.push('Shift');
  if (parts.alt) combo.push('Alt');
  combo.push(base);
  return combo.join('+');
}

/** 允许单键绑定的安全键名（无修饰键也不会干扰文本输入；大小写不敏感）。 */
const SAFE_BARE_KEYS = /^(F([1-9]|1[0-9]|2[0-4])|PgUp|PgDn|Home|End|Tab|Insert|Delete)$/i;

/** 已知多字符键名白名单（规范化结果必须命中其一）。 */
const KNOWN_NAMED_KEYS = new Set([
  'PgUp',
  'PgDn',
  'Home',
  'End',
  'Tab',
  'Insert',
  'Delete',
  'Backspace',
  'Enter',
  'Space',
  'Esc',
  'Up',
  'Down',
  'Left',
  'Right',
  ...Array.from({ length: 24 }, (_, index) => `F${index + 1}`),
]);

/** 由 KeyboardEvent 组装组合键（`Ctrl` 与 macOS `Meta` 归一为 `Ctrl`）。 */
export function comboFromEvent(event: KeyboardEvent): string | null {
  return comboFromParts({
    key: event.key,
    ctrl: event.ctrlKey || event.metaKey,
    shift: event.shiftKey,
    alt: event.altKey,
  });
}

/** 录制校验失败原因。 */
export type RecordError = 'empty' | 'needsModifier' | 'reserved' | 'duplicate';

/** 校验失败原因 → 用户可读提示。 */
export const RECORD_ERROR_MESSAGES: Record<RecordError, string> = {
  empty: '未识别到有效按键，请重试',
  needsModifier: '该组合会干扰正常输入，请配合 Ctrl / Shift / Alt 或改用功能键',
  reserved: 'Ctrl+1~9 是固定的标签跳转键，不可占用',
  duplicate: '该组合已被其他动作使用',
};

/**
 * 录制结果校验（设置窗口录制时调用）。
 *
 * 参数：
 * - `combo`：录制出的组合键；`action`：目标动作；
 * - `effective`：当前生效绑定表（含所有动作）。
 * 返回：null 表示合法；否则为原因码。
 */
export function validateRecorded(
  combo: string | null,
  action: string,
  effective: Record<string, string>,
): RecordError | null {
  if (!combo) return 'empty';
  const upper = combo.toUpperCase();
  if (/^CTRL\+[1-9]$/.test(upper)) return 'reserved';
  const hasModifier =
    upper.startsWith('CTRL+') || upper.startsWith('SHIFT+') || upper.startsWith('ALT+');
  const base = combo.split('+').pop() ?? '';
  if (!hasModifier && !SAFE_BARE_KEYS.test(base)) return 'needsModifier';
  for (const [other, bound] of Object.entries(effective)) {
    if (other !== action && bound.toUpperCase() === upper) return 'duplicate';
  }
  return null;
}
