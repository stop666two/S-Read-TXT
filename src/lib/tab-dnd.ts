// 标签拖放命中计算（标签栏本地排序与跨窗口落点共用的纯几何）。
//
// 说明：`clientX` 为标签栏所在窗口的**内容坐标**（CSS px）；
// 跨窗口拖放时由 Rust 侧换算后随事件传入，语义与本地指针事件一致。

/** 落点命中结果 */
export interface DropHit {
  /** 「移除后插入下标」语义（越界由后端收敛） */
  index: number;
  /** 指示线左偏移（相对标签栏内容，px） */
  lineLeft: number;
}

/**
 * 计算落点：按各标签中点命中，返回插入下标与指示线位置。
 *
 * 参数：`bar` 标签栏容器；`clientX` 内容坐标 X；`excludeId` 排除的标签 id
 * （拖拽源标签；跨窗口悬停指示传 undefined）。
 */
export function computeDropHit(bar: HTMLElement, clientX: number, excludeId?: number): DropHit {
  const others = [...bar.querySelectorAll<HTMLElement>('[data-tab-id]')].filter(
    (node) => Number(node.dataset.tabId) !== excludeId,
  );
  let index = 0;
  for (const node of others) {
    const rect = node.getBoundingClientRect();
    if (clientX > rect.left + rect.width / 2) index += 1;
  }
  if (others.length === 0) return { index: 0, lineLeft: 0 };
  const target = others[Math.min(index, others.length - 1)];
  const lineLeft = index >= others.length ? target.offsetLeft + target.offsetWidth : target.offsetLeft;
  return { index, lineLeft };
}
