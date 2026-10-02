// 阅读滚动位置注册表（跨模块共享）
// 用途：ReaderView 在标签切换/离开时记录顶部定位行；App 在退出时写会话、
// 启动恢复时预热（seed）——保证「会话恢复滚动位置」与「标签切换恢复」同一份数据。

const rows = new Map<number, number>();

export const scrollMemory = {
  /** 读取标签的定位行（无记录返回 undefined） */
  get: (tabId: number): number | undefined => rows.get(tabId),
  /** 记录标签的定位行 */
  set: (tabId: number, row: number): void => {
    rows.set(tabId, row);
  },
  /** 预热（会话恢复：行号 > 0 才记录，避免把顶部标签写成 0 覆盖有效值） */
  seed: (tabId: number, row: number): void => {
    if (row > 0) rows.set(tabId, row);
  },
  /** 关闭标签时清除 */
  remove: (tabId: number): void => {
    rows.delete(tabId);
  },
  /** 当前快照（复制；会话保存用） */
  snapshot: (): Map<number, number> => new Map(rows),
};
