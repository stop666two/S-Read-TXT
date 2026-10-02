<script lang="ts">
  // 阅读视图：虚拟滚动（仅渲染可视行 + 实测高度缓存 + 滚动锚定）+ 按窗取行。
  // 进度口径：状态栏百分比 = 顶部定位行的累计高度 / 内容总高度（视觉进度）。
  // 标签/编码切换：重建高度与缓存、按记忆行号恢复滚动位置（阶段 8 会话持久化同口径）。
  import { untrack } from 'svelte';

  import EditLayer from './EditLayer.svelte';
  import type { EditorAction } from '../edit/actions';
  import { describeIpcError, ipc, toIpcError, type EditApplied, type TabInfo } from '../ipc';
  import { HeightModel } from '../reader/heights';
  import { RowCache } from '../reader/row-cache';
  import { scrollMemory } from '../reader/scroll-memory';
  import { computePercent, computeWindow, planBatches } from '../reader/viewport';
  import { toasts } from '../state/toasts.svelte';

  interface Props {
    /** 当前活动标签（组件仅服务活动标签） */
    tab: TabInfo;
    /** 阅读百分比回报（状态栏显示） */
    onPercent: (percent: number) => void;
    /** 编辑应用回报（App 同步标签信息：行数/字节数/脏态） */
    onEditApplied?: (tabId: number, result: EditApplied) => void;
    /** 外部编辑动作信号（菜单触发；透传给编辑层） */
    editorAction?: EditorAction | null;
    /** 排版变更键（字体/字号/行高/限宽/边距；变化触发行高失效重排） */
    layoutKey: string;
  }
  let { tab, onPercent, onEditApplied, editorAction, layoutKey }: Props = $props();

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
  /** 标签 → 顶部定位行的注册表（跨模块共享；见 reader/scroll-memory.ts） */
  /** 程序化滚动标记（锚定补偿时避免重入滚动处理） */
  let programmatic = false;
  let lastPercent = -1;
  let scrollScheduled = false;

  /** 当前渲染行（窗口 + 文本；version 驱动刷新）。 */
  const renderedRows = $derived.by(() => {
    void version;
    const rows: { row: number; text: string }[] = [];
    for (let row = startRow; row < endRow; row += 1) {
      rows.push({ row, text: cache.get(row)?.text ?? '' });
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
          for (const row of payload.rows) {
            cache.set(row.row, {
              text: row.text,
              logicalRow: row.logicalRow ?? row.row,
              baseUtf16: row.baseUtf16 ?? 0,
            });
          }
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

  /** 编辑结果回报：失效受影响行起的缓存与行高（行号平移的最小正确范围），
   *  刷新窗口并上报 App（同步标签信息）。 */
  function handleEditApplied(result: EditApplied): void {
    cache.invalidateFrom(result.touchedRow);
    heights.invalidateFrom(result.touchedRow);
    inflight.clear();
    lastPercent = -1;
    refreshWindow();
    version += 1;
    onEditApplied?.(tab.tabId, result);
  }

  /** 确保单行已加载（编辑层光标定位/复制使用；返回加载后的文本）。 */
  async function ensureRow(row: number): Promise<string | undefined> {
    const cached = cache.get(row);
    if (cached !== undefined) return cached.text;
    try {
      const payload = await ipc.getRows(tab.tabId, row, 1);
      const item = payload.rows[0];
      if (item !== undefined) {
        cache.set(row, {
          text: item.text,
          logicalRow: item.logicalRow ?? row,
          baseUtf16: item.baseUtf16 ?? 0,
        });
        version += 1;
      }
      return item?.text;
    } catch (error) {
      if (import.meta.env.DEV) console.error('[reader] 单行取行失败', error);
      toasts.error(describeIpcError(toIpcError(error)));
      return undefined;
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

  /** 滚动处理（rAF 节流）：刷新窗口 + 实时更新滚动记忆（会话保存直接读取）。 */
  function handleScroll(): void {
    if (programmatic || scrollScheduled) return;
    scrollScheduled = true;
    requestAnimationFrame(() => {
      scrollScheduled = false;
      refreshWindow();
      // 实时记录顶部定位行（会话/标签切换共用数据源）；
      // 此前仅在切换标签的清理阶段记录，导致「滚动后直接退出」恢复不到位置。
      scrollMemory.set(
        tab.tabId,
        heights.rowAtOffset(contentScrollTop(), Math.max(1, tab.rowsTotal)),
      );
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
    // 实时查询已渲染行（与 rowNodeOf 同一策略：不维护易失步的注册表）
    for (const node of el.querySelectorAll<HTMLElement>('.row[data-row]')) {
      const row = Number(node.dataset.row ?? -1);
      if (!Number.isFinite(row) || row < 0) continue;
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

  /** 按行号实时查询已渲染的行元素。
   *  说明：此前用「注册表 Map」记录节点，但属性对象更新触发重建后注册表会失步
   *  （编辑层找不到行 → 光标/选区叠加层死亡）；DOM 实时查询天然与渲染状态一致。 */
  function rowNodeOf(row: number): HTMLElement | undefined {
    const el = container?.querySelector(`[data-row="${row}"]`);
    return el instanceof HTMLElement ? el : undefined;
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

  // 排版变更（字体/字号/行高/限宽/边距）：行高模型失效并重排；
  // 滚动位置由 measureRendered 的锚定机制保持（不会跳回顶部）。
  // 关键：version 的自增必须在 untrack 内——`version += 1` 同时读取并写入该 $state，
  // 若参与依赖收集会让本 effect 自触发形成死循环（实测每帧数千次，渲染管线被持续冲刷）。
  $effect(() => {
    void layoutKey;
    untrack(() => {
      heights.clear();
      version += 1;
    });
  });

  /** 应用初始滚动位置（会话恢复/切回长文档）。
   *  冷启动挂载早于首屏行与占位渲染完成时，容器可能还没有足量可滚动高度，
   *  直接赋值 scrollTop 会被浏览器钳到 0；因此逐帧重试直到赋值真正生效
   *  （上限约 2 秒），确保阅读位置不丢。 */
  function applyInitialScroll(row: number, rowsTotal: number): void {
    let attempts = 0;
    const attempt = (): void => {
      if (!container) return;
      const contentTop = heights.offsetOf(row, Math.max(1, rowsTotal));
      setContentScrollTop(contentTop);
      if (Math.abs(container.scrollTop - (contentTop + pagePadTop)) <= 2 || attempts >= 120) {
        requestAnimationFrame(() => refreshWindow());
        return;
      }
      attempts += 1;
      requestAnimationFrame(attempt);
    };
    attempt();
  }

  // 标签或编码变化：重建缓存/高度，按记忆行号恢复位置；离开前记录当前行号
  $effect(() => {
    const currentTabId = tab.tabId;
    const rowsTotal = tab.rowsTotal;
    void tab.encoding;
    cache.clear();
    heights.clear();
    inflight.clear();
    lastPercent = -1;
    const restoredRow = Math.min(scrollMemory.get(currentTabId) ?? 0, Math.max(0, rowsTotal - 1));
    if (container) {
      applyInitialScroll(restoredRow, rowsTotal);
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
        <div class="row" data-row={item.row}>{item.text}</div>
      {/each}
      <div class="spacer" style="height: {spacerBottom}px"></div>
    {/if}
    {#if tab.editing}
      <EditLayer
        tabId={tab.tabId}
        rowsTotal={tab.rowsTotal}
        revision={version}
        rowNode={rowNodeOf}
        rowText={(row) => cache.get(row)?.text}
        rowMeta={(row) => {
          const cached = cache.get(row);
          return cached
            ? { logicalRow: cached.logicalRow, baseUtf16: cached.baseUtf16 }
            : undefined;
        }}
        ensureRow={(row) => ensureRow(row)}
        getContainer={() => container}
        onApplied={handleEditApplied}
        {editorAction}
      />
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
    position: relative;
    z-index: 0;
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
