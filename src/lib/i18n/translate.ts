// i18n 纯函数层（无 Svelte 依赖，可直接在 vitest 中单测）。

/** 命名参数插值：`{name}` → 对应值；缺失参数保留占位符（防御未知调用方）。 */
export function formatMessage(
  template: string,
  params?: Record<string, string | number>,
): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}

/** 按键取值：键缺失时回退键名本身（类型层已禁止，运行期防御）。 */
export function pickMessage(catalog: Record<string, string>, key: string): string {
  return catalog[key] ?? key;
}
