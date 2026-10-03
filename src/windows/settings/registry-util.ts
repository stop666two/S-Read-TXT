// 设置注册表工具（纯函数；供设置界面与单元测试使用）。
//
// 约定：设置项 id 为「点分 JSON 路径」，根段为 `app` 或 `reader`；
// 读取与补丁构造都基于聚合快照的对应子对象，避免组件内重复实现路径拼装。

import type { SettingsSnapshot } from '../../lib/ipc';

/**
 * 读取设置项在快照中的当前值。
 *
 * 参数：
 * - `snapshot`：聚合配置快照（app + reader + shortcuts）
 * - `id`：设置项点分 id（如 `app.history.maxEntries`）
 *
 * 返回：路径存在时返回值；未知根段或路径不存在时返回 `undefined`。
 */
export function getSettingValue(snapshot: SettingsSnapshot, id: string): unknown {
  const segments = id.split('.');
  let node: unknown;
  if (segments[0] === 'app') {
    node = snapshot.app;
  } else if (segments[0] === 'reader') {
    node = snapshot.reader;
  } else {
    return undefined;
  }
  for (const segment of segments.slice(1)) {
    if (node === null || typeof node !== 'object') return undefined;
    node = (node as Record<string, unknown>)[segment];
  }
  return node;
}

/**
 * 在子对象（app 或 reader 段）的深拷贝上按路径写入新值。
 *
 * 参数：
 * - `section`：`settings.app` 或 `settings.reader`（原始对象不被修改）
 * - `id`：设置项点分 id（根段会被跳过，如 `app.history.maxEntries`）
 * - `value`：新值（类型应与目标字段一致，由控件保证）
 *
 * 返回：写入后的完整子对象副本（可直接作为 `saveApp` / `saveReader` 的补丁）。
 */
export function buildPatch<T extends object>(section: T, id: string, value: unknown): T {
  const clone = JSON.parse(JSON.stringify(section)) as Record<string, unknown>;
  const segments = id.split('.').slice(1);
  let node: Record<string, unknown> = clone;
  for (const segment of segments.slice(0, -1)) {
    const child = node[segment];
    if (child === null || typeof child !== 'object') {
      node[segment] = {};
    }
    node = node[segment] as Record<string, unknown>;
  }
  node[segments[segments.length - 1]] = value;
  return clone as T;
}
