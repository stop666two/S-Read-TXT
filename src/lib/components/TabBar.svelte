<script lang="ts">
  // 标签栏（阶段 5 完成版）：选择/关闭 + 中键关闭 + 右键菜单 + 拖拽排序 + 溢出滚动。
  // 视觉规格（设计 D28）：高 34px；活动标签顶部 2px 强调条 + 阅读区底色；
  // 宽度 90–180px、文本省略；关闭按钮悬停/活动时显现。
  // 拖拽采用指针事件自实现（非 HTML5 DnD）：跨平台一致且可被 CDP 自动化驱动。
  import Icon from './Icon.svelte';
  import TabContextMenu from './TabContextMenu.svelte';
  import type { TabInfo } from '../ipc';

  interface Props {
    /** 标签列表（后端展示顺序） */
    tabs: TabInfo[];
    /** 活动标签 id */
    activeId: number | null;
    /** 选择标签 */
    onSelect: (tabId: number) => void;
    /** 关闭标签 */
    onClose: (tabId: number) => void;
    /** 关闭其他标签（保留指定标签） */
    onCloseOthers: (tabId: number) => void;
    /** 关闭全部标签 */
    onCloseAll: () => void;
    /** 拖拽排序（toIndex = 移除后插入下标语义） */
    onReorder: (tabId: number, toIndex: number) => void;
  }
  let { tabs, activeId, onSelect, onClose, onCloseOthers, onCloseAll, onReorder }: Props = $props();

  /** 右键菜单状态（null = 关闭；坐标为视口像素） */
  let menu = $state<{ x: number; y: number; tabId: number } | null>(null);

  /** 拖拽状态（pending：按下未越阈值；active：拖拽中） */
  let drag = $state<{ tabId: number; startX: number; active: boolean } | null>(null);
  /** 落点指示线（相对标签栏内容的左偏移，px；null = 不显示） */
  let dropLineLeft = $state<number | null>(null);

  /** 标签栏滚动容器 */
  let bar = $state<HTMLElement | null>(null);
  /** 拖拽后抑制 click（避免排序完又触发选择——选择本身无害，但会抢焦点） */
  let suppressClick = false;

  /** 拖动阈值（px）：超过才进入拖拽，避免影响普通点击 */
  const DRAG_THRESHOLD = 5;

  /** 指针按下：记录拖拽候选（仅左键；关闭按钮不参与）。 */
  function onPointerDown(event: PointerEvent, tabId: number): void {
    if (event.button !== 0) return;
    if ((event.target as HTMLElement).closest('.close')) return;
    drag = { tabId, startX: event.clientX, active: false };
  }

  /** 计算拖拽落点：返回「移除后插入下标」与指示线位置（相对内容坐标）。 */
  function computeDrop(clientX: number, sourceId: number): { index: number; lineLeft: number } {
    const el = bar;
    if (!el) return { index: 0, lineLeft: 0 };
    const others = [...el.querySelectorAll<HTMLElement>('[data-tab-id]')].filter(
      (node) => Number(node.dataset.tabId) !== sourceId,
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

  /** 窗口指针移动：激活拖拽并更新落点指示。 */
  function onPointerMove(event: PointerEvent): void {
    const current = drag;
    if (!current) return;
    if (!current.active) {
      if (Math.abs(event.clientX - current.startX) < DRAG_THRESHOLD) return;
      drag = { ...current, active: true };
    }
    dropLineLeft = computeDrop(event.clientX, current.tabId).lineLeft;
  }

  /** 窗口指针抬起：完成排序或结束拖拽。 */
  function onPointerUp(event: PointerEvent): void {
    const current = drag;
    drag = null;
    if (!current) return;
    if (current.active) {
      suppressClick = true;
      const { index } = computeDrop(event.clientX, current.tabId);
      const from = tabs.findIndex((tab) => tab.tabId === current.tabId);
      if (from >= 0 && index !== from) onReorder(current.tabId, index);
    }
    dropLineLeft = null;
  }

  /** 标签点击（拖拽结束后的 click 忽略一次）。 */
  function handleSelect(tabId: number): void {
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    onSelect(tabId);
  }

  // 溢出滚动：纵向滚轮映射为横向滚动（常见于多标签场景）
  $effect(() => {
    const el = bar;
    if (!el) return;
    const onWheel = (event: WheelEvent): void => {
      if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
      el.scrollLeft += event.deltaY;
      event.preventDefault();
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  });
</script>

<svelte:window onpointermove={onPointerMove} onpointerup={onPointerUp} onpointercancel={onPointerUp} />

<div class="tab-bar" role="tablist" aria-label="打开的文件" bind:this={bar}>
  {#each tabs as tab (tab.tabId)}
    <div
      class="tab"
      class:active={tab.tabId === activeId}
      class:dragging={drag?.active === true && drag.tabId === tab.tabId}
      data-tab-id={tab.tabId}
      role="tab"
      aria-selected={tab.tabId === activeId}
      tabindex={0}
      title={tab.path}
      onclick={() => handleSelect(tab.tabId)}
      oncontextmenu={(event) => {
        event.preventDefault();
        menu = { x: event.clientX, y: event.clientY, tabId: tab.tabId };
      }}
      onmousedown={(event) => {
        // 中键：阻止自动滚动；关闭动作在 auxclick
        if (event.button === 1) event.preventDefault();
      }}
      onauxclick={(event) => {
        if (event.button === 1) {
          event.preventDefault();
          onClose(tab.tabId);
        }
      }}
      onpointerdown={(event) => onPointerDown(event, tab.tabId)}
      onkeydown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect(tab.tabId);
        }
      }}
    >
      <span class="name">{tab.name}</span>
      <button
        class="close"
        title="关闭标签（Ctrl+W）"
        aria-label={`关闭 ${tab.name}`}
        onclick={(event) => {
          event.stopPropagation();
          onClose(tab.tabId);
        }}
      >
        <Icon name="close" size={12} />
      </button>
    </div>
  {/each}
  {#if dropLineLeft !== null}
    <div class="drop-line" style="left: {dropLineLeft}px"></div>
  {/if}
</div>

{#if menu}
  <TabContextMenu
    x={menu.x}
    y={menu.y}
    onClose={() => onClose(menu?.tabId ?? 0)}
    onCloseOthers={() => onCloseOthers(menu?.tabId ?? 0)}
    onCloseAll={onCloseAll}
    onDismiss={() => (menu = null)}
  />
{/if}

<style>
  .tab-bar {
    position: relative;
    display: flex;
    align-items: stretch;
    height: var(--h-tabbar);
    background: var(--chrome);
    border-bottom: 1px solid var(--line);
    user-select: none;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .tab-bar::-webkit-scrollbar {
    display: none;
  }

  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 0 0 auto;
    min-width: 90px;
    max-width: 180px;
    padding: 0 8px 0 12px;
    border-right: 1px solid var(--line);
    color: var(--muted);
    font-size: 12.5px;
    cursor: default;
  }

  .tab:hover {
    background: var(--hover);
    color: var(--ink);
  }

  .tab.active {
    background: var(--tab-active);
    color: var(--ink);
  }

  .tab.active::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--accent);
  }

  .tab.dragging {
    opacity: 0.55;
  }

  .name {
    flex: 1;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: inherit;
    opacity: 0;
    cursor: default;
  }

  .tab:hover .close,
  .tab.active .close {
    opacity: 1;
  }

  .close:hover {
    background: var(--hover);
  }

  .drop-line {
    position: absolute;
    top: 2px;
    bottom: 2px;
    width: 2px;
    background: var(--accent);
    pointer-events: none;
  }
</style>
