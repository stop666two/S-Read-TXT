<script lang="ts">
  // 标签栏：选择/关闭 + 中键关闭 + 右键菜单 + 拖拽排序 + 溢出滚动。
  // 视觉规格：高 34px；活动标签顶部 2px 强调条 + 阅读区底色；
  // 宽度 90–180px、文本省略；关闭按钮悬停/活动时显现。
  // 拖拽采用指针事件自实现（非 HTML5 DnD）：跨平台一致且可被 CDP 自动化驱动。
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import { t } from '../i18n/index.svelte';
  import { ipc, type TabInfo } from '../ipc';
  import { computeDropHit } from '../tab-dnd';
  import Icon from './Icon.svelte';
  import TabContextMenu from './TabContextMenu.svelte';

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
    /** 设置标签颜色（右键菜单；null = 清除） */
    onSetColor?: (tabId: number, color: string | null) => void;
    /** 其他主窗口（跨窗口移动目标） */
    windows?: { label: string; title: string }[];
    /** 请求刷新窗口列表（右键菜单打开时由父级拉取） */
    onRequestWindows?: () => void;
    /** 移动标签到目标窗口 */
    onMoveToWindow?: (tabId: number, label: string) => void;
    /** 新建标签（标签栏右侧按钮） */
    onNewTab?: () => void;
    /** 新建窗口（标签栏右侧按钮；仅单栏时显示） */
    onNewWindow?: () => void;
    /** 多栏紧凑模式（显示栏位操作按钮） */
    compact?: boolean;
    /** 是否允许关闭本栏（多栏时可用） */
    canClosePane?: boolean;
    /** 向右拆分本栏 */
    onSplitRight?: () => void;
    /** 向下拆分本栏 */
    onSplitDown?: () => void;
    /** 关闭本栏（标签并入相邻栏） */
    onClosePane?: () => void;
    /** 本地拖拽移动（拖拽中每次指针移动；坐标用于父级栏位落点解析） */
    onLocalDragMove?: (clientX: number, clientY: number) => void;
    /** 本地拖拽释放（父级解析目标栏位并执行移动/重排） */
    onLocalDrop?: (tabId: number, clientX: number, clientY: number) => void;
    /** 本地拖拽取消（拖出窗口转原生 / 拖拽结束未释放） */
    onLocalDragCancel?: () => void;
  }
  let {
    tabs,
    activeId,
    onSelect,
    onClose,
    onCloseOthers,
    onCloseAll,
    onReorder,
    onSetColor,
    windows = [],
    onRequestWindows,
    onMoveToWindow,
    onNewTab,
    onNewWindow,
    compact = false,
    canClosePane = false,
    onSplitRight,
    onSplitDown,
    onClosePane,
    onLocalDragMove,
    onLocalDrop,
    onLocalDragCancel,
  }: Props = $props();

  /** 右键菜单状态（null = 关闭；坐标为视口像素） */
  let menu = $state<{ x: number; y: number; tabId: number } | null>(null);

  /** 右键菜单对应标签的颜色（菜单打开时随标签数据更新） */
  let menuColor = $derived(menu ? (tabs.find((tab) => tab.tabId === menu?.tabId)?.color ?? null) : null);

  /** 拖拽状态（pending：按下未越阈值；active：拖拽中） */
  let drag = $state<{ tabId: number; startX: number; active: boolean } | null>(null);
  /** 落点指示线（相对标签栏内容的左偏移，px；null = 不显示） */
  let dropLineLeft = $state<number | null>(null);
  /** 原生跨窗口拖拽是否进行中（越出源窗口后由 Rust 接管拖影与落点裁决） */
  let nativeActive = $state(false);
  /** 拖拽源窗口原点与缩放（物理坐标换算；阈值触发时捕获，失败时保持 null 仅本地排序） */
  let dragOrigin: { x: number; y: number; scale: number } | null = null;

  /** 标签栏滚动容器 */
  let bar = $state<HTMLElement | null>(null);
  /** 标签栏外壳（承载滚轮监听与右侧按钮定位） */
  let shell = $state<HTMLElement | null>(null);
  /** 拖拽后抑制 click（避免排序完又触发选择——选择本身无害，但会抢焦点） */
  let suppressClick = false;

  /** 拖动阈值（px）：超过才进入拖拽，避免影响普通点击 */
  const DRAG_THRESHOLD = 5;

  /** 指针按下：记录拖拽候选（仅左键；关闭按钮不参与）。 */
  function onPointerDown(event: PointerEvent, tabId: number): void {
    if (event.button !== 0) return;
    if ((event.target as HTMLElement).closest('.close')) return;
    drag = { tabId, startX: event.clientX, active: false };
    dragOrigin = null;
    // 指针捕获：越出窗口后仍能收到移动/抬起（Windows 隐式捕获 + 显式捕获双保险）
    try {
      (event.target as Element).setPointerCapture(event.pointerId);
    } catch {
      // 捕获失败不影响本地排序（越界由 Windows 隐式捕获兜底）
    }
  }

  /** 计算拖拽落点：返回「移除后插入下标」与指示线位置（相对内容坐标）。 */
  function computeDrop(clientX: number, sourceId: number): { index: number; lineLeft: number } {
    if (!bar) return { index: 0, lineLeft: 0 };
    return computeDropHit(bar, clientX, sourceId);
  }

  /** 指针是否越出本窗口边界 */
  function isOutside(event: PointerEvent): boolean {
    return (
      event.clientX < 0 ||
      event.clientY < 0 ||
      event.clientX > window.innerWidth ||
      event.clientY > window.innerHeight
    );
  }

  /** 坐标是否位于指定元素矩形内（跨渲染点判断用）。 */
  function isInsideRect(el: HTMLElement, clientX: number, clientY: number): boolean {
    const rect = el.getBoundingClientRect();
    return clientX >= rect.left && clientX <= rect.right && clientY >= rect.top && clientY <= rect.bottom;
  }

  /** 指针事件 → 屏幕物理坐标（拖拽会话使用） */
  function screenPoint(event: PointerEvent): { x: number; y: number } {
    const origin = dragOrigin ?? { x: 0, y: 0, scale: window.devicePixelRatio || 1 };
    return {
      x: origin.x + event.clientX * origin.scale,
      y: origin.y + event.clientY * origin.scale,
    };
  }

  /** 越出窗口：交给 Rust 原生拖拽（拖影窗口 + 跨窗口/桌面落点裁决）。 */
  function startNativeDrag(event: PointerEvent): void {
    const current = drag;
    if (!current || nativeActive) return;
    const tab = tabs.find((item) => item.tabId === current.tabId);
    if (!tab) return;
    nativeActive = true;
    const point = screenPoint(event);
    const label =
      tab.untitled != null ? t('untitled.name', { n: tab.untitled }) : tab.name;
    void ipc
      .beginTabDrag(current.tabId, label, tab.color, document.documentElement.dataset.themeBase === 'dark')
      .then(() => ipc.dragMove(point.x, point.y))
      .catch(() => {
        nativeActive = false;
      });
  }

  /** 窗口指针移动：激活拖拽并更新落点指示（越界后转原生拖拽）。 */
  function onPointerMove(event: PointerEvent): void {
    const current = drag;
    if (!current) return;
    if (!current.active) {
      if (Math.abs(event.clientX - current.startX) < DRAG_THRESHOLD) return;
      drag = { ...current, active: true };
      void getCurrentWindow()
        .outerPosition()
        .then((position) => {
          dragOrigin = { x: position.x, y: position.y, scale: window.devicePixelRatio || 1 };
        })
        .catch(() => {
          dragOrigin = null;
        });
    }
    if (nativeActive) {
      const point = screenPoint(event);
      void ipc.dragMove(point.x, point.y);
      dropLineLeft = null;
      return;
    }
    if (isOutside(event) && dragOrigin) {
      startNativeDrag(event);
      onLocalDragCancel?.();
      dropLineLeft = null;
      return;
    }
    // 指示线仅在本栏标签条内显示；跨栏落点由父级按指针坐标解析并预览
    dropLineLeft =
      bar && isInsideRect(bar, event.clientX, event.clientY)
        ? computeDrop(event.clientX, current.tabId).lineLeft
        : null;
    onLocalDragMove?.(event.clientX, event.clientY);
  }

  /** 窗口指针抬起：完成排序或结束拖拽。 */
  function onPointerUp(event: PointerEvent): void {
    const current = drag;
    drag = null;
    if (!current) return;
    if (nativeActive) {
      nativeActive = false;
      dropLineLeft = null;
      suppressClick = true;
      const point = screenPoint(event);
      dragOrigin = null;
      void ipc.dragEnd(point.x, point.y, false);
      return;
    }
    if (current.active) {
      suppressClick = true;
      if (onLocalDrop) {
        onLocalDrop(current.tabId, event.clientX, event.clientY);
      } else {
        const { index } = computeDrop(event.clientX, current.tabId);
        const from = tabs.findIndex((tab) => tab.tabId === current.tabId);
        if (from >= 0 && index !== from) onReorder(current.tabId, index);
      }
    } else {
      onLocalDragCancel?.();
    }
    dropLineLeft = null;
    dragOrigin = null;
  }

  /** 键盘：Esc 取消原生拖拽（拖出后释放）；本地排序不拦截。 */
  function onKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Escape' || !nativeActive) return;
    event.preventDefault();
    nativeActive = false;
    dropLineLeft = null;
    dragOrigin = null;
    void ipc.dragEnd(0, 0, true);
  }

  /** 标签点击（拖拽结束后的 click 忽略一次）。 */
  function handleSelect(tabId: number): void {
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    onSelect(tabId);
  }

  // 溢出滚动：纵向滚轮映射为横向滚动（常见于多标签场景）。
  // 监听挂在标签栏外壳上：悬停在右侧按钮区时滚轮同样滚动标签条。
  $effect(() => {
    const host = shell;
    const el = bar;
    if (!host || !el) return;
    const onWheel = (event: WheelEvent): void => {
      if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
      el.scrollLeft += event.deltaY;
      event.preventDefault();
    };
    host.addEventListener('wheel', onWheel, { passive: false });
    return () => host.removeEventListener('wheel', onWheel);
  });

  // 跨窗口拖放事件：悬停目标显示插入指示；源窗口在拖拽结束（原生侧）清理状态。
  $effect(() => {
    let stopHover: (() => void) | undefined;
    let stopEnd: (() => void) | undefined;
    void listen<{ active: boolean; clientX: number; clientY: number }>('srt://tab-drag-hover', (event) => {
      if (!event.payload.active) {
        dropLineLeft = null;
        return;
      }
      if (!bar) return;
      // 仅当悬停点位于本栏标签条内才显示插入线（多栏时其余落点由父级预览）
      dropLineLeft = isInsideRect(bar, event.payload.clientX, event.payload.clientY)
        ? computeDropHit(bar, event.payload.clientX, drag?.tabId).lineLeft
        : null;
    }).then((stop) => {
      stopHover = stop;
    });
    void listen('srt://tab-drag-end', () => {
      nativeActive = false;
      dropLineLeft = null;
      dragOrigin = null;
    }).then((stop) => {
      stopEnd = stop;
    });
    return () => {
      stopHover?.();
      stopEnd?.();
    };
  });
