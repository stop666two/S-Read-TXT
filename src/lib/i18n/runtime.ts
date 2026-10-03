// i18n 运行时（纯模块，无 Svelte 依赖）：
// 供非组件模块（ipc.ts、快捷键校验等）与单元测试环境调用；
// 组件请使用响应式入口 `index.svelte.ts` 的 `t`（locale 变化触发重渲染）。

import { en } from './en';
import { zhCN, type MessageKey } from './zh-CN';
import { formatMessage, pickMessage } from './translate';

/** 运行时语言（由 index.svelte.ts 的 setLocale 同步；默认简体中文）。 */
let currentLocale: 'zh-CN' | 'en' = 'zh-CN';

/** 同步运行时语言（仅 index.svelte.ts 调用；测试可直接调用以切换断言语言）。 */
export function setRuntimeLocale(locale: 'zh-CN' | 'en'): void {
  currentLocale = locale;
}

/** 翻译（非响应式）：读取运行时语言包并做命名参数插值。 */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  const catalog = currentLocale === 'en' ? en : zhCN;
  return formatMessage(pickMessage(catalog, key), params);
}
