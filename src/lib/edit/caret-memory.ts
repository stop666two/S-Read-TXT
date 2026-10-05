// 跨标签光标记忆：编辑层组件随标签切换重建，位置经本模块跨实例保留。
// 键 = 标签 id，值 = 最近一次光标位置（切回标签时恢复；会话保存时落盘）。

import type { CaretPos } from './caret';

const memory = new Map<number, CaretPos>();

export const caretMemory = {
  /** 读取标签的光标位置（无记录返回 undefined） */
  get: (tabId: number): CaretPos | undefined => memory.get(tabId),
  /** 记录光标位置 */
  set: (tabId: number, pos: CaretPos): void => {
    memory.set(tabId, pos);
  },
  /** 预热（会话恢复：仅非文档开头位置才写，防止覆盖有效记录） */
  seed: (tabId: number, pos: CaretPos): void => {
    if (pos.row > 0 || pos.utf16 > 0) memory.set(tabId, pos);
  },
  /** 关闭标签时清除 */
  remove: (tabId: number): void => {
    memory.delete(tabId);
  },
  /** 当前快照（复制；会话保存用） */
  snapshot: (): Map<number, CaretPos> => new Map(memory),
};
