<script lang="ts">
  // 阅读视图：虚拟滚动（仅渲染可视行 + 实测高度缓存 + 滚动锚定）+ 按窗取行。
  // 进度口径：状态栏百分比 = 顶部定位行的累计高度 / 内容总高度（视觉进度）。
  // 标签/编码切换：重建高度与缓存、按记忆行号恢复滚动位置（阶段 8 会话持久化同口径）。
  import { untrack } from 'svelte';

  import EditLayer from './EditLayer.svelte';
  import type { EditorAction } from '../edit/actions';
  import { t } from '../i18n/index.svelte';
  import {
    describeIpcError,
    ipc,
    toIpcError,
    type EditApplied,
    type FilterQuery,
    type TabInfo,
  } from '../ipc';
  import { HeightModel } from '../reader/heights';
  import { RowCache } from '../reader/row-cache';
  import { scrollMemory } from '../reader/scroll-memory';
  import { jumpStore } from '../state/jump.svelte';
  import { computePercent, computeWindow, planBatches } from '../reader/viewport';
  import { toasts } from '../state/toasts.svelte';
  import type {
    AutoPairsSettings,
    CleanupSettings,
    EditorLinesSettings,
    FindSettings,
    InsertSettings,
    MultiCursorSettings,
    TextStats,
  } from '../ipc';

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
    /** 行操作默认值（编辑器设置；透传给编辑层弹窗；设置未就绪为 null） */
    lineDefaults?: EditorLinesSettings | null;
  /** 多光标设置（透传编辑层） */
  multiCursor?: MultiCursorSettings | null;
  /** 查找设置（透传编辑层；未就绪为 null） */
  findSettings?: FindSettings | null;
  /** 时间戳插入设置（透传编辑层；未就绪为 null） */
  insertSettings?: InsertSettings | null;
  /** 括号匹配/自动缩进设置（透传编辑层；未就绪为 null） */
  autoPairs?: AutoPairsSettings | null;
  /** 清理类操作设置（透传编辑层；未就绪为 null） */
  cleanupSettings?: CleanupSettings | null;
    /** 状态栏：顶部可视行回报（1 基；P2-1） */
    onTopRow?: (row: number) => void;
    /** 状态栏：选区统计透传（编辑层；P2-1） */
    onSelectionStats?: (stats: TextStats | null) => void;
    /** 状态栏：光标行列透传（编辑层；P2-1） */
    onCaretInfo?: (info: { row: number; column: number }) => void;
  }
  let { tab, onPercent, onEditApplied, editorAction, layoutKey, lineDefaults, multiCursor, findSettings, insertSettings, autoPairs, cleanupSettings, onTopRow, onSelectionStats, onCaretInfo }: Props = $props();

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
  /** 位置恢复代次：用户主动交互（滚轮/指针/触摸/按键）自增，用于中止进行中的恢复重试 */
  let scrollEpoch = 0;

  // ---------------------------------------------------------------------------
  // 过滤视图（P1-4 / D61：仅阅读模式；以稀疏虚拟列表渲染命中行）
  // ---------------------------------------------------------------------------
  /** 过滤条是否展开 */
  let filterOpen = $state(false);
  /** 查询文本（正则或字面量） */
  let filterText = $state('');
  let filterRegex = $state(false);
  let filterCase = $state(false);
  let filterHideEmpty = $state(false);
  /** 命中显示行号；null = 过滤未启用 */
  let filterRows = $state<number[] | null>(null);
  let filterTruncated = $state(false);
  let filterBusy = $state(false);
  let filterError = $state<string | null>(null);
  const filterActive = $derived(filterRows !== null);

  /** 显示行 → 文件行（过滤关闭时二者相同）。 */
  function fileRowOf(displayRow: number): number {
    const filter = filterRows;
    return filter ? (filter[displayRow] ?? 0) : displayRow;
  }

  /** 当前视图总行数（过滤启用后为命中数）。 */
  function viewRowsTotal(): number {
    return filterRows ? filterRows.length : tab.rowsTotal;
  }

  /** 过滤启用/清除后的视图基线重建（缓存、高度、窗口、滚动记忆统一重置）。 */
  function resetViewState(): void {
    cache.clear();
    heights.clear();
    inflight.clear();
    lastPercent = -1;
    startRow = 0;
    endRow = 0;
    if (container) setContentScrollTop(0);
    refreshWindow();
    version += 1;
  }

  /** 应用过滤（仅阅读模式）：扫描命中行后以稀疏虚拟列表渲染。 */
  async function applyFilter(): Promise<void> {
    if (tab.editing || filterBusy) return;
    filterBusy = true;
    filterError = null;
    const tabId = tab.tabId;
    const query: FilterQuery = {
      text: filterText.trim(),
      regex: filterRegex,
      caseSensitive: filterCase,
      hideEmpty: filterHideEmpty,
    };
    try {
      const result = await ipc.filterRows(tabId, query);
      if (tab.tabId !== tabId) return;
      filterRows = result.rows;
      filterTruncated = result.truncated;
      resetViewState();
      scrollMemory.set(tab.tabId, 0);
    } catch (error) {
      if (import.meta.env.DEV) console.error('[reader] 过滤失败', error);
      filterError = describeIpcError(toIpcError(error));
    } finally {
      filterBusy = false;
    }
  }

  /** 清除过滤，恢复全量视图。 */
  function clearFilter(): void {
    if (filterRows === null) return;
    filterRows = null;
    filterTruncated = false;
    filterError = null;
    resetViewState();
    scrollMemory.set(tab.tabId, 0);
  }

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

  /** 请求窗口内缺失的行（合并为连续批次；切换标签/过滤变化后丢弃过期结果）。
   *  过滤启用时：批次中的显示行经 `filterRows` 映射为文件行，经 `fetch_rows_at` 稀疏取回，
   *  并按「显示行」键写入缓存（虚拟列表与百分比均以显示行为准）。 */
  function ensureRows(start: number, end: number): void {
    const wanted: number[] = [];
    for (let row = start; row < end; row += 1) wanted.push(row);
    for (const batch of planBatches(wanted, (row) => cache.has(row), MAX_BATCH)) {
      const key = `${batch.start}:${batch.count}`;
      if (inflight.has(key)) continue;
      inflight.add(key);
      const tabId = tab.tabId;
      const filter = filterRows; // 快照：过滤结果变化后旧批次作废
      const fetch = filter
        ? ipc.fetchRowsAt(
            tabId,
            Array.from({ length: batch.count }, (_, index) => filter[batch.start + index] ?? 0),
          )
        : ipc.getRows(tabId, batch.start, batch.count).then((payload) => payload.rows);
      void fetch.then(
        (rows) => {
          inflight.delete(key);
          if (tab.tabId !== tabId || filterRows !== filter) return;
          rows.forEach((row, index) => {
            const displayRow = filter ? batch.start + index : row.row;
            cache.set(displayRow, {
              text: row.text,
              logicalRow: row.logicalRow ?? row.row,
              baseUtf16: row.baseUtf16 ?? 0,
            });
          });
          version += 1;
        },
        (error: unknown) => {
          inflight.delete(key);
          if (tab.tabId !== tabId || filterRows !== filter) return;
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

  /** 确保单行已加载（编辑层光标定位/复制使用；返回加载后的文本）。
   *  过滤启用时入参为显示行，内部映射到文件行（编辑模式下过滤恒为关闭，此处仅为防御）。 */
  async function ensureRow(row: number): Promise<string | undefined> {
    const cached = cache.get(row);
    if (cached !== undefined) return cached.text;
    const filter = filterRows;
    try {
      const fileRow = filter ? fileRowOf(row) : row;
      const rows = filter
        ? await ipc.fetchRowsAt(tab.tabId, [fileRow])
        : (await ipc.getRows(tab.tabId, fileRow, 1)).rows;
      const item = rows[0];
      if (item !== undefined) {
        cache.set(row, {
          text: item.text,
          logicalRow: item.logicalRow ?? fileRow,
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
    const rowsTotal = viewRowsTotal();
    const { start, end, anchorRow } = computeWindow(
      heights,
      rowsTotal,
      contentScrollTop(),
      el.clientHeight,
      OVERSCAN,
    );
    startRow = start;
    endRow = end;
    spacerTop = heights.offsetOf(start, rowsTotal);
    spacerBottom = Math.max(0, heights.totalHeight(rowsTotal) - heights.offsetOf(end, rowsTotal));
    ensureRows(start, end);
    // 过滤视图的显示行与文件行不是同一坐标，不写入滚动记忆（会话恢复始终针对文件行）
    if (filterRows === null) scrollMemory.set(tab.tabId, anchorRow);
    const percent = computePercent(heights, rowsTotal, anchorRow);
    if (Math.round(percent) !== lastPercent) {
      lastPercent = Math.round(percent);
      onPercent(percent);
    }
  }

  /** 上次回报的顶部行（去抖：仅在变化时回调）。 */
  let lastTopRow = -1;

  /** 计算并回报当前顶部行（过滤态映射回文件行；去抖）。 */
  function reportTopRow(): void {
    const topDisplayRow = heights.rowAtOffset(contentScrollTop(), Math.max(1, viewRowsTotal()));
    if (filterRows === null) {
      if (topDisplayRow !== lastTopRow) {
        lastTopRow = topDisplayRow;
        onTopRow?.(topDisplayRow + 1);
      }
    } else {
      const fileRow = filterRows[Math.min(topDisplayRow, filterRows.length - 1)] ?? 0;
      if (fileRow !== lastTopRow) {
        lastTopRow = fileRow;
        onTopRow?.(fileRow + 1);
      }
    }
  }

  // 初始/切换/过滤变化时上报顶部行（未发生滚动时状态栏也应显示行号）。
  $effect(() => {
    void tab.tabId;
    void tab.rowsTotal;
    void filterRows;
    reportTopRow();
  });

  /** 滚动处理（rAF 节流）：刷新窗口 + 实时更新滚动记忆（会话保存直接读取）。 */
  function handleScroll(): void {
    if (programmatic || scrollScheduled) return;
    scrollScheduled = true;
    requestAnimationFrame(() => {
      scrollScheduled = false;
      refreshWindow();
      // 实时记录顶部定位行（会话/标签切换共用数据源）；
      // 此前仅在切换标签的清理阶段记录，导致「滚动后直接退出」恢复不到位置。
      // 过滤视图下跳过（显示行 ≠ 文件行）。
      if (filterRows === null) {
        scrollMemory.set(
          tab.tabId,
          heights.rowAtOffset(contentScrollTop(), Math.max(1, viewRowsTotal())),
        );
      }
      reportTopRow();
    });
  }

  /** 测量已渲染行高，并做滚动锚定（窗口上方高度变化时保持视觉位置不跳）。 */
  function measureRendered(): void {
    const el = container;
    if (!el) return;
    const beforeScroll = contentScrollTop();
    const rowsTotal = viewRowsTotal();
    const anchorRow = heights.rowAtOffset(beforeScroll, rowsTotal);
    const anchorDelta = Math.max(0, beforeScroll - heights.offsetOf(anchorRow, rowsTotal));
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
    if (changed) {
      const corrected = heights.offsetOf(anchorRow, rowsTotal) + anchorDelta;
      if (Math.abs(corrected - beforeScroll) > 1) {
        programmatic = true;
        el.scrollTop = corrected + pagePadTop;
        requestAnimationFrame(() => {
          programmatic = false;
        });
      }
    }
    // 无论是否有新测量都重算占位：排版变更（clear 后）新旧行高差异可能小于测量阈值（0.5px），
    // 若此时提前返回，占位会停留在旧值——表现为「改完不立刻生效、滚动一次才落定」。
    spacerTop = heights.offsetOf(startRow, rowsTotal);
    spacerBottom = Math.max(
      0,
      heights.totalHeight(rowsTotal) - heights.offsetOf(endRow, rowsTotal),
    );
    if (changed) version += 1;
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

  // 用户主动交互（滚轮/拖拽/触摸/按键）→ 中止进行中的位置恢复重试（避免与其竞争）
  $effect(() => {
    const el = container;
    if (!el) return;
    const bump = (): void => {
      scrollEpoch += 1;
    };
    el.addEventListener('wheel', bump, { passive: true });
    el.addEventListener('pointerdown', bump);
    el.addEventListener('touchstart', bump, { passive: true });
    el.addEventListener('keydown', bump);
    return () => {
      el.removeEventListener('wheel', bump);
      el.removeEventListener('pointerdown', bump);
      el.removeEventListener('touchstart', bump);
      el.removeEventListener('keydown', bump);
    };
  });

  // 排版变更（字体/字号/行高/限宽/边距）：行高模型失效并重排；
  // 滚动位置由 measureRendered 的锚定机制保持（不会跳回顶部）。
  // 关键 1：version 的自增必须在 untrack 内——`version += 1` 同时读取并写入该 $state，
  //   若参与依赖收集会让本 effect 自触发形成死循环（实测每帧数千次，渲染管线被持续冲刷）。
  // 关键 2：CSS 变量应用与 DOM 重排存在先后——首轮测量可能拿到旧字号度量，
  //   导致下调字号后滚动高度偏大、需滚动一次才修正；因此两帧后再失效重测一次兜底。
  $effect(() => {
    void layoutKey;
    untrack(() => {
      heights.clear();
      version += 1;
    });
    let raf2 = 0;
    const raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => {
        untrack(() => {
          heights.clear();
          version += 1;
        });
      });
    });
    return () => {
      cancelAnimationFrame(raf1);
      cancelAnimationFrame(raf2);
    };
  });

  /** 已处理的工作区跳转序号（普通变量：跨渲染保留且不参与响应式依赖）。 */
  let handledJumpSeq = 0;

  // 工作区搜索跳转（P1-8b）：定位到目标显示行；与标签恢复共用滚动机制。
  // 过滤视图下显示行映射不同，跳转前先恢复全量视图。
  $effect(() => {
    const seq = jumpStore.seq;
    const target = jumpStore.tabId;
    if (seq === 0 || seq === handledJumpSeq || target !== tab.tabId) return;
    handledJumpSeq = seq;
    if (filterRows !== null) clearFilter();
    const rowCount = Math.max(1, tab.rowsTotal);
    const targetRow = Math.min(jumpStore.row, rowCount - 1);
    if (container) {
      applyInitialScroll(targetRow, rowCount);
    }
  });

  /** 应用初始滚动位置（会话恢复/切回长文档）。
   *  三个关键点：
   *  1. 先 refreshWindow() 建立占位高度与首批取行——此前直接赋值 scrollTop，
   *     在内容比视口矮（切换瞬间占位未重建/冷启动首屏未渲染）时被浏览器钳到 0，
   *     且触发逐帧重试循环，与用户随后的滚动相互竞争（表现为：滚动位置被反复拉回顶部，
   *     滚动记忆被写成 0，历史进度丢失）。
   *  2. 仅对第 >0 行重试（行高模型校准期间位置会漂移）；顶部无需恢复。
   *  3. 用户主动交互（滚轮/指针/触摸/按键）通过 scrollEpoch 中止重试。 */
  function applyInitialScroll(row: number, rowsTotal: number): void {
    refreshWindow();
    if (row <= 0) return;
    const epoch = scrollEpoch;
    let attempts = 0;
    const attempt = (): void => {
      if (!container || scrollEpoch !== epoch) return;
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
  // 说明：体调用的 refreshWindow 经 viewRowsTotal 读取过滤状态——若不放入 untrack，
  // 应用过滤时的 filterRows 赋值会反向触发本 effect 清空刚应用的过滤（真实缺陷，已修）。
  $effect(() => {
    const currentTabId = tab.tabId;
    const rowsTotal = tab.rowsTotal;
    void tab.encoding;
    untrack(() => {
      cache.clear();
      heights.clear();
      inflight.clear();
      lastPercent = -1;
      // 切换标签/编码：过滤状态随视图重建而清空（过滤结果为当前会话的一次性扫描）
      filterRows = null;
      filterTruncated = false;
      filterOpen = false;
      filterError = null;
      const restoredRow = Math.min(
        scrollMemory.get(currentTabId) ?? 0,
        Math.max(0, rowsTotal - 1),
      );
      if (container) {
        applyInitialScroll(restoredRow, rowsTotal);
      }
    });
    return () => {
      scrollMemory.set(
        currentTabId,
        heights.rowAtOffset(contentScrollTop(), Math.max(1, rowsTotal)),
      );
    };
  });

  // 进入编辑模式时关闭过滤（D61：过滤视图仅阅读模式）
  $effect(() => {
    if (!tab.editing) return;
    if (filterRows !== null) clearFilter();
    filterOpen = false;
  });
</script>

<div class="reader" bind:this={container} onscroll={handleScroll}>
  {#if !tab.editing && tab.rowsTotal > 0}
    <div class="filter-host">
      {#if filterActive && filterRows}
        <span class="filter-count" data-filter-count>
          {t('filter.count', { shown: filterRows.length, total: tab.rowsTotal })}{filterTruncated
            ? t('filter.truncated')
            : ''}
        </span>
        <button class="filter-btn" data-filter-clear onclick={clearFilter}>{t('filter.clear')}</button>
      {/if}
      <button
        class="filter-btn toggle"
        class:on={filterOpen}
        data-filter-toggle
        aria-expanded={filterOpen}
        onclick={() => {
          filterOpen = !filterOpen;
        }}
      >
        {t('filter.toggle')}
      </button>
      {#if filterOpen}
        <div class="filter-bar" data-filter-bar>
          <input
            class="filter-input"
            data-filter-input
            aria-label={t('filter.placeholder')}
            placeholder={t('filter.placeholder')}
            bind:value={filterText}
            onkeydown={(event) => {
              if (event.key === 'Enter') void applyFilter();
              if (event.key === 'Escape') filterOpen = false;
            }}
          />
          <button
            class="flag"
            class:on={filterRegex}
            data-filter-regex
            title={t('filter.regex')}
            onclick={() => {
              filterRegex = !filterRegex;
            }}>.*</button
          >
          <button
            class="flag"
            class:on={filterCase}
            data-filter-case
            title={t('filter.case')}
            onclick={() => {
              filterCase = !filterCase;
            }}>Aa</button
          >
          <label class="flag-label">
            <input type="checkbox" data-filter-hide-empty bind:checked={filterHideEmpty} />
            {t('filter.hideEmpty')}
          </label>
          <button class="filter-btn apply" data-filter-apply disabled={filterBusy} onclick={() => void applyFilter()}>
            {filterBusy ? t('filter.applying') : t('filter.apply')}
          </button>
          {#if filterError}
            <span class="filter-err" data-filter-error>{filterError}</span>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
  <div class="page">
    {#if filterActive && filterRows?.length === 0}
      <p class="empty-file" data-filter-no-match>{t('filter.noMatch')}</p>
    {:else if tab.rowsTotal === 0}
      <p class="empty-file">{t('reader.emptyFile')}</p>
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
        lineDefaults={lineDefaults ?? null}
        multiCursor={multiCursor ?? null}
        findSettings={findSettings ?? null}
        insertSettings={insertSettings ?? null}
        autoPairs={autoPairs ?? null}
        cleanupSettings={cleanupSettings ?? null}
        onSelectionStats={onSelectionStats ?? undefined}
        onCaretInfo={onCaretInfo ?? undefined}
      />
    {/if}
  </div>
</div>

<style>
  .reader {
    flex: 1;
    overflow-y: auto;
    background: var(--base);
    /* 禁用浏览器原生滚动锚定：本组件已自研锚定补偿（见 measureRendered），
     * 两者叠加会在「程序化远跳 + 窗口重建」时被浏览器二次调整（实测 100MB 文件
     * 首次跳转被放大数倍并形成反馈循环）；由自带补偿独立负责位置稳定。 */
    overflow-anchor: none;
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
    /* 排版扩展：文字对齐 / 首行缩进 / 段间距（值由 App.svelte 写入 CSS 变量） */
    text-align: var(--reading-align, left);
    text-indent: var(--reading-indent, 0);
    padding-bottom: var(--reading-para-spacing, 0);
  }

  .spacer {
    width: 100%;
  }

  .empty-file {
    margin: 30vh 0 0;
    text-align: center;
    color: var(--muted);
  }

  /* 过滤视图（P1-4）：阅读区顶部粘性条（不透明底，避免滚动内容透出） */
  .filter-host {
    position: sticky;
    top: 0;
    z-index: 3;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 8px;
    padding: 6px 12px;
    background: var(--base);
    border-bottom: 1px solid transparent;
  }

  .filter-host:has(.filter-bar) {
    border-bottom-color: var(--line);
  }

  .filter-count {
    font-size: 12px;
    color: var(--muted);
  }

  .filter-btn {
    height: 24px;
    padding: 0 10px;
    font-size: 12px;
    color: var(--ink);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 4px;
    cursor: pointer;
  }

  .filter-btn:hover {
    background: var(--hover);
  }

  .filter-btn.on,
  .filter-btn.apply {
    color: var(--accent);
    border-color: var(--accent);
  }

  .filter-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .filter-bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    flex: 1;
    justify-content: flex-end;
  }

  .filter-input {
    flex: 1;
    min-width: 160px;
    max-width: 360px;
    height: 24px;
    padding: 0 8px;
    font-size: 12px;
    color: var(--ink);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 4px;
  }

  .filter-bar .flag {
    height: 24px;
    min-width: 28px;
    font-size: 12px;
    color: var(--muted);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 4px;
    cursor: pointer;
  }

  .filter-bar .flag.on {
    color: var(--accent);
    border-color: var(--accent);
  }

  .flag-label {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
  }

  .filter-err {
    font-size: 12px;
    color: var(--danger, #c0392b);
  }
</style>
