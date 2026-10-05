// 跨标签折叠记忆：折叠状态属 ReaderView 实例内的临时态，标签切换即清空；
// 本模块在切换前保存、加载后恢复，并由会话采集写入 session.json。
// 键 = 标签 id，值 = 折叠区域锚点（起始行 + 行数；恢复时与重算区间校验）。

import type { SessionFoldSpan } from '../ipc';

const folds = new Map<number, SessionFoldSpan[]>();

export const foldMemory = {
  /** 读取标签的折叠锚点（无记录返回 undefined） */
  get: (tabId: number): SessionFoldSpan[] | undefined => folds.get(tabId),
  /** 覆盖写入（空数组 = 明确无折叠，用于清除记录） */
  set: (tabId: number, spans: SessionFoldSpan[]): void => {
    if (spans.length === 0) {
      folds.delete(tabId);
      return;
    }
    folds.set(tabId, spans);
  },
  /** 预热（会话恢复：先于标签激活写入，首次加载即可套用） */
  seed: (tabId: number, spans: SessionFoldSpan[]): void => {
    if (spans.length > 0) folds.set(tabId, spans);
  },
  /** 关闭标签时清除 */
  remove: (tabId: number): void => {
    folds.delete(tabId);
  },
  /** 当前快照（复制；会话保存用） */
  snapshot: (): Map<number, SessionFoldSpan[]> => new Map(folds),
};
