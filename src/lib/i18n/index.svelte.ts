// i18n 响应式入口（Svelte runes 模块）：
// - `setLocale` 切换语言（立即生效：读取 `i18n.locale` 的组件自动重渲染）
// - `t` 翻译 + 命名参数插值（组件模板直接调用）
// 语言持久化：`settings.json` 的 `app.locale`（App.reloadSettings 时同步）。

import { en } from './en';
import { formatMessage, pickMessage } from './translate';
import { zhCN, type MessageKey } from './zh-CN';

/** 受支持的界面语言（BCP 47；与 Rust `Language` 枚举对齐）。 */
export type Locale = 'zh-CN' | 'en';

const catalogs: Record<Locale, Record<string, string>> = {
  'zh-CN': zhCN,
  en,
};

/** 当前界面语言（runes 状态：供组件依赖追踪）。 */
export const i18n = $state({ locale: 'zh-CN' as Locale });

/** 设为指定语言；未知值回退简体中文；同步 `<html lang>`（无障碍/正确断词）。 */
export function setLocale(locale: string): void {
  const next: Locale = locale === 'en' ? 'en' : 'zh-CN';
  i18n.locale = next;
  if (typeof document !== 'undefined') {
    document.documentElement.lang = next;
  }
}

/** 翻译：读取当前语言包并做命名参数插值。 */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  const catalog = catalogs[i18n.locale];
  return formatMessage(pickMessage(catalog, key), params);
}
