<script lang="ts">
  // 阅读视图：虚拟滚动（仅渲染可视行 + 实测高度缓存 + 滚动锚定）+ 按窗取行。
  // 进度口径：状态栏百分比 = 顶部定位行的累计高度 / 内容总高度（视觉进度）。
  // 标签/编码切换：重建高度与缓存、按记忆行号恢复滚动位置（阶段 8 会话持久化同口径）。
  import { onDestroy, untrack } from 'svelte';

  import EditLayer from './EditLayer.svelte';
  import type { EditorAction } from '../edit/actions';
  import { t } from '../i18n/index.svelte';
  import {
    describeIpcError,
    ipc,
    toIpcError,
    type EditApplied,
    type FoldRegion,
    type FilterQuery,
    type TabInfo,
  } from '../ipc';
  import { breadcrumbChain, buildVisibleRows, hiddenIntervals } from '../reader/folds';
  import { HeightModel } from '../reader/heights';
  import { RowCache } from '../reader/row-cache';
  import { scrollMemory } from '../reader/scroll-memory';
  import { jumpStore } from '../state/jump.svelte';
  import { annotations } from '../state/annotations.svelte';
  import { computePercent, computeWindow, planBatches } from '../reader/viewport';
  import { toasts } from '../state/toasts.svelte';
  import type {
    AutoPairsSettings,
    CleanupSettings,
    DisplaySettings,
    EditorLinesSettings,
    FindSettings,
    InsertSettings,
    MultiCursorSettings,
  ReadingSettings,
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
  /** 阅读模式设置（P2-4b：专注/打字机/进度记忆；空对象=默认行为） */
  readingSettings?: ReadingSettings | null;
  /** 用户主动滚动回调（用于停止自动滚动） */
  onUserScroll?: () => void;
  /** 面包屑跳转（点击路径段） */
  onBreadcrumbJump?: (row: number) => void;
  /** 快照恢复请求（P3-1；seq 去重，由父组件触发） */
  snapshotRestore?: { name: string; seq: number } | null;
  /** 折叠指令（P2-6b：菜单折叠全部/展开全部；seq 去重） */
  foldCommand?: { kind: 'all' | 'none'; seq: number } | null;
  /** 时间戳插入设置（透传编辑层；未就绪为 null） */
  insertSettings?: InsertSettings | null;
  /** 括号匹配/自动缩进设置（透传编辑层；未就绪为 null） */
  autoPairs?: AutoPairsSettings | null;
  /** 清理类操作设置（透传编辑层；未就绪为 null） */
  cleanupSettings?: CleanupSettings | null;
    /** 状态栏：顶部可视行回报（1 基；P2-1） */
    onTopRow?: (row: number) => void;
    /** 翻页信号（App 快捷键；分屏路径消费） */
    pageTurn?: { seq: number; kind: 'up' | 'down' | 'top' | 'bottom' } | null;
    /** 状态栏：选区统计透传（编辑层；P2-1） */
    onSelectionStats?: (stats: TextStats | null) => void;
    /** 状态栏：光标行列透传（编辑层；P2-1） */
    onCaretInfo?: (info: { row: number; column: number }) => void;
    /** 显示选项（P2-2；未就绪为 null = 全部默认行为） */
    displaySettings?: DisplaySettings | null;
    /** 编辑态光标所在显示行（0 基；当前行高亮/相对行号参照；阅读态忽略） */
    editCaretRow?: number | null;
  }
  let { tab, onPercent, onEditApplied, editorAction, layoutKey, lineDefaults, multiCursor, findSettings, readingSettings,
    pageTurn = null, insertSettings, autoPairs, cleanupSettings, onTopRow, onSelectionStats, onCaretInfo, displaySettings, editCaretRow, onUserScroll, onBreadcrumbJump, snapshotRestore, foldCommand = null }: Props = $props();

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
  /** 在途取行批次键 → 发起时间（看门狗：响应偶发丢失时超时重试，防永久空白） */
  const inflight = new Map<string, number>();
  /** 在途批次看门狗超时（ms）：超过即视为丢失，允许重新取行
   *  （实测取行响应 <1ms；快速滚动下偶发请求未达后端，故设置较短超时保证可见区及时补齐） */
  const INFLIGHT_TIMEOUT_MS = 1200;
  /** 同时最多在途批次数（快速滚动时不淹没 IPC，降低消息丢失概率） */
  const MAX_INFLIGHT = 4;
  /** 延迟补取定时器（有被跳过的批次时兜底重试） */
  let ensureRetryTimer: ReturnType<typeof setTimeout> | null = null;
  function scheduleEnsureRetry(): void {
    if (ensureRetryTimer) return;
    ensureRetryTimer = setTimeout(() => {
      ensureRetryTimer = null;
      refreshWindow();
    }, 250);
  }
  onDestroy(() => {
    if (ensureRetryTimer) clearTimeout(ensureRetryTimer);
  });
  /** 标签 → 顶部定位行的注册表（跨模块共享；见 reader/scroll-memory.ts） */
  /** 程序化滚动标记（锚定补偿时避免重入滚动处理） */
  let programmatic = false;

  /** 最近一次真实用户输入时间（滚轮/指针/按键）。
   *  自动滚动等程序化写入不产生输入事件，借此区分「用户滚动」并回调 onUserScroll。 */
  let lastUserInputAt = 0;
  function markUserInput(): void {
    lastUserInputAt = performance.now();
  }
  let lastPercent = -1;
  /** 当前渲染窗口首行（阅读态当前行高亮/相对行号参照） */
  let windowStartRow = $state(0);

  // ---------- 显示选项（P2-2 V 组；null 时保持既有默认行为） ----------
  const disp = $derived(displaySettings ?? null);
  const showLn = $derived(disp?.lineNumbers ?? false);
  const relLn = $derived(disp?.relativeLineNumbers ?? false);
  const hlCurrent = $derived(disp?.highlightCurrentLine ?? false);
  const rulerOn = $derived(disp?.ruler ?? false);
  const guidesOn = $derived(disp?.indentGuides ?? false);
  const nlMark = $derived(disp?.invisible.includes('newline') ?? false);
  const markSpace = $derived(disp?.invisible.includes('space') ?? false);
  const markTab = $derived(disp?.invisible.includes('tab') ?? false);
  const markTrailing = $derived(disp?.invisible.includes('trailingSpace') ?? false);
  const invisibleOn = $derived(markSpace || markTab || markTrailing);
  const nowrap = $derived(!(disp?.wordWrap ?? true));
  const indentWidth = $derived(Math.max(1, lineDefaults?.indentWidth ?? 4));
  /** 行号列宽（按总行数位数近似；ch 单位） */
  const lnDigits = $derived(Math.max(2, String(Math.max(1, viewRowsTotal())).length));
  /** 当前行：编辑态 = 光标行；阅读态 = 窗口首行 */
  const currentRow = $derived(tab.editing ? (editCaretRow ?? -1) : windowStartRow);
  /** 不可见标记的逐行长度守卫（超长行跳过标记，保持流畅） */
  const MARK_LIMIT = 4000;

  /** 行号标签（相对模式：参照行显示绝对行号，其余显示距离）。 */
  function lnLabel(row: number): string {
    if (!relLn) return String(row + 1);
    const ref = currentRow;
    if (ref < 0 || row === ref) return String(row + 1);
    return String(Math.abs(row - ref));
  }

  /** 当前标签标注快照（书签/高亮/注释；未加载为 null）。 */
  const annData = $derived(annotations.forTab(tab.tabId));

  /** 行分段：按高亮区间切分已标记文本（不可见字符标记为 1:1 替换，偏移与逻辑 UTF-16 一致）。
   *  说明：`hl` 标记该段是否属于高亮（颜色为空时仍以主题默认色渲染，不能仅凭 color 判断）。 */
  function splitHighlights(main: string, row: number): Array<{ text: string; hl: boolean; color: string | null }> {
    const list = annData?.highlights;
    if (!list || list.length === 0) return [{ text: main, hl: false, color: null }];
    const inRow = list
      .filter((h) => h.row === row && h.endUtf16 > h.startUtf16)
      .sort((a, b) => a.startUtf16 - b.startUtf16);
    if (inRow.length === 0) return [{ text: main, hl: false, color: null }];
    const parts: Array<{ text: string; hl: boolean; color: string | null }> = [];
    let cursor = 0;
    for (const h of inRow) {
      const start = Math.max(cursor, Math.min(h.startUtf16, main.length));
      const end = Math.min(Math.max(h.endUtf16, start), main.length);
      if (start > cursor) parts.push({ text: main.slice(cursor, start), hl: false, color: null });
      if (end > start) parts.push({ text: main.slice(start, end), hl: true, color: h.color ?? null });
      cursor = Math.max(cursor, end);
    }
    if (cursor < main.length) parts.push({ text: main.slice(cursor), hl: false, color: null });
    return parts;
  }

  /** 行的标注标记（书签丝带 / 待办点 / 注释点）。 */
  function rowAnn(row: number): { bm: boolean; todo: boolean; note: boolean } {
    const data = annData;
    if (!data) return { bm: false, todo: false, note: false };
    return {
      bm: data.bookmarks.some((b) => b.row === row),
      todo: data.notes.some((n) => n.row === row && n.kind === 'todo' && !n.done),
      note: data.notes.some((n) => n.row === row && (n.kind !== 'todo' || n.done)),
    };
  }

  // 标注加载：标签切换（含首次显示）时按需拉取一次
  $effect(() => {
    const id = tab.tabId;
    if (id && !annotations.forTab(id)) void annotations.load(id);
  });

  /** 不可见字符标记（空格·/制表→/行尾空白切片；均为 1:1 替换，
   *  保证 DOM 索引与逻辑 UTF-16 偏移一致；¶ 在 .txt 之外追加不影响映射）。 */
  function markRow(text: string): { main: string; trailing: string } {
    if (!invisibleOn || text.length > MARK_LIMIT) return { main: text, trailing: '' };
    let body = text;
    let trailing = '';
    if (markTrailing) {
      const match = /[ \t]+$/u.exec(body);
      if (match) {
        trailing = match[0];
        body = body.slice(0, body.length - trailing.length);
      }
    }
    const apply = (value: string, isTrailing: boolean): string => {
      let out = value;
      if (markTab) out = out.replace(/\t/g, '→');
      if (markSpace) out = out.replace(/ /g, '·');
      else if (isTrailing && markTrailing) out = out.replace(/ /g, '␣');
      return out;
    };
    return { main: apply(body, false), trailing: apply(trailing, true) };
  }

  /** 缩进参考线位置（ch 单位；每 indentWidth 列一条，前列空白超出即止）。 */
  function guidePositions(text: string): number[] {
    if (!guidesOn) return [];
    let columns = 0;
    for (let i = 0; i < text.length && columns < 400; i += 1) {
      const ch = text[i];
      if (ch === ' ') columns += 1;
      else if (ch === '\t') columns += indentWidth - (columns % indentWidth);
      else break;
    }
    const positions: number[] = [];
    for (let col = indentWidth; col <= columns; col += indentWidth) positions.push(col);
    return positions;
  }
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
  // ---------- 折叠（P2-6b V-08：与过滤共用视图行通道；过滤优先，分屏路径暂不启用） ----------
  let foldRegions = $state<FoldRegion[] | null>(null);
  let foldedRows = $state<Set<number>>(new Set());
  const foldingMode = $derived(disp?.folding ?? 'off');
  const foldsActive = $derived.by(() => foldingMode !== 'off' && !spreadMode);
  const foldStarts = $derived(new Set((foldRegions ?? []).map((region) => region.startRow)));
  const foldVisibleRows = $derived.by(() => {
    if (!foldsActive || !foldRegions || foldedRows.size === 0) return null;
    const hidden = hiddenIntervals(foldRegions, foldedRows);
    if (hidden.length === 0) return null;
    return buildVisibleRows(tab.rowsTotal, hidden);
  });
  /** 视图行通道：过滤优先，其次折叠（显示行索引 → 文件行） */
  const viewRows = $derived(filterRows ?? foldVisibleRows);
  let filterTruncated = $state(false);
  let filterBusy = $state(false);
  let filterError = $state<string | null>(null);
  const filterActive = $derived(filterRows !== null);

  /** 显示行 → 文件行（过滤关闭时二者相同）。 */
  function fileRowOf(displayRow: number): number {
    const filter = viewRows;
    return filter ? (filter[displayRow] ?? 0) : displayRow;
  }

  /** 当前视图总行数（过滤启用后为命中数）。 */
  // 折叠状态随标签/编码切换重置（区间与折叠集属标签级；内容变更由 version 触发重载）
  $effect(() => {
    void tab.tabId;
    void tab.encoding;
    untrack(() => {
      foldedRows = new Set();
      foldRegions = null;
    });
  });

  // 折叠区间加载（模式/标签/修订变化；300ms 防抖；失败提示一次并停止折叠）
  let foldLoadSeq = 0;
  $effect(() => {
    const tabId = tab.tabId;
    void version;
    const active = foldsActive;
    if (!active) {
      foldRegions = null;
      return;
    }
    const seq = ++foldLoadSeq;
    const timer = setTimeout(() => {
      void (async () => {
        try {
          const regions = await ipc.foldRegions(tabId);
          if (seq === foldLoadSeq) foldRegions = regions;
        } catch (error) {
          if (seq === foldLoadSeq) {
            foldRegions = [];
            toasts.error(describeIpcError(toIpcError(error)));
          }
        }
      })();
    }, 300);
    return () => clearTimeout(timer);
  });

  // 折叠指令（菜单：折叠全部/展开全部）
  let foldCommandSeq = 0;
  $effect(() => {
    const command = foldCommand;
    if (!command || command.seq === foldCommandSeq) return;
    foldCommandSeq = command.seq;
    if (!foldRegions) return;
    foldedRows =
      command.kind === 'all'
        ? new Set(
            foldRegions
              .filter((region) => region.endRow > region.startRow)
              .map((region) => region.startRow),
          )
        : new Set();
    applyFoldChange();
  });

  /** 折叠切换后保持顶部行稳定并重排 */
  function applyFoldChange(): void {
    const topDisplay = heights.rowAtOffset(contentScrollTop(), viewRowsTotal());
    const anchorRow = viewRows ? (viewRows[Math.min(topDisplay, viewRows.length - 1)] ?? 0) : topDisplay;
    refreshWindow();
    void untrack(() => applyInitialScroll(anchorRow, viewRowsTotal()));
  }

  /** 切换单行折叠 */
  function toggleFold(row: number): void {
    const next = new Set(foldedRows);
    if (next.has(row)) next.delete(row);
    else next.add(row);
    foldedRows = next;
    applyFoldChange();
  }

  function viewRowsTotal(): number {
    return viewRows ? viewRows.length : tab.rowsTotal;
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
    // 看门狗：清掉超时未回的在途键（IPC 响应偶发丢失；不清将永久跳过该窗口）
    const now = Date.now();
    for (const [key, at] of inflight) {
      if (now - at > INFLIGHT_TIMEOUT_MS) inflight.delete(key);
    }
    const wanted: number[] = [];
    for (let row = start; row < end; row += 1) wanted.push(row);
    let deferred = false;
    for (const batch of planBatches(wanted, (row) => cache.has(row), MAX_BATCH)) {
      const key = `${batch.start}:${batch.count}`;
      if (inflight.has(key)) {
        deferred = true;
        continue;
      }
      if (inflight.size >= MAX_INFLIGHT) {
        deferred = true;
        break;
      }
      inflight.set(key, Date.now());
      const tabId = tab.tabId;
      const filter = viewRows; // 快照：视图行集变化后旧批次作废
      const fetch = filter
        ? ipc.fetchRowsAt(
            tabId,
            Array.from({ length: batch.count }, (_, index) => filter[batch.start + index] ?? 0),
          )
        : ipc.getRows(tabId, batch.start, batch.count).then((payload) => payload.rows);
      void fetch.then(
        (rows) => {
          inflight.delete(key);
          if (tab.tabId !== tabId || viewRows !== filter) return;
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
    // 有批次因在途/上限被跳过：稍后重试，保证窗口最终补齐
    if (deferred) scheduleEnsureRetry();
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
    // 编辑后锚点可能移动：防抖重读（后端按摘录重定位并回写校正）
    annotations.refreshSoon(tab.tabId);
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
    windowStartRow = start;
    spacerTop = heights.offsetOf(start, rowsTotal);
    spacerBottom = Math.max(0, heights.totalHeight(rowsTotal) - heights.offsetOf(end, rowsTotal));
    ensureRows(start, end);
    // 过滤视图的显示行与文件行不是同一坐标，不写入滚动记忆（会话恢复始终针对文件行）
    if (viewRows === null) scrollMemory.set(tab.tabId, anchorRow);
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
    if (viewRows === null) {
      if (topDisplayRow !== lastTopRow) {
        lastTopRow = topDisplayRow;
        bcTopRow = topDisplayRow;
        onTopRow?.(topDisplayRow + 1);
      }
    } else {
      const fileRow = viewRows![Math.min(topDisplayRow, viewRows!.length - 1)] ?? 0;
      if (fileRow !== lastTopRow) {
        lastTopRow = fileRow;
        bcTopRow = fileRow;
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
      if (viewRows === null && (readingSettings?.progressMemory ?? true)) {
        scrollMemory.set(
          tab.tabId,
          heights.rowAtOffset(contentScrollTop(), Math.max(1, viewRowsTotal())),
        );
      }
      // 用户主动滚动（滚轮/指针/按键后 400ms 内）：通知外部停止自动滚动
      if (performance.now() - lastUserInputAt < 400) onUserScroll?.();
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
      spreadViewport = Math.max(1, el.clientHeight);
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
    if (spreadMode) {
      spreadOffset = spreadStartOf(heights.offsetOf(targetRow, rowCount));
      reportSpreadTop();
    } else if (container) {
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
      // 打字机模式：程序化定位（跳转/恢复）时目标行保持视口中部
      const target = (readingSettings?.typewriter ?? false)
        ? Math.max(0, contentTop - (container.clientHeight - heights.heightOf(row)) / 2)
        : contentTop;
      setContentScrollTop(target);
      if (Math.abs(container.scrollTop - (target + pagePadTop)) <= 2 || attempts >= 120) {
        requestAnimationFrame(() => {
          refreshWindow();
          // 程序化定位同样需要上报顶部行（handleScroll 在 programmatic 期间被忽略）
          reportTopRow();
        });
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
      if (spreadMode) {
        spreadOffset = spreadStartOf(heights.offsetOf(restoredRow, Math.max(1, rowsTotal)));
        reportSpreadTop();
      } else if (container) {
        applyInitialScroll(restoredRow, rowsTotal);
      }
    });
    return () => {
      if (readingSettings?.progressMemory ?? true) {
        const offset = untrack(() => (spreadMode ? spreadOffset : contentScrollTop()));
        scrollMemory.set(currentTabId, heights.rowAtOffset(offset, Math.max(1, rowsTotal)));
      }
    };
  });

  // 进入编辑模式时关闭过滤（D61：过滤视图仅阅读模式）
  $effect(() => {
    if (!tab.editing) return;
    if (filterRows !== null) clearFilter();
    filterOpen = false;
  });

  // ---------- 分屏渲染路径（P2-4d：R-02 分栏 / R-10 翻页方式） ----------
  /** 翻页方式（scroll / paged / double） */
  function pageModeSetting(): 'scroll' | 'paged' | 'double' {
    const mode = readingSettings?.pageMode ?? 'scroll';
    return mode === 'paged' || mode === 'double' ? mode : 'scroll';
  }
  /** 每屏列数（双页恒 2；分栏取设置 1–2） */
  const spreadColumns = $derived(
    pageModeSetting() === 'double' ? 2 : Math.min(2, Math.max(1, readingSettings?.columns ?? 1)),
  );
  /** 是否分页（翻页动画；分栏滚动模式为瞬时整屏切换） */
  const pagedSpread = $derived(pageModeSetting() !== 'scroll');
  /** 是否走分屏渲染路径（编辑态恒走滚动路径） */
  const spreadMode = $derived(!tab.editing && (spreadColumns > 1 || pagedSpread));
  let spreadOffset = $state(0);
  let spreadViewport = $state(0);
  let spreadTween: number | undefined;
  let wheelAcc = 0;
  let handledTurnSeq = 0;

  /** 每屏内容跨度 = 视口内容高 × 列数 */
  function spreadSpan(): number {
    return Math.max(1, spreadViewport) * spreadColumns;
  }
  function spreadStartOf(offset: number): number {
    const span = spreadSpan();
    return Math.max(0, Math.round(offset / span) * span);
  }
  /** 当前顶部行的文件行号（含过滤映射；1 基由调用方加） */
  function topRowFromOffset(offset: number): number | null {
    const total = Math.max(1, viewRowsTotal());
    const display = heights.rowAtOffset(offset, total);
    return viewRows ? (viewRows[display] ?? null) : display;
  }
  const spreadCols = $derived.by(() => {
    void version;
    void spreadOffset;
    void spreadColumns;
    void spreadViewport;
    if (!spreadMode) return [];
    const total = Math.max(1, viewRowsTotal());
    const height = Math.max(1, spreadViewport);
    const cols: { start: number; rows: { row: number; top: number; text: string }[] }[] = [];
    for (let index = 0; index < spreadColumns; index += 1) {
      const start = spreadOffset + index * height;
      const rows: { row: number; top: number; text: string }[] = [];
      if (start < heights.totalHeight(total)) {
        let row = heights.rowAtOffset(start, total);
        while (row < total) {
          const top = heights.offsetOf(row, total) - start;
          if (top >= height) break;
          rows.push({ row, top, text: cache.get(row)?.text ?? '' });
          row += 1;
        }
      }
      cols.push({ start, rows });
    }
    return cols;
  });

  /** 分屏路径：取数 + 逐帧测量（测量即写高度模型，version 驱动重排） */
  $effect(() => {
    if (!spreadMode) return;
    const cols = spreadCols;
    let min = Number.POSITIVE_INFINITY;
    let max = -1;
    for (const col of cols) {
      for (const item of col.rows) {
        if (item.row < min) min = item.row;
        if (item.row > max) max = item.row;
      }
    }
    if (max >= 0) ensureRows(min, max + 1);
    requestAnimationFrame(() => measureSpread());
  });

  function measureSpread(): void {
    if (!container || !spreadMode) return;
    let changed = false;
    for (const el of container.querySelectorAll<HTMLElement>('.row[data-row]')) {
      const row = Number(el.dataset.row);
      const height = el.getBoundingClientRect().height;
      if (height > 0 && Math.abs(heights.heightOf(row) - height) > 0.5) {
        heights.measure(row, height);
        changed = true;
      }
    }
    if (changed) version += 1;
  }

  /** 报告分屏当前顶部行与百分比（状态栏） */
  function reportSpreadTop(): void {
    const total = Math.max(1, viewRowsTotal());
    const fileRow = topRowFromOffset(spreadOffset);
    if (fileRow !== null) onTopRow?.(fileRow + 1);
    const totalHeight = heights.totalHeight(total);
    const percent = totalHeight > 0 ? (spreadOffset / totalHeight) * 100 : 0;
    const rounded = Math.round(percent);
    if (rounded !== lastPercent) {
      lastPercent = rounded;
      onPercent(percent);
    }
  }

  /** 翻屏（到边界不动作） */
  function turnSpread(dir: 1 | -1): void {
    if (!spreadMode || spreadTween !== undefined) return;
    const span = spreadSpan();
    const total = Math.max(1, viewRowsTotal());
    const maxOffset = Math.max(0, Math.ceil(heights.totalHeight(total) / span) * span - span);
    const target = Math.min(maxOffset, Math.max(0, spreadOffset + dir * span));
    animateSpreadTo(target);
  }

  /** 滚轮翻屏（阈值累积防误触；双栏滚动模式瞬时） */
  function handleSpreadWheel(event: WheelEvent): void {
    if (!spreadMode) return;
    event.preventDefault();
    if (spreadTween !== undefined) return;
    wheelAcc += event.deltaY;
    if (Math.abs(wheelAcc) >= 40) {
      turnSpread(wheelAcc > 0 ? 1 : -1);
      wheelAcc = 0;
    }
  }

  function animateSpreadTo(target: number): void {
    if (spreadTween !== undefined) cancelAnimationFrame(spreadTween);
    const from = spreadOffset;
    const ms = pagedSpread ? Math.max(0, readingSettings?.pageAnimMs ?? 320) : 0;
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (ms === 0 || reduce) {
      spreadOffset = target;
      reportSpreadTop();
      return;
    }
    const start = performance.now();
    const step = (now: number): void => {
      const k = Math.min(1, (now - start) / ms);
      const eased = 1 - (1 - k) ** 3;
      spreadOffset = Math.round(from + (target - from) * eased);
      if (k < 1) {
        spreadTween = requestAnimationFrame(step);
      } else {
        spreadTween = undefined;
        reportSpreadTop();
      }
    };
    spreadTween = requestAnimationFrame(step);
  }

  /** App 翻页信号（PgUp/PgDn/Home/End 在分屏路径的一致行为） */
  $effect(() => {
    const signal = pageTurn;
    if (!signal || signal.seq === handledTurnSeq) return;
    handledTurnSeq = signal.seq;
    if (!spreadMode) return;
    if (signal.kind === 'top') {
      spreadOffset = 0;
      reportSpreadTop();
    } else if (signal.kind === 'bottom') {
      const span = spreadSpan();
      const total = Math.max(1, viewRowsTotal());
      spreadOffset = Math.max(0, Math.ceil(heights.totalHeight(total) / span) * span - span);
      reportSpreadTop();
    } else {
      turnSpread(signal.kind === 'down' ? 1 : -1);
    }
  });
  // ---------- 面包屑（P2-6c V-10：顶行所在章节路径；仅阅读滚动模式） ----------
  let outlineData = $state<{ row: number; level: number; title: string }[]>([]);
  const bcActive = $derived.by(
    () => (disp?.breadcrumb ?? true) && !tab.editing && !spreadMode && outlineData.length > 0,
  );
  /** 顶部文件行（0 基，随滚动更新；面包屑依据） */
  let bcTopRow = $state(0);
  const bcChain = $derived(breadcrumbChain(outlineData, bcTopRow));
  let outlineSeq = 0;
  $effect(() => {
    const enabled = disp?.breadcrumb ?? true;
    void tab.tabId;
    void version;
    if (!enabled || tab.editing) {
      outlineData = [];
      return;
    }
    const seq = ++outlineSeq;
    void ipc
      .outlineItems(tab.tabId)
      .then((items) => {
        if (seq === outlineSeq) outlineData = items;
      })
      .catch(() => {
        if (seq === outlineSeq) outlineData = [];
      });
  });

</script>

{#snippet rowMarkup(item: { row: number; text: string }, absolute: boolean, top: number)}
  {@const marked = markRow(item.text)}
  {@const ann = rowAnn(item.row)}
  <div class="row" data-row={item.row} class:current={hlCurrent && item.row === currentRow} style={absolute ? `top: ${top}px` : undefined}>{#if ann.bm}<span class="bmark" aria-hidden="true"></span>{/if}{#if showLn}<span class="ln" aria-hidden="true" style="width: calc({lnDigits}ch + 12px)">{lnLabel(item.row)}</span>{/if}{#if foldsActive && foldStarts.has(item.row)}<button class="fold-mark" class:folded={foldedRows.has(item.row)} data-fold-mark={item.row} aria-label={t('fold.toggle')} onpointerdown={(event) => event.stopPropagation()} onclick={(event) => { event.stopPropagation(); toggleFold(item.row); }}></button>{/if}<span class="txt">{#each splitHighlights(marked.main, item.row) as seg, i (i)}{#if seg.hl}<span class="hl" style={seg.color ? `--hl-color: ${seg.color}` : undefined}>{seg.text}</span>{:else}{seg.text}{/if}{/each}{#if marked.trailing}<span class="ts">{marked.trailing}</span>{/if}{#if nlMark}<span class="nl" aria-hidden="true">¶</span>{/if}</span>{#if ann.todo || ann.note}<span class="nmark" class:todo={ann.todo} aria-hidden="true"></span>{/if}{#if guidesOn}{#each guidePositions(item.text) as col (col)}<span class="guide" aria-hidden="true" style="left: {col}ch"></span>{/each}{/if}</div>
{/snippet}

<svelte:window onkeydown={markUserInput} />
<!-- svelte-ignore a11y_no_static_element_interactions -- 滚动容器仅用于「区分用户滚动」的输入标记，不引入交互语义 -->
<div class="reader" class:spread={spreadMode} bind:this={container} onscroll={handleScroll} onwheel={handleSpreadWheel} onpointerdown={markUserInput} ontouchstart={markUserInput}>
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
  {#if spreadMode}
    <div class="spread" data-spread>
      {#if filterActive && filterRows?.length === 0}
        <p class="empty-file" data-filter-no-match>{t('filter.noMatch')}</p>
      {:else if tab.rowsTotal === 0}
        <p class="empty-file">{t('reader.emptyFile')}</p>
      {:else}
        {#each spreadCols as col, ci (ci)}
          <div class="spread-col" data-spread-col={ci}>
            {#each col.rows as item (item.row)}{@render rowMarkup(item, true, item.top)}{/each}
          </div>
        {/each}
      {/if}
    </div>
  {:else}
  <div class="page" class:nowrap class:editing={tab.editing}>
    {#if bcActive && bcChain.length > 0}
      <nav class="breadcrumb" data-breadcrumb aria-label={t('outline.title')}>
        {#each bcChain as item (item.row)}
          <button class="crumb" data-breadcrumb-seg={item.row} onclick={() => onBreadcrumbJump?.(item.row)}>{item.title}</button><span class="crumb-sep" aria-hidden="true">›</span>
        {/each}
      </nav>
    {/if}
    {#if rulerOn}
      <!-- 标尺（P2-2 V-05）：位置相对正文列左缘（px），仅视觉参考 -->
      <div
        class="ruler"
        data-ruler
        aria-hidden="true"
        style="left: calc(var(--reading-pad-left) + {disp?.rulerPosition ?? 0}px)"
      ></div>
    {/if}
    {#if filterActive && filterRows?.length === 0}
      <p class="empty-file" data-filter-no-match>{t('filter.noMatch')}</p>
    {:else if tab.rowsTotal === 0}
      <p class="empty-file">{t('reader.emptyFile')}</p>
    {:else}
      <div class="spacer" style="height: {spacerTop}px"></div>
      {#each renderedRows as item (item.row)}
        {@const marked = markRow(item.text)}
        {@const ann = rowAnn(item.row)}
        <div class="row" data-row={item.row} class:current={hlCurrent && item.row === currentRow}>{#if ann.bm}<span class="bmark" aria-hidden="true"></span>{/if}{#if showLn}<span class="ln" aria-hidden="true" style="width: calc({lnDigits}ch + 12px)">{lnLabel(item.row)}</span>{/if}{#if foldsActive && foldStarts.has(item.row)}<button class="fold-mark" class:folded={foldedRows.has(item.row)} data-fold-mark={item.row} aria-label={t('fold.toggle')} onpointerdown={(event) => event.stopPropagation()} onclick={(event) => { event.stopPropagation(); toggleFold(item.row); }}></button>{/if}<span class="txt">{#each splitHighlights(marked.main, item.row) as seg, i (i)}{#if seg.hl}<span class="hl" style={seg.color ? `--hl-color: ${seg.color}` : undefined}>{seg.text}</span>{:else}{seg.text}{/if}{/each}{#if marked.trailing}<span class="ts">{marked.trailing}</span>{/if}{#if nlMark}<span class="nl" aria-hidden="true">¶</span>{/if}</span>{#if ann.todo || ann.note}<span class="nmark" class:todo={ann.todo} aria-hidden="true"></span>{/if}{#if guidesOn}{#each guidePositions(item.text) as col (col)}<span class="guide" aria-hidden="true" style="left: {col}ch"></span>{/each}{/if}</div>
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
        snapshotRestore={snapshotRestore ?? null}
        insertSettings={insertSettings ?? null}
        autoPairs={autoPairs ?? null}
        cleanupSettings={cleanupSettings ?? null}
        onSelectionStats={onSelectionStats ?? undefined}
        onCaretInfo={onCaretInfo ?? undefined}
      />
    {/if}
  </div>
  {/if}
</div>

<style>
  /* 编辑模式：改用编辑专用边距（四向独立，设置中分开展示） */
  .page.editing {
    padding: var(--edit-pad-top) var(--edit-pad-right) var(--edit-pad-bottom) var(--edit-pad-left);
  }

  .reader {
    flex: 1;
    overflow-y: auto;
    background: var(--base);
    /* 禁用浏览器原生滚动锚定：本组件已自研锚定补偿（见 measureRendered），
     * 两者叠加会在「程序化远跳 + 窗口重建」时被浏览器二次调整（实测 100MB 文件
     * 首次跳转被放大数倍并形成反馈循环）；由自带补偿独立负责位置稳定。 */
    overflow-anchor: none;
  }

  /* 分屏路径（P2-4d：分页/双页/双栏）：列内行绝对定位，滚轮整屏切换 */
  .reader.spread {
    position: relative;
    overflow: hidden;
  }

  .spread {
    position: absolute;
    inset: 0;
    display: flex;
    gap: var(--reading-col-gap, 48px);
    padding: 0 var(--reading-pad-right) 0 var(--reading-pad-left);
    font-family: var(--font-reading);
    font-size: var(--reading-size);
    line-height: var(--reading-line-height);
    color: var(--ink);
  }

  .spread-col {
    position: relative;
    flex: 1 1 0;
    min-width: 0;
    overflow: hidden;
  }

  .spread-col .row {
    position: absolute;
    left: 0;
    right: 0;
    white-space: pre-wrap;
    overflow-wrap: break-word;
  }

  .page {
    position: relative;
    z-index: 0;
    max-width: var(--reading-width);
    margin: 0 auto;
    padding: var(--reading-pad-top) var(--reading-pad-right) var(--reading-pad-bottom)
      var(--reading-pad-left);
    font-family: var(--font-reading);
    font-size: var(--reading-size);
    line-height: var(--reading-line-height);
    color: var(--ink);
  }

  /* 自动换行关闭（P2-2 V-04）：单行不折行，容器横向滚动 */
  .page.nowrap {
    max-width: none;
    width: max-content;
    min-width: 100%;
    margin: 0;
  }
  .page.nowrap .row {
    white-space: pre;
  }

  .row {
    position: relative;
    white-space: pre-wrap;
    overflow-wrap: break-word;
    min-height: calc(var(--reading-line-height) * 1em);
    /* 排版扩展：文字对齐 / 首行缩进 / 段间距（值由 App.svelte 写入 CSS 变量） */
    text-align: var(--reading-align, left);
    text-indent: var(--reading-indent, 0);
    padding-bottom: var(--reading-para-spacing, 0);
  }

  /* 当前行高亮（P2-2 V-03） */
  .row.current {
    background: var(--hover);
  }

  /* 行号（P2-2 V-01/V-02）：不可选中、不参与复制（导出/复制仍走引擎文本） */
  .ln {
    display: inline-block;
    text-align: right;
    padding-right: 12px;
    color: var(--muted);
    opacity: 0.75;
    user-select: none;
  }

  /* 文本容器：仅结构标记（不可加 position/z-index——会盖住 edit-surface
     导致鼠标定位与修饰键点击失效） */
  .fold-mark {
    /* 三角由伪元素绘制：按钮内不放文本节点（编辑层 textAt 遍历文本节点定位字符） */
    flex: none;
    position: relative;
    z-index: 7;
    width: 1.1em;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--muted);
    font-size: inherit;
    font-family: inherit;
    line-height: inherit;
    cursor: pointer;
  }
  .fold-mark::before {
    content: '▾';
  }
  .fold-mark.folded::before {
    content: '▸';
  }
  .fold-mark:hover {
  color: var(--accent);
}
.breadcrumb {
  position: sticky;
  top: 0;
  z-index: 6;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 2px;
  padding: 3px 6px;
  margin-bottom: 6px;
  background: var(--base);
  border-bottom: 1px solid var(--line);
  font-size: 12px;
  color: var(--muted);
}
.crumb {
  border: none;
  background: none;
  color: var(--muted);
  font: inherit;
  cursor: pointer;
  padding: 0 2px;
}
.crumb:hover {
  color: var(--accent);
  text-decoration: underline;
}
.crumb-sep:last-child {
  display: none;
}
  .txt {
    display: inline;
  }

  /* 行尾空白标记（P2-2 V-07）与换行标记 */
  .ts {
    color: var(--accent);
  }
  .nl {
    color: var(--muted);
    opacity: 0.7;
  }

  /* 缩进参考线（P2-2 V-06） */
  .guide {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--line);
    pointer-events: none;
  }

  /* 标尺（P2-2 V-05） */
  .ruler {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--accent);
    opacity: 0.35;
    pointer-events: none;
    z-index: 0;
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

  /* 标注渲染（P2-3）：书签丝带 / 高亮 / 注释点；绝对定位、不参与布局与文本 */
  .bmark {
    position: absolute;
    left: -8px;
    top: 3px;
    bottom: 3px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
    pointer-events: none;
  }

  .hl {
    background: color-mix(in srgb, var(--hl-color, var(--accent)) 30%, transparent);
    border-radius: 2px;
  }

  .nmark {
    position: absolute;
    right: -10px;
    top: 9px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    pointer-events: none;
  }

  .nmark.todo {
    background: var(--warning, #8a5200);
  }
</style>
