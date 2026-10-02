<script lang="ts">
  // 阅读视图：虚拟滚动（仅渲染可视行 + 实测高度缓存 + 滚动锚定）+ 按窗取行。
  // 进度口径：状态栏百分比 = 顶部定位行的累计高度 / 内容总高度（视觉进度）。
  // 标签/编码切换：重建高度与缓存、按记忆行号恢复滚动位置（阶段 8 会话持久化同口径）。
  import { describeIpcError, ipc, toIpcError, type TabInfo } from '../ipc';
  import { HeightModel } from '../reader/heights';
  import { RowCache } from '../reader/row-cache';
  import { computePercent, computeWindow, planBatches } from '../reader/viewport';
  import { toasts } from '../state/toasts.svelte';

  interface Props {
    /** 当前活动标签（组件仅服务活动标签） */
    tab: TabInfo;
    /** 阅读百分比回报（状态栏显示） */
    onPercent: (percent: number) => void;
  }
  let { tab, onPercent }: Props = $props();

  /** 可视区上下额外渲染行数（预取缓冲） */
  const OVERSCAN = 30;
  /** 单次取行批量上限 */
  const MAX_BATCH = 256;
  /** 基准行高（16px × 1.8 ≈ 29px；实测后自动校准） */
  const BASE_LINE_HEIGHT = 29;
  /** 页面顶部内边距初始值（ResizeObserver 首次回调会用真实计算值覆盖） */
  const DEFAULT_PAGE_PAD_TOP = 48;

  /** 滚动容器 */
  let container = $state<HTMLElement | null>(null);
  /** 渲染重算触发计数（缓存到达/测量更新时自增） */
  let version = $state(0);
  /** 当前渲染窗口 [startRow, endRow) */
  let startRow = $state(0);
  let endRow = $state(0);
  /** 上下占位高度（px） */
  let spacerTop = $state(0);
  let spacerBottom = $state(0);
  /** 页面顶部内边距（内容坐标系换算） */
  let pagePadTop = DEFAULT_PAGE_PAD_TOP;

  const heights = new HeightModel(BASE_LINE_HEIGHT);
  const cache = new RowCache();
  /** 在途取行批次键（防重复请求） */
  const inflight = new Set<string>();
  /** 已渲染行节点（测量用） */
  const nodes = new Map<number, HTMLElement>();
  /** 标签 → 顶部定位行号（跨标签切换恢复位置） */
  const scrollMemory = new Map<number, number>();
  /** 程序化滚动标记（锚定补偿时避免重入滚动处理） */
  let programmatic = false;
  let lastPercent = -1;
  let scrollScheduled = false;

  /** 当前渲染行（窗口 + 文本；version 驱动刷新）。 */
  const renderedRows = $derived.by(() => {
    void version;
    const rows: { row: number; text: string }[] = [];
    for (let row = startRow; row < endRow; row += 1) {
      rows.push({ row, text: cache.get(row) ?? '' });
    }
    return rows;
  });

  /** 内容坐标系滚动位置（容器 scrollTop 扣除页面顶部内边距）。 */
  function contentScrollTop(): number {
    const el = container;
    if (!el) return 0;
    return Math.max(0, el.scrollTop - pagePadTop);
  }

  /** 设置滚动位置（内容坐标），标记程序化以避免窗口重算循环。 */
  function setContentScrollTop(top: number): void {
    const el = container;
    if (!el) return;
    programmatic = true;
    el.scrollTop = top + pagePadTop;
    requestAnimationFrame(() => {
      programmatic = false;
    });
  }

  /** 请求窗口内缺失的行（合并为连续批次；切换标签后丢弃过期结果）。 */
  function ensureRows(start: number, end: number): void {
    const wanted: number[] = [];
    for (let row = start; row < end; row += 1) wanted.push(row);
    for (const batch of planBatches(wanted, (row) => cache.has(row), MAX_BATCH)) {
      const key = `${batch.start}:${batch.count}`;
      if (inflight.has(key)) continue;
      inflight.add(key);
      const tabId = tab.tabId;
      void ipc.getRows(tabId, batch.start, batch.count).then(
        (payload) => {
          inflight.delete(key);
          if (tab.tabId !== tabId) return;
          for (const row of payload.rows) cache.set(row.row, row.text);
          version += 1;
        },
        (error: unknown) => {
          inflight.delete(key);
          if (tab.tabId !== tabId) return;
          if (import.meta.env.DEV) console.error('[reader] 取行失败', error);
          toasts.error(describeIpcError(toIpcError(error)));
        },
      );
    }
  }

  /** 依据当前滚动位置重建渲染窗口、触发取行并上报进度。 */
  function refreshWindow(): void {
    const el = container;
    if (!el) return;
    const { start, end, anchorRow } = computeWindow(
      heights,
      tab.rowsTotal,
      contentScrollTop(),
      el.clientHeight,
      OVERSCAN,
    );
    startRow = start;
    endRow = end;
    spacerTop = heights.offsetOf(start, tab.rowsTotal);
    spacerBottom = Math.max(
      0,
      heights.totalHeight(tab.rowsTotal) - heights.offsetOf(end, tab.rowsTotal),
    );
    ensureRows(start, end);
    scrollMemory.set(tab.tabId, anchorRow);
    const percent = computePercent(heights, tab.rowsTotal, anchorRow);
    if (Math.round(percent) !== lastPercent) {
      lastPercent = Math.round(percent);
      onPercent(percent);
    }
  }

  /** 滚动处理（rAF 节流）。 */
  function handleScroll(): void {
    if (programmatic || scrollScheduled) return;
    scrollScheduled = true;
    requestAnimationFrame(() => {
      scrollScheduled = false;
      refreshWindow();
    });
  }

  /** 测量已渲染行高，并做滚动锚定（窗口上方高度变化时保持视觉位置不跳）。 */
  function measureRendered(): void {
    const el = container;
    if (!el) return;
    const beforeScroll = contentScrollTop();
    const anchorRow = heights.rowAtOffset(beforeScroll, tab.rowsTotal);
    const anchorDelta = Math.max(0, beforeScroll - heights.offsetOf(anchorRow, tab.rowsTotal));
    let changed = false;
    for (const [row, node] of nodes) {
      const height = node.offsetHeight;
      if (height > 0 && Math.abs(heights.heightOf(row) - height) > 0.5) {
        heights.measure(row, height);
        changed = true;
      }
    }
    if (!changed) return;
    const corrected = heights.offsetOf(anchorRow, tab.rowsTotal) + anchorDelta;
    if (Math.abs(corrected - beforeScroll) > 1) {
      programmatic = true;
      el.scrollTop = corrected + pagePadTop;
      requestAnimationFrame(() => {
        programmatic = false;
      });
    }
    spacerTop = heights.offsetOf(startRow, tab.rowsTotal);
    spacerBottom = Math.max(
      0,
      heights.totalHeight(tab.rowsTotal) - heights.offsetOf(endRow, tab.rowsTotal),
    );
    version += 1;
  }

  /** 行节点注册（Svelte action）。 */
  function rowHost(node: HTMLElement, row: number) {
    nodes.set(row, node);
    return {
      destroy(): void {
        nodes.delete(row);
      },
    };
  }

  // DOM 更新后测量（窗口变化或文本到达均会改变 version/startRow/endRow）
  $effect(() => {
    void version;
    void startRow;
    void endRow;
    measureRendered();
  });

  // 视口尺寸变化：列宽变化会改变折行，行高需全部重测
  $effect(() => {
    const el = container;
    if (!el) return;
    const observer = new ResizeObserver(() => {
      const page = el.querySelector('.page');
      if (page instanceof HTMLElement) {
        pagePadTop = Number.parseFloat(getComputedStyle(page).paddingTop) || 0;
      }
      heights.clear();
      refreshWindow();
      version += 1;
    });
    observer.observe(el);
    return () => {
      observer.disconnect();
    };
  });

  // 标签或编码变化：重建缓存/高度，按记忆行号恢复位置；离开前记录当前行号
  $effect(() => {
    const currentTabId = tab.tabId;
    const rowsTotal = tab.rowsTotal;
    void tab.encoding;
    cache.clear();
    heights.clear();
    inflight.clear();
    nodes.clear();
    lastPercent = -1;
    const restoredRow = Math.min(scrollMemory.get(currentTabId) ?? 0, Math.max(0, rowsTotal - 1));
    if (container) {
      setContentScrollTop(heights.offsetOf(restoredRow, rowsTotal));
      requestAnimationFrame(() => {
        refreshWindow();
      });
    }
    return () => {
      scrollMemory.set(
        currentTabId,
        heights.rowAtOffset(contentScrollTop(), Math.max(1, rowsTotal)),
      );
    };
  });
</script>

<div class="reader" bind:this={container} onscroll={handleScroll}>
  <div class="page">
    {#if tab.rowsTotal === 0}
      <p class="empty-file">（空文件）</p>
    {:else}
      <div class="spacer" style="height: {spacerTop}px"></div>
      {#each renderedRows as item (item.row)}
        <div class="row" use:rowHost={item.row}>{item.text}</div>
      {/each}
      <div class="spacer" style="height: {spacerBottom}px"></div>
    {/if}
  </div>
</div>

<style>
  .reader {
    flex: 1;
    overflow-y: auto;
    background: var(--base);
  }

  .page {
    max-width: var(--reading-width);
    margin: 0 auto;
    padding: var(--reading-pad-y) var(--reading-pad-x);
    font-family: var(--font-reading);
    font-size: var(--reading-size);
    line-height: var(--reading-line-height);
    color: var(--ink);
  }

  .row {
    white-space: pre-wrap;
    overflow-wrap: break-word;
    min-height: calc(var(--reading-line-height) * 1em);
  }

  .spacer {
    width: 100%;
  }

  .empty-file {
    margin: 30vh 0 0;
    text-align: center;
    color: var(--muted);
  }
</style>