</script>

<svelte:window
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onkeydown={onKeydown}
/>

<div class="tab-shell" class:compact bind:this={shell}>
  <div class="tab-bar" role="tablist" aria-label={t('tabBar.aria')} bind:this={bar}>
  {#each tabs as tab (tab.tabId)}
    <div
      class="tab"
      class:active={tab.tabId === activeId}
      class:dragging={drag?.active === true && drag.tabId === tab.tabId}
      data-tab-id={tab.tabId}
      data-color={tab.color ?? undefined}
      role="tab"
      aria-selected={tab.tabId === activeId}
      tabindex={0}
      title={tab.path}
      onclick={() => handleSelect(tab.tabId)}
      oncontextmenu={(event) => {
        event.preventDefault();
        onRequestWindows?.();
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
      {#if tab.color}
        <span class="color-bar" aria-hidden="true"></span>
      {/if}
      <span class="name">{tab.untitled != null ? t('untitled.name', { n: tab.untitled }) : tab.name}</span>
      <button
        class="close"
        title={t('tabBar.closeHint')}
        aria-label={t('tabBar.closeAria', { name: tab.untitled != null ? t('untitled.name', { n: tab.untitled }) : tab.name })}
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
  <div class="bar-actions">
    <button
      class="bar-btn"
      type="button"
      title={t('tabBar.newTabHint')}
      aria-label={t('tabBar.newTabAria')}
      data-tab-new
      onclick={() => onNewTab?.()}
    >
      <Icon name="plus" size={14} />
    </button>
    <button
      class="bar-btn"
      type="button"
      title={t('tabBar.splitRightHint')}
      aria-label={t('tabBar.splitRightAria')}
      data-pane-split-right
      onclick={() => onSplitRight?.()}
    >
      <Icon name="split-right" size={14} />
    </button>
    <button
      class="bar-btn"
      type="button"
      title={t('tabBar.splitDownHint')}
      aria-label={t('tabBar.splitDownAria')}
      data-pane-split-down
      onclick={() => onSplitDown?.()}
    >
      <Icon name="split-down" size={14} />
    </button>
    {#if compact}
      <button
        class="bar-btn"
        type="button"
        title={t('tabBar.closePaneHint')}
        aria-label={t('tabBar.closePaneAria')}
        data-pane-close
        disabled={!canClosePane}
        onclick={() => onClosePane?.()}
      >
        <Icon name="close" size={12} />
      </button>
    {:else}
      <button
        class="bar-btn"
        type="button"
        title={t('tabBar.newWindowHint')}
        aria-label={t('tabBar.newWindowAria')}
        data-window-new
        onclick={() => onNewWindow?.()}
      >
        <Icon name="window" size={14} />
      </button>
    {/if}
  </div>
</div>

{#if menu}
  <TabContextMenu
    x={menu.x}
    y={menu.y}
    color={menuColor}
    windows={windows}
    onClose={() => onClose(menu?.tabId ?? 0)}
    onCloseOthers={() => onCloseOthers(menu?.tabId ?? 0)}
    onCloseAll={onCloseAll}
    onSetColor={(color) => onSetColor?.(menu?.tabId ?? 0, color)}
    onMoveTo={(label) => onMoveToWindow?.(menu?.tabId ?? 0, label)}
    onDismiss={() => (menu = null)}
  />
{/if}

<style>
  .tab-bar {
    position: relative;
    display: flex;
    align-items: stretch;
    height: var(--h-tabbar);
    padding-right: 92px;
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

  /* 多栏紧凑模式：更窄的标签与更宽的右侧按钮区（四个栏位操作按钮）。 */
  .tab-shell.compact .tab-bar {
    padding-right: 118px;
  }

  .tab-shell.compact .tab {
    min-width: 70px;
    max-width: 140px;
    font-size: 12px;
  }

  .bar-btn:disabled {
    opacity: 0.45;
    pointer-events: none;
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

  /* 标签颜色竖条（左侧；色值由 --tab-color 按 data-color 映射） */
  .tab[data-color='red'] {
    --tab-color: var(--tab-color-red);
  }
  .tab[data-color='orange'] {
    --tab-color: var(--tab-color-orange);
  }
  .tab[data-color='yellow'] {
    --tab-color: var(--tab-color-yellow);
  }
  .tab[data-color='green'] {
    --tab-color: var(--tab-color-green);
  }
  .tab[data-color='cyan'] {
    --tab-color: var(--tab-color-cyan);
  }
  .tab[data-color='blue'] {
    --tab-color: var(--tab-color-blue);
  }
  .tab[data-color='purple'] {
    --tab-color: var(--tab-color-purple);
  }
  .tab[data-color='gray'] {
    --tab-color: var(--tab-color-gray);
  }

  .color-bar {
    position: absolute;
    left: 0;
    top: 2px;
    bottom: 2px;
    width: 3px;
    border-radius: 0 2px 2px 0;
    background: var(--tab-color);
    pointer-events: none;
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

  .tab-shell {
    position: relative;
    height: var(--h-tabbar);
  }

  .bar-actions {
    position: absolute;
    top: 0;
    right: 4px;
    height: var(--h-tabbar);
    display: flex;
    align-items: center;
    gap: 2px;
    padding-left: 6px;
    background: var(--chrome);
    border-left: 1px solid var(--line);
  }

  .bar-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 22px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--muted);
    cursor: default;
  }

  .bar-btn:hover {
    background: var(--hover);
    color: var(--ink);
  }
</style>
