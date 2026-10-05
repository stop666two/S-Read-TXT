<!--
  EditLayer — 编辑交互层。
  职责：光标/选区叠加层渲染、隐藏输入框承接键入与 IME、键盘与鼠标交互、
        编辑操作下发（每次输入/按键 = 单个撤销步）。
  坐标：位置 = (行号, 行内 UTF-16 偏移)，与编辑引擎一致；叠加层坐标相对 .page。
-->
<script lang="ts">
  import { tick, untrack } from 'svelte';

  import { readText, writeHtml, writeText } from '@tauri-apps/plugin-clipboard-manager';

  import { describeIpcError, ipc, toIpcError, type BatchNumberingConfig, type BatchPreview, type ClipboardEntry, type EditApplied, type FindHit, type FindScope, type FindSettings, type ReplacePreview, type SearchMode, type NoteKind } from '../ipc';
  import {
    clampPos,
    collapsed,
    isCollapsed,
    moveEnd,
    moveHome,
    moveLeft,
    moveRight,
    moveVertical,
    orderedSelection,
    UTF16_END,
    type CaretPos,
    type Selection,
  } from '../edit/caret';
  import { caretMemory } from '../edit/caret-memory';
  import { focusEditorProxy } from '../edit/focus';
  import { jumpStore } from '../state/jump.svelte';
  import { planBackspace, planDeleteForward, type SegMeta } from '../edit/longline';
  import {
    clampRectRows,
    normalizeRect,
    rectBackspaceSpans,
    rectDeleteSpans,
    rectIsCollapsed,
    rectSpans,
    toggleCaret,
    uniquePositions,
    type RectRange,
  } from '../edit/multi';
  import { deleteOp, deleteRangeOp, insertOp, replaceOp } from '../edit/ops';
  import { t } from '../i18n/index.svelte';
  import { toasts } from '../state/toasts.svelte';
  import FindBar from './FindBar.svelte';
  import ReplacePreviewDialog from './ReplacePreviewDialog.svelte';
  import BatchNumberingDialog from './BatchNumberingDialog.svelte';
  import LineOpsDialog from './LineOpsDialog.svelte';
import ClipboardHistoryDialog from './ClipboardHistoryDialog.svelte';
import NoteDialog from './NoteDialog.svelte';
import { annotations } from '../state/annotations.svelte';
  import type {
    AutoPairsSettings,
    CleanupSettings,
    EditOp,
    EditorLinesSettings,
    InsertSettings,
    LineOpConfig,
    LineOpOutcome,
    LineOpPreview,
    MultiCursorSettings,
    TimestampFormat,
    TextStats,
  } from '../ipc';
  import type { EditActionType, EditorAction } from '../edit/actions';

  interface Props {
    /** 当前标签 id */
    tabId: number;
    /** 总行数（编辑后随父组件更新） */
    rowsTotal: number;
    /** 渲染版本号（文本到达/测量变化时驱动叠加层重算） */
    revision: number;
    /** 快照恢复请求（seq 去重，由父组件触发） */
    snapshotRestore?: { name: string; seq: number } | null;
    /** 已渲染行节点查询 */
    rowNode: (row: number) => HTMLElement | undefined;
    /** 行文本查询（可能未加载） */
    rowText: (row: number) => string | undefined;
    /** 行分段元数据查询（编辑态超长行：逻辑行号与段基 UTF-16；普通行可缺省） */
    rowMeta: (row: number) => { logicalRow: number; baseUtf16: number } | undefined;
    /** 确保行文本已加载（返回加载后的文本） */
    ensureRow: (row: number) => Promise<string | undefined>;
    /** 滚动容器查询 */
    getContainer: () => HTMLElement | null;
    /** 编辑结果回报（父组件失效缓存、刷新并同步标签信息） */
    onApplied: (result: EditApplied) => void;
    /** 外部编辑动作信号（菜单触发；seq 变化表示一次新动作） */
    editorAction?: EditorAction | null;
    /** 行操作默认值（编辑器设置；未就绪为 null 时使用兜底常量） */
    lineDefaults?: EditorLinesSettings | null;
    /** 多光标设置（编辑器设置；未就绪为 null 时使用兜底常量） */
    multiCursor?: MultiCursorSettings | null;
    /** 查找设置（未就绪为 null 时使用兜底常量） */
    findSettings?: FindSettings | null;
    /** 时间戳插入设置（未就绪为 null 时使用兜底常量） */
    insertSettings?: InsertSettings | null;
    /** 括号匹配/自动缩进设置（未就绪为 null 时使用兜底常量） */
    autoPairs?: AutoPairsSettings | null;
    /** 清理类操作设置（未就绪为 null 时使用兜底常量） */
    cleanupSettings?: CleanupSettings | null;
    /** 状态栏：选区统计回报（无选区/失败为 null） */
    onSelectionStats?: (stats: TextStats | null) => void;
    /** 状态栏：光标行列回报（1 基） */
    onCaretInfo?: (info: { row: number; column: number }) => void;
  }
  let {
    tabId,
    rowsTotal,
    revision,
    rowNode,
    rowText,
    rowMeta,
    ensureRow,
    getContainer,
    snapshotRestore,
    onApplied,
    editorAction,
    lineDefaults,
    multiCursor,
    findSettings,
    insertSettings,
    autoPairs,
    cleanupSettings,
    onSelectionStats,
    onCaretInfo,
  }: Props = $props();

  /** 叠加层盒子（相对 .page 的像素坐标） */
  interface Box {
    left: number;
    top: number;
    height: number;
    width: number;
  }

  /** 光标滚动入视区时的安全边距（px） */
  const SCROLL_MARGIN = 48;
  /** 复制行数上限（v1 保护；超大选区提示分段复制） */
  const COPY_MAX_ROWS = 50_000;
  /** 复制字符上限（防单行长行复制出超长字符串卡顿） */
  const COPY_MAX_CHARS = 5_000_000;

  // svelte-ignore state_referenced_locally
  // （仅取初值：挂载时从跨标签记忆恢复一次，之后不再依赖 tabId 初值）
  let selection = $state<Selection>(collapsed(caretMemory.get(tabId) ?? { row: 0, utf16: 0 }));

  /** 已处理的工作区跳转序号（普通变量：不参与响应式依赖）。 */
  let handledJumpSeq = 0;

  /** 选区/光标回报（状态栏）：光标即时上报；选区 250ms 防抖统计。 */
  let statsTimer: ReturnType<typeof setTimeout> | null = null;
  let statsSeq = 0;
  $effect(() => {
    const sel = orderedSelection(selection);
    onCaretInfo?.({ row: selection.head.row + 1, column: selection.head.utf16 + 1 });
    if (isCollapsed(selection)) {
      if (statsTimer) clearTimeout(statsTimer);
      statsTimer = null;
      onSelectionStats?.(null);
      return;
    }
    const seq = ++statsSeq;
    if (statsTimer) clearTimeout(statsTimer);
    statsTimer = setTimeout(() => {
      statsTimer = null;
      void ipc
        .selectionStats(tabId, sel.start.row, sel.start.utf16, sel.end.row, sel.end.utf16)
        .then((stats) => {
          if (seq === statsSeq) onSelectionStats?.(stats);
        })
        .catch(() => {
          if (seq === statsSeq) onSelectionStats?.(null);
        });
    }, 250);
  });

  // 工作区搜索跳转：等待目标行加载后设置选区并移交键盘焦点
  // （滚动由 ReaderView 同序号请求统一处理）。
  $effect(() => {
    const seq = jumpStore.seq;
    const target = jumpStore.tabId;
    if (seq === 0 || seq === handledJumpSeq || target !== tabId) return;
    handledJumpSeq = seq;
    const row = jumpStore.row;
    void (async () => {
      await ensureRow(row);
      selection = {
        anchor: { row, utf16: jumpStore.from },
        head: { row, utf16: jumpStore.to },
      };
      focusEditorProxy();
    })();
  });

  let composing = $state(false);
  let preedit = $state('');
  let caretBox = $state<Box | null>(null);
  let selBoxes = $state<Box[]>([]);
  let textarea = $state<HTMLTextAreaElement | null>(null);
  /** 拖选进行中 */
  let dragging = false;
  /** 垂直移动的列目标（水平移动/点击时清除） */
  let goalUtf16: number | null = null;
  /** 查找条开关（查找/替换两种形态） */
  let findOpen = $state(false);
  let replaceOpen = $state(false);
  /** 查找条聚焦信号（每次打开自增，通知 FindBar 重新聚焦输入框） */
  let findFocusSeq = $state(0);
  /** 最近一次已处理的外部动作序号（去重） */
  let lastActionSeq = -1;
  /** 文档高亮单次最多渲染的命中数（可见窗口内足够；防止海量命中拖慢渲染） */
  const MATCH_HIGHLIGHT_MAX = 800;
  /** 文档内全部命中高亮盒（当前可见行窗口内） */
  let matchBoxes = $state<Box[]>([]);
  /** 括号配对高亮盒（空=无高亮）。 */
  let bracketBoxes = $state<Box[]>([]);
  /** 当前括号配对结果（[光标侧, 配对侧] 显示坐标；null=无）。 */
  let bracketMatch = $state<CaretPos[] | null>(null);
  /** 查找设置兜底（与 Rust 默认一致）。 */
  const FALLBACK_FIND: FindSettings = {
    caseSensitive: false,
    wholeWord: false,
    wrapAround: true,
    highlightAll: true,
    matchCount: true,
    replacePreview: true,
    multifileEnabled: true,
    multifileConcurrency: 4,
    defaultScope: 'document',
    historyLimit: 50,
    highlightColor: '',
  };
  /** 生效的查找设置。 */
  const findDefaults = $derived(findSettings ?? FALLBACK_FIND);
  /** 查找历史（最新在前；空数组 = 无历史） */
  let findHistory = $state<string[]>([]);
  /** 命中计数文案（空串 = 不显示） */
  let findCountText = $state('');
  /** 计数请求序号（丢弃过期响应） */
  let countSeq = 0;
  /** 最近一次查询条件（FindBar 上报；驱动高亮与计数刷新） */
  let liveQuery = $state<{
    query: string;
    caseSensitive: boolean;
    mode: SearchMode;
    wholeWord: boolean;
    scope: FindScope;
    range: [number, number] | null;
  }>({
    query: '',
    caseSensitive: false,
    mode: 'literal',
    wholeWord: false,
    scope: 'document',
    range: null,
  });
  /** 高亮刷新防抖定时器 */
  let matchTimer: ReturnType<typeof setTimeout> | null = null;
  /** 高亮请求序号（丢弃过期响应） */
  let matchSeq = 0;
  /** 全部替换预览数据（二次确认弹窗；null = 未激活） */
  let previewData = $state<ReplacePreview | null>(null);
  /** 预览对应的查询参数（执行时回传） */
  let previewArgs: {
    query: string;
    replacement: string;
    caseSensitive: boolean;
    mode: SearchMode;
    wholeWord: boolean;
  } | null = null;
  /** 预览弹窗开关 */
  let previewOpen = $state(false);

  /** 批量序号弹窗开关 */
  let batchOpen = $state(false);

  /** 行操作弹窗开关 */
  let lineOpsOpen = $state(false);
  let noteDialog = $state<{ kind: 'note' | 'todo' } | null>(null);
  /** 剪贴板历史弹窗与列表。 */
let clipboardOpen = $state(false);
let clipboardEntries = $state<ClipboardEntry[]>([]);

  /** 多光标设置兜底（与 Rust 默认一致）。 */
  const FALLBACK_MULTI: MultiCursorSettings = { enabled: true, rectModifier: 'alt', maxCount: 1000 };

  /** 生效的多光标设置。 */
  const multi = $derived(multiCursor ?? FALLBACK_MULTI);

  /** 时间戳插入设置兜底（与 Rust 默认一致）。 */
  const FALLBACK_INSERT: InsertSettings = { timestampFormat: 'localDateTime' };

  /** 生效的时间戳插入设置。 */
  const insertCfg = $derived(insertSettings ?? FALLBACK_INSERT);

  /** 括号匹配/自动缩进设置兜底（与 Rust 默认一致）。 */
  const FALLBACK_AUTO_PAIRS: AutoPairsSettings = {
    enabled: true,
    autoClose: true,
    autoIndent: true,
    highlightMatch: true,
  };

  /** 生效的括号匹配设置。 */
  const autoPairsCfg = $derived(autoPairs ?? FALLBACK_AUTO_PAIRS);

  /** 清理设置兜底（与 Rust 默认一致）。 */
  const FALLBACK_CLEANUP: CleanupSettings = {
    trailingWhitespace: true,
    collapseBlankLines: true,
    trailingNewline: true,
  };

  /** 生效的清理设置。 */
  const cleanupCfg = $derived(cleanupSettings ?? FALLBACK_CLEANUP);

  /** 附加光标（多光标；显示坐标，不含主光标） */
  let extraCarets = $state<CaretPos[]>([]);
  /** 矩形选择锚点/端点（修饰键拖拽；null=未激活） */
  let rectAnchor = $state<CaretPos | null>(null);
  let rectHead = $state<CaretPos | null>(null);
  /** 当前拖拽是否为矩形（区分普通拖选与矩形拖选） */
  let dragRect = false;
  /** 矩形拖拽是否发生过移动（区分「修饰键单击=加光标」与「拖选=矩形」） */
  let rectDragMoved = false;
  /** 附加光标盒与矩形选择盒 */
  let extraBoxes = $state<Box[]>([]);
  let rectBoxes = $state<Box[]>([]);

  /** 规格化矩形（行数受多光标上限保护；无锚点/端点时为 null）。 */
  const rectRange = $derived.by(() =>
    rectAnchor && rectHead
      ? clampRectRows(normalizeRect(rectAnchor, rectHead), Math.max(2, multi.maxCount))
      : null,
  );

  /** 矩形修饰键是否激活（设置可配：Alt 或 Ctrl+Alt）。 */
  function rectModifierActive(event: MouseEvent): boolean {
    if (!multi.enabled) return false;
    return multi.rectModifier === 'alt'
      ? event.altKey && !event.ctrlKey && !event.metaKey
      : event.altKey && event.ctrlKey;
  }

  /** 清空多光标与矩形选择。 */
  function clearMulti(): void {
    extraCarets = [];
    rectAnchor = null;
    rectHead = null;
    dragRect = false;
    rectDragMoved = false;
  }

  /** 行操作默认值兜底（设置未就绪时；与 Rust 默认一致）。 */
  const FALLBACK_LINE_DEFAULTS: EditorLinesSettings = {
    defaultScope: 'all',
    sortMode: 'lex',
    dedupeMode: 'keepFirst',
    dedupeIgnoreCase: false,
    dedupeFuzzy: false,
    indentWidth: 4,
    indentStyle: 'spaces',
    caseDefault: 'lower',
    columnDelimiter: '\t',
    preview: true,
    skipEmptyLines: false,
  };

  /** 当前选区的显示行范围（无选区时 from==to==当前行；批量序号作用范围默认值用） */
  const batchSelectionRows = $derived.by(() => {
    const ordered = orderedSelection(logicalSelection(selection));
    return { from: ordered.start.row, to: ordered.end.row };
  });

  // ---- 坐标与渲染 ----

  /** .page 元素（叠加层定位基准）。 */
  function pageEl(): HTMLElement | null {
    const page = getContainer()?.querySelector('.page');
    return page instanceof HTMLElement ? page : null;
  }

  /** `.txt` 内的文本节点定位（空格/制表标记为 1:1 替换，DOM 索引与逻辑
   *  UTF-16 偏移一致；行尾空白可能是独立文本节点，需跨节点换算）。 */
  function textAt(node: Element, offset: number): { node: Text; local: number } | null {
    const root = node.querySelector('.txt') ?? node;
    let remaining = offset;
    for (const child of root.childNodes) {
      if (child instanceof Text) {
        if (remaining <= child.length) return { node: child, local: remaining };
        remaining -= child.length;
      }
    }
    let last: Text | null = null;
    for (const child of root.childNodes) if (child instanceof Text) last = child;
    return last ? { node: last, local: last.length } : null;
  }

  /** 指定位置的光标盒（相对 .page）；结点缺失或空文档时为 null。 */
  function caretRectAt(pos: CaretPos, pageRect: DOMRect): Box | null {
    const node = rowNode(pos.row);
    if (!node) return null;
    const text = rowText(pos.row);
    const offset = text === undefined ? 0 : Math.min(Math.max(0, pos.utf16), text.length);
    const range = document.createRange();
    const anchor = textAt(node, offset);
    if (anchor) {
      range.setStart(anchor.node, anchor.local);
      range.collapse(true);
    } else {
      range.selectNodeContents(node);
      range.collapse(true);
    }
    let rect = range.getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) {
      rect = node.getBoundingClientRect();
    }
    const height = rect.height || node.offsetHeight || 29;
    return { left: rect.left - pageRect.left, top: rect.top - pageRect.top, height, width: 0 };
  }

  /** 单行内 [from,to) 的测量盒（仅当前已渲染行）。 */
  function rowRangeRects(row: number, from: number, to: number, pageRect: DOMRect): Box[] {
    const node = rowNode(row);
    const text = rowText(row);
    if (!node || text === undefined) return [];
    const start = Math.min(from, text.length);
    const end = Math.min(to, text.length);
    if (end <= start) return [];
    const startAnchor = textAt(node, start);
    const endAnchor = textAt(node, end);
    if (!startAnchor || !endAnchor) return [];
    const range = document.createRange();
    range.setStart(startAnchor.node, startAnchor.local);
    range.setEnd(endAnchor.node, endAnchor.local);
    const boxes: Box[] = [];
    for (const rect of range.getClientRects()) {
      if (rect.width > 0 || rect.height > 0) {
        boxes.push({
          left: rect.left - pageRect.left,
          top: rect.top - pageRect.top,
          height: rect.height,
          width: rect.width,
        });
      }
    }
    return boxes;
  }

  /** 选区盒列表（仅当前已渲染行；跨行逐段测量）。 */
  function selectionRects(pageRect: DOMRect): Box[] {
    if (isCollapsed(selection)) return [];
    const { start, end } = orderedSelection(selection);
    const boxes: Box[] = [];
    const lastRow = Math.min(end.row, rowsTotal - 1);
    for (let row = start.row; row <= lastRow; row += 1) {
      const from = row === start.row ? start.utf16 : 0;
      const to = row === end.row ? end.utf16 : Number.MAX_SAFE_INTEGER;
      boxes.push(...rowRangeRects(row, from, to, pageRect));
    }
    return boxes;
  }

  /** 矩形选择盒（各已渲染行的 [fromCol,toCol)）。 */
  function rectSelectionRects(range: RectRange | null, pageRect: DOMRect): Box[] {
    if (!range || rectIsCollapsed(range)) return [];
    const boxes: Box[] = [];
    const lastRow = Math.min(range.toRow, rowsTotal - 1);
    for (let row = range.fromRow; row <= lastRow; row += 1) {
      boxes.push(...rowRangeRects(row, range.fromCol, range.toCol, pageRect));
    }
    return boxes;
  }

  /** 自动补对字符表（开→闭；成对引号亦在此）。 */
  const PAIR_OPENERS: Record<string, string> = {
    '(': ')',
    '[': ']',
    '{': '}',
    '"': '"',
    "'": "'",
    '`': '`',
  };

  /** 闭括号反查表（开括号；用于跳过与配对扫描）。 */
  const PAIR_CLOSERS: Record<string, string> = { ')': '(', ']': '[', '}': '{' };

  /** 括号配对高亮盒（从 bracketMatch 换算）。 */
  function bracketMatchRects(pageRect: DOMRect): Box[] {
    const match = bracketMatch;
    if (!match || match.length !== 2) return [];
    const boxes: Box[] = [];
    for (const pos of match) {
      boxes.push(...rowRangeRects(pos.row, pos.utf16, pos.utf16 + 1, pageRect));
    }
    return boxes;
  }

  /** 光标旁括号 → 配对位置（同/跨显示行按字符流扫描；限额 400 行 / 20 万字符）。 */
  async function findBracketMatch(pos: CaretPos): Promise<CaretPos | null> {
    const cur = rowText(pos.row) ?? (await ensureRow(pos.row));
    if (cur === undefined) return null;
    const before = pos.utf16 > 0 ? cur.charAt(pos.utf16 - 1) : '';
    const at = pos.utf16 < cur.length ? cur.charAt(pos.utf16) : '';
    let ch = '';
    let index = 0;
    if (before && (before in PAIR_OPENERS || before in PAIR_CLOSERS)) {
      ch = before;
      index = pos.utf16 - 1;
    } else if (at && (at in PAIR_OPENERS || at in PAIR_CLOSERS)) {
      ch = at;
      index = pos.utf16;
    } else {
      return null;
    }
    const opener = PAIR_OPENERS[ch];
    const forward = opener !== undefined;
    const counterpart = forward ? opener : PAIR_CLOSERS[ch];
    if (!counterpart) return null;
    let depth = 0;
    let budget = 200_000;
    if (forward) {
      const lastRow = Math.min(rowsTotal - 1, pos.row + 400);
      for (let row = pos.row; row <= lastRow && budget > 0; row += 1) {
        const text = row === pos.row ? cur : rowText(row) ?? (await ensureRow(row));
        if (text === undefined) break;
        const from = row === pos.row ? index : 0;
        for (let i = from; i < text.length && budget > 0; i += 1) {
          budget -= 1;
          const c = text.charAt(i);
          if (c === ch) depth += 1;
          else if (c === counterpart) {
            depth -= 1;
            if (depth === 0) return { row, utf16: i };
          }
        }
      }
    } else {
      const firstRow = Math.max(0, pos.row - 400);
      for (let row = pos.row; row >= firstRow && budget > 0; row -= 1) {
        const text = row === pos.row ? cur : rowText(row) ?? (await ensureRow(row));
        if (text === undefined) break;
        const from = row === pos.row ? index : text.length - 1;
        for (let i = from; i >= 0 && budget > 0; i -= 1) {
          budget -= 1;
          const c = text.charAt(i);
          if (c === ch) depth += 1;
          else if (c === counterpart) {
            depth -= 1;
            if (depth === 0) return { row, utf16: i };
          }
        }
      }
    }
    return null;
  }

  /** 重算光标、多光标与选区叠加层。 */
  function refreshOverlay(): void {
    const page = pageEl();
    if (!page) {
      caretBox = null;
      selBoxes = [];
      extraBoxes = [];
      rectBoxes = [];
      bracketBoxes = [];
      return;
    }
    const pageRect = page.getBoundingClientRect();
    caretBox = caretRectAt(selection.head, pageRect);
    selBoxes = selectionRects(pageRect);
    extraBoxes = extraCarets
      .map((pos) => caretRectAt(pos, pageRect))
      .filter((box): box is Box => box !== null);
    rectBoxes = rectSelectionRects(rectRange, pageRect);
    bracketBoxes = bracketMatchRects(pageRect);
  }

  /** 括号配对高亮：折叠光标且开关开启时防抖扫描（否则清除）。 */
  $effect(() => {
    void selection;
    void revision;
    const active = autoPairsCfg.enabled && autoPairsCfg.highlightMatch && isCollapsed(selection);
    if (!active) {
      bracketMatch = null;
      return;
    }
    const pos = selection.head;
    const timer = setTimeout(() => {
      void (async () => {
        const found = await findBracketMatch(pos);
        bracketMatch = found ? [pos, found] : null;
        await tick();
        refreshOverlay();
      })();
    }, 80);
    return () => clearTimeout(timer);
  });

  /** 光标移出可视区时滚动容器（保持安全边距）。 */
  function scrollCaretIntoView(): void {
    const container = getContainer();
    const page = pageEl();
    const box = caretBox;
    if (!container || !page || !box) return;
    const pageRect = page.getBoundingClientRect();
    const top = pageRect.top + box.top - container.getBoundingClientRect().top;
    if (top < SCROLL_MARGIN) {
      container.scrollBy(0, top - SCROLL_MARGIN);
    } else if (top + box.height > container.clientHeight - SCROLL_MARGIN) {
      container.scrollBy(0, top + box.height - (container.clientHeight - SCROLL_MARGIN));
    }
  }

  /** 更新选区并（必要时）记忆光标、重算叠加层。 */
  function setSelection(next: Selection): void {
    selection = next;
    caretMemory.set(tabId, next.head);
    void tick().then(() => {
      refreshOverlay();
      scrollCaretIntoView();
    });
  }

  /** 设置活动端（extend = 保持锚点形成选区）。 */
  function setHead(pos: CaretPos, extend: boolean): void {
    if (extend) {
      setSelection({ anchor: selection.anchor, head: pos });
    } else {
      setSelection(collapsed(pos));
    }
  }

  /** 垂直移动行数（按视口高度估算翻页步长）。 */
  function pageRowCount(): number {
    const container = getContainer();
    if (!container) return 10;
    return Math.max(1, Math.floor((container.clientHeight - 96) / 29));
  }

  /** 垂直移动（保持列目标）。 */
  function moveVertically(delta: number, extend: boolean): void {
    if (goalUtf16 === null) goalUtf16 = selection.head.utf16;
    const base = moveVertical(selection.head, delta, rowsTotal);
    setHead({ row: base.row, utf16: goalUtf16 }, extend);
  }

  // ---- 编辑操作下发 ----

  /** 确保位置所在行已加载后钳制列偏移。 */
  async function resolveLoadedPos(pos: CaretPos): Promise<CaretPos> {
    const text = rowText(pos.row) ?? (await ensureRow(pos.row));
    const length = text?.length ?? pos.utf16;
    return { row: pos.row, utf16: Math.min(pos.utf16, length) };
  }

  /** 选区钳制：两个端点都收敛到真实行内长度（下发引擎前的必经步骤——
   *  引擎会把越界偏移按 `Utf16OutOfRange` 拒绝，哨兵值（MAX_SAFE_INTEGER）不可外发）。 */
  async function resolveSelection(): Promise<Selection> {
    const anchor = await resolveLoadedPos(selection.anchor);
    const head = await resolveLoadedPos(selection.head);
    return { anchor, head };
  }

  /** 显示行的分段元数据（文本 + 逻辑行号 + 段基偏移）。 */
  function segOf(row: number): SegMeta | undefined {
    const text = rowText(row);
    if (text === undefined) return undefined;
    const meta = rowMeta(row);
    return { logicalRow: meta?.logicalRow ?? row, baseUtf16: meta?.baseUtf16 ?? 0, text };
  }

  /** 显示坐标 → 逻辑坐标（普通行恒等；分段行叠加段基偏移）。 */
  function logicalOf(pos: CaretPos): CaretPos {
    const meta = rowMeta(pos.row);
    return { row: meta?.logicalRow ?? pos.row, utf16: (meta?.baseUtf16 ?? 0) + pos.utf16 };
  }

  /** 选区两端 → 逻辑坐标。 */
  function logicalSelection(sel: Selection): Selection {
    return { anchor: logicalOf(sel.anchor), head: logicalOf(sel.head) };
  }

  /** 快照恢复：由父组件触发（seq 去重），结果走单撤销步。 */
  let restoreHandledSeq = 0;
  $effect(() => {
    const request = snapshotRestore;
    if (!request || request.seq === restoreHandledSeq) return;
    restoreHandledSeq = request.seq;
    void ipc
      .restoreSnapshot(tabId, request.name)
      .then(async (result) => {
        await applyResult(async () => result);
        toasts.show(t('snapshot.restoreDone'));
      })
      .catch((error) => toasts.error(describeIpcError(error)));
  });

  /** 下发操作并在完成后落定光标（后端权威落点）、刷新。 */
  async function applyResult(apply: () => Promise<EditApplied>): Promise<void> {
    try {
      const result = await apply();
      goalUtf16 = null;
      const next = { row: result.caretRow, utf16: result.caretUtf16 };
      const clamped = clampPos(next, result.rowsTotal, (row) => rowText(row)?.length ?? next.utf16);
      onApplied(result);
      setSelection(collapsed(clamped));
      await tick();
      refreshOverlay();
      scrollCaretIntoView();
      void ensureRow(clamped.row);
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[edit] 操作失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 批量操作排序键（起始位置）。 */
  function opStart(op: EditOp): CaretPos {
    return op.kind === 'insert'
      ? { row: op.row, utf16: op.utf16 }
      : { row: op.startRow, utf16: op.startUtf16 };
  }

  /** 批量下发（单撤销步）：ops 排序后应用；成功后落定各光标（主光标 + 附加光标）。 */
  async function applyMulti(ops: EditOp[], newCarets: CaretPos[], primaryIndex: number): Promise<void> {
    const ordered = [...ops]
      .map((op) => ({ op, start: opStart(op) }))
      .sort((a, b) => a.start.row - b.start.row || a.start.utf16 - b.start.utf16)
      .map((item) => item.op);
    try {
      const result = await ipc.applyEdits(tabId, ordered);
      goalUtf16 = null;
      onApplied(result);
      const primary = newCarets[primaryIndex] ?? { row: result.caretRow, utf16: result.caretUtf16 };
      const clamped = clampPos(
        primary,
        result.rowsTotal,
        (row) => rowText(row)?.length ?? primary.utf16,
      );
      selection = collapsed(clamped);
      caretMemory.set(tabId, clamped);
      extraCarets = newCarets.filter((_, index) => index !== primaryIndex);
      rectAnchor = null;
      rectHead = null;
      dragRect = false;
      await tick();
      refreshOverlay();
      scrollCaretIntoView();
      void ensureRow(clamped.row);
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[edit] 批量操作失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 矩形插入（无换行文本）：各行替换 [fromCol,toCol) 并落定列光标。 */
  async function insertIntoRect(text: string, rect: RectRange): Promise<void> {
    for (let row = rect.fromRow; row <= rect.toRow; row += 1) {
      await ensureRow(row);
    }
    const spans = rectSpans((row) => rowText(row)?.length ?? 0, rect);
    const ops: EditOp[] = [];
    const newCarets: CaretPos[] = [];
    for (const span of spans) {
      const meta = rowMeta(span.row);
      const logicalRow = meta?.logicalRow ?? span.row;
      const base = meta?.baseUtf16 ?? 0;
      ops.push(
        replaceOp(
          {
            anchor: { row: logicalRow, utf16: base + span.from },
            head: { row: logicalRow, utf16: base + span.to },
          },
          text,
        ),
      );
      newCarets.push({ row: span.row, utf16: span.from + text.length });
    }
    const primaryIndex = Math.max(
      0,
      newCarets.findIndex((pos) => pos.row >= selection.head.row),
    );
    await applyMulti(ops, newCarets, primaryIndex);
  }

  /** 多光标插入（无换行文本）：各光标点插入（主光标在列表首位）。 */
  async function insertAtCarets(text: string): Promise<void> {
    const ordered = uniquePositions(selection.head, extraCarets);
    const ops: EditOp[] = [];
    const newCarets: CaretPos[] = [];
    for (const raw of ordered) {
      const pos = await resolveLoadedPos(raw);
      ops.push(insertOp(logicalOf(pos), text));
      newCarets.push({ row: pos.row, utf16: pos.utf16 + text.length });
    }
    await applyMulti(ops, newCarets, 0);
  }

  /** 多光标/矩形批量删除（退格/前删；成功后落定到后端权威光标）。 */
  async function multiDelete(kind: 'backspace' | 'forward'): Promise<void> {
    const rect = rectRange;
    const ops: EditOp[] = [];
    if (rect && !rectIsCollapsed(rect)) {
      for (let row = rect.fromRow; row <= rect.toRow; row += 1) {
        await ensureRow(row);
      }
      const lengthAt = (row: number): number => rowText(row)?.length ?? 0;
      const spans =
        kind === 'backspace' ? rectBackspaceSpans(lengthAt, rect) : rectDeleteSpans(lengthAt, rect);
      for (const span of spans) {
        const meta = rowMeta(span.row);
        const logicalRow = meta?.logicalRow ?? span.row;
        const base = meta?.baseUtf16 ?? 0;
        ops.push(
          deleteRangeOp({
            startRow: logicalRow,
            startUtf16: base + span.from,
            endRow: logicalRow,
            endUtf16: base + span.to,
          }),
        );
      }
    } else {
      const ordered = uniquePositions(selection.head, extraCarets);
      ordered.sort((a, b) => a.row - b.row || a.utf16 - b.utf16);
      for (const raw of ordered) {
        const pos = await resolveLoadedPos(raw);
        const cur = segOf(pos.row);
        if (!cur) continue;
        let neighbour: SegMeta | null = null;
        const neighbourRow = kind === 'backspace' ? pos.row - 1 : pos.row + 1;
        const needsNeighbour =
          kind === 'backspace'
            ? pos.utf16 === 0 && pos.row > 0
            : pos.utf16 >= cur.text.length && pos.row + 1 < rowsTotal;
        if (needsNeighbour) {
          const text = rowText(neighbourRow) ?? (await ensureRow(neighbourRow));
          const meta = rowMeta(neighbourRow);
          if (text !== undefined) {
            neighbour = {
              logicalRow: meta?.logicalRow ?? neighbourRow,
              baseUtf16: meta?.baseUtf16 ?? 0,
              text,
            };
          }
        }
        const range =
          kind === 'backspace'
            ? planBackspace(cur, pos.utf16, neighbour)
            : planDeleteForward(cur, pos.utf16, neighbour);
        if (range) ops.push(deleteRangeOp(range));
      }
    }
    clearMulti();
    if (ops.length === 0) {
      refreshOverlay();
      return;
    }
    await applyResult(() => ipc.applyEdits(tabId, ops));
  }

  /** 矩形文本收集（各行 [fromCol,toCol) 以换行连接）。 */
  async function gatherRectText(rect: RectRange): Promise<string | null> {
    if (rect.toRow - rect.fromRow + 1 > COPY_MAX_ROWS) {
      toasts.error(t('edit.selectionTooLarge'));
      return null;
    }
    let out = '';
    for (let row = rect.fromRow; row <= rect.toRow; row += 1) {
      const text = rowText(row) ?? (await ensureRow(row));
      if (text === undefined) return null;
      out += text.slice(rect.fromCol, Math.min(rect.toCol, text.length));
      if (row < rect.toRow) out += '\n';
      if (out.length > COPY_MAX_CHARS) {
        toasts.error(t('edit.selectionTooLarge'));
        return null;
      }
    }
    return out;
  }

  /** 矩形删除（剪切用）：整段 [fromCol,toCol)。 */
  async function deleteRect(rect: RectRange): Promise<void> {
    for (let row = rect.fromRow; row <= rect.toRow; row += 1) {
      await ensureRow(row);
    }
    const spans = rectSpans((row) => rowText(row)?.length ?? 0, rect);
    const ops: EditOp[] = [];
    for (const span of spans) {
      if (span.to <= span.from) continue;
      const meta = rowMeta(span.row);
      const logicalRow = meta?.logicalRow ?? span.row;
      const base = meta?.baseUtf16 ?? 0;
      ops.push(
        deleteRangeOp({
          startRow: logicalRow,
          startUtf16: base + span.from,
          endRow: logicalRow,
          endUtf16: base + span.to,
        }),
      );
    }
    rectAnchor = null;
    rectHead = null;
    if (ops.length === 0) return;
    await applyResult(() => ipc.applyEdits(tabId, ops));
  }

  /** 插入文本（含选区替换；每次 = 单个撤销步）。 */
  async function doInsert(text: string): Promise<void> {
    if (text.length === 0) return;
    const rect = rectRange;
    const hasNewline = text.includes('\n');
    if (!hasNewline && rect && !rectIsCollapsed(rect)) {
      await insertIntoRect(text, rect);
      return;
    }
    if (!hasNewline && extraCarets.length > 0) {
      await insertAtCarets(text);
      return;
    }
    if (extraCarets.length > 0 || (rect && !rectIsCollapsed(rect))) {
      // 含换行的批量插入暂保守退化为单点编辑（清空多光标）
      clearMulti();
    }
    if (!isCollapsed(selection)) {
      const resolved = await resolveSelection();
      await applyResult(() => ipc.applyEdits(tabId, [replaceOp(logicalSelection(resolved), text)]));
      return;
    }
    const pos = await resolveLoadedPos(selection.head);
    await applyResult(() => ipc.applyEdits(tabId, [insertOp(logicalOf(pos), text)]));
  }

  /** 退格（选区删除或前一个字符/合并上一行；多光标/矩形批量=单撤销步）。 */
  async function doBackspace(): Promise<void> {
    if (extraCarets.length > 0 || (rectRange && !rectIsCollapsed(rectRange))) {
      await multiDelete('backspace');
      return;
    }
    if (!isCollapsed(selection)) {
      const resolved = await resolveSelection();
      await applyResult(() => ipc.applyEdits(tabId, [deleteOp(logicalSelection(resolved))]));
      return;
    }
    const pos = await resolveLoadedPos(selection.head);
    const cur = segOf(pos.row);
    if (!cur) return;
    // 自动补对：光标恰在空对（如 `()`）之间时，退格一次删除整对。
    const pairPrev = pos.utf16 > 0 ? cur.text.charAt(pos.utf16 - 1) : '';
    const pairNext = pos.utf16 < cur.text.length ? cur.text.charAt(pos.utf16) : '';
    if (
      autoPairsCfg.enabled &&
      autoPairsCfg.autoClose &&
      pairPrev &&
      PAIR_OPENERS[pairPrev] === pairNext
    ) {
      const start = logicalOf({ row: pos.row, utf16: pos.utf16 - 1 });
      const end = logicalOf({ row: pos.row, utf16: pos.utf16 + 1 });
      await applyResult(() => ipc.applyEdits(tabId, [deleteOp({ anchor: start, head: end })]));
      return;
    }
    let prev: SegMeta | null = null;
    if (pos.utf16 === 0 && pos.row > 0) {
      const text = rowText(pos.row - 1) ?? (await ensureRow(pos.row - 1));
      const meta = rowMeta(pos.row - 1);
      if (text !== undefined) {
        prev = {
          logicalRow: meta?.logicalRow ?? pos.row - 1,
          baseUtf16: meta?.baseUtf16 ?? 0,
          text,
        };
      }
    }
    const range = planBackspace(cur, pos.utf16, prev);
    if (!range) return;
    await applyResult(() => ipc.applyEdits(tabId, [deleteRangeOp(range)]));
  }

  /** 前向删除（选区删除或后一个字符/合并下一行；多光标/矩形批量=单撤销步）。 */
  async function doDeleteForward(): Promise<void> {
    if (extraCarets.length > 0 || (rectRange && !rectIsCollapsed(rectRange))) {
      await multiDelete('forward');
      return;
    }
    if (!isCollapsed(selection)) {
      const resolved = await resolveSelection();
      await applyResult(() => ipc.applyEdits(tabId, [deleteOp(logicalSelection(resolved))]));
      return;
    }
    const pos = await resolveLoadedPos(selection.head);
    const cur = segOf(pos.row);
    if (!cur) return;
    let next: SegMeta | null = null;
    if (pos.utf16 >= cur.text.length && pos.row + 1 < rowsTotal) {
      const text = rowText(pos.row + 1) ?? (await ensureRow(pos.row + 1));
      const meta = rowMeta(pos.row + 1);
      if (text !== undefined) {
        next = {
          logicalRow: meta?.logicalRow ?? pos.row + 1,
          baseUtf16: meta?.baseUtf16 ?? 0,
          text,
        };
      }
    }
    const range = planDeleteForward(cur, pos.utf16, next);
    if (!range) return;
    await applyResult(() => ipc.applyEdits(tabId, [deleteRangeOp(range)]));
  }

  /** 撤销/重做（结果落定到受影响行首）。 */
  async function doUndoRedo(redo: boolean): Promise<void> {
    try {
      const result = redo ? await ipc.redoEdit(tabId) : await ipc.undoEdit(tabId);
      if (!result) return;
      goalUtf16 = null;
      onApplied(result);
      setSelection(
        collapsed(
          clampPos(
            { row: result.caretRow, utf16: result.caretUtf16 },
            result.rowsTotal,
            (row) => rowText(row)?.length ?? 0,
          ),
        ),
      );
      await tick();
      refreshOverlay();
      scrollCaretIntoView();
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[edit] 撤销/重做失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 收集选区文本（缺行时按需加载；超上限提示分段复制）。
   *  分段行按逻辑行拼接：同一逻辑行内相邻段直接相接，不插入换行。 */
  async function gatherSelectedText(): Promise<string | null> {
    const { start, end } = orderedSelection(selection);
    if (end.row - start.row + 1 > COPY_MAX_ROWS) {
      toasts.error(t('edit.selectionTooLarge'));
      return null;
    }
    let out = '';
    let prevLogical: number | null = null;
    const lastRow = Math.min(end.row, rowsTotal - 1);
    for (let row = start.row; row <= lastRow; row += 1) {
      const text = rowText(row) ?? (await ensureRow(row));
      if (text === undefined) return null;
      const meta = rowMeta(row);
      const logicalRow = meta?.logicalRow ?? row;
      const from = row === start.row ? Math.min(start.utf16, text.length) : 0;
      const to = row === end.row ? Math.min(end.utf16, text.length) : text.length;
      if (prevLogical !== null && logicalRow !== prevLogical) out += '\n';
      out += text.slice(from, to);
      prevLogical = logicalRow;
      if (out.length > COPY_MAX_CHARS) {
        toasts.error(t('edit.selectionTooLarge'));
        return null;
      }
    }
    return out;
  }

  /** 复制/剪切选区（写系统剪贴板；经 Tauri 剪贴板插件，无浏览器权限弹窗）。
   *  矩形选择优先：复制各行 [fromCol,toCol) 以换行连接；剪切=删除对应跨度。 */
  async function doCopy(cut: boolean): Promise<void> {
    const rect = rectRange;
    if (rect && !rectIsCollapsed(rect)) {
      const text = await gatherRectText(rect);
      if (text === null) return;
      try {
        await writeText(text);
      } catch {
        toasts.error(t('edit.copyFailed'));
        return;
      }
      recordClipboard(text);
      if (cut) await deleteRect(rect);
      return;
    }
    if (isCollapsed(selection)) return;
    const text = await gatherSelectedText();
    if (text === null) return;
    try {
      await writeText(text);
    } catch {
      toasts.error(t('edit.copyFailed'));
      return;
    }
    recordClipboard(text);
    if (cut) {
      const resolved = await resolveSelection();
      await applyResult(() => ipc.applyEdits(tabId, [deleteOp(logicalSelection(resolved))]));
    }
  }

  /** 全选：优先取末行真实长度（哨兵值会被引擎拒绝，仅行未加载时暂用并等待钳制）。 */
  function selectAll(): void {
    const lastRow = Math.max(0, rowsTotal - 1);
    const lastText = rowText(lastRow);
    const endUtf16 = lastText !== undefined ? lastText.length : UTF16_END;
    setSelection({
      anchor: { row: 0, utf16: 0 },
      head: { row: lastRow, utf16: endUtf16 },
    });
    if (lastText === undefined) void ensureRow(lastRow);
  }

  // ---- 查找/替换（v2：标准/正则双模式 + 预览确认 + 文档高亮） ----

  /** 打开查找条（replace = 同时显示替换行）。 */
  function openFind(replace: boolean): void {
    findOpen = true;
    replaceOpen = replace;
    findFocusSeq += 1;
    void refreshFindHistory();
  }

  /** 关闭查找条、清除高亮并归还键盘焦点。 */
  function closeFind(): void {
    findOpen = false;
    matchBoxes = [];
    findCountText = '';
    focusInput();
  }

  /** 把命中设为选区（显示坐标）并滚动可见。 */
  function selectHit(hit: FindHit): void {
    setSelection({
      anchor: { row: hit.startRow, utf16: hit.startUtf16 },
      head: { row: hit.endRow, utf16: hit.endUtf16 },
    });
  }

  /** 查询条件变化（FindBar 上报）：记录，高亮/计数由防抖 effect 统一刷新。 */
  function handleQueryChange(
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    scope: FindScope,
    range: [number, number] | null,
  ): void {
    liveQuery = { query, caseSensitive, mode, wholeWord, scope, range };
  }

  /**
   * 解析查找范围：document → null；selection → 当前选中行；rowRange → 输入值。
   * 输入为 1 基行号，内部坐标为 0 基；非法（<1 或倒挂）返回 'invalid'。
   */
  function boundsOf(
    scope: FindScope,
    range: [number, number] | null,
  ): [number, number] | 'invalid' | null {
    if (scope === 'document') return null;
    if (scope === 'selection') {
      const sel = orderedSelection(selection);
      return [sel.start.row, sel.end.row];
    }
    if (!range) return null;
    const [from, to] = range;
    if (!Number.isFinite(from) || !Number.isFinite(to) || from < 1 || to < 1 || from > to) {
      return 'invalid';
    }
    return [from - 1, to - 1];
  }

  /** 拉取查找历史（打开查找条/记录后刷新；辅助功能失败静默）。 */
  async function refreshFindHistory(): Promise<void> {
    try {
      findHistory = await ipc.listFindHistory();
    } catch {
      findHistory = [];
    }
  }

  /** 记录一次查找历史（辅助功能：失败静默）。 */
  async function recordFindHistory(query: string): Promise<void> {
    try {
      findHistory = await ipc.addFindHistory(query);
    } catch {
      // 历史为辅助功能：失败不影响查找流程，静默
    }
  }

  /** 清空查找历史（辅助功能：失败静默）。 */
  async function clearFindHistory(): Promise<void> {
    try {
      findHistory = await ipc.clearFindHistory();
    } catch {
      // 同上：静默
    }
  }

  /** 可见行窗口内全部命中 → 高亮盒（正则语法错误等静默；动作路径有 toast 反馈）。 */
  async function refreshMatches(): Promise<void> {
    const { query, caseSensitive, mode, wholeWord, scope, range } = liveQuery;
    const bounds = boundsOf(scope, range);
    if (!findOpen || query.length === 0 || !findDefaults.highlightAll || bounds === 'invalid') {
      matchBoxes = [];
      return;
    }
    const container = getContainer();
    if (!container) {
      matchBoxes = [];
      return;
    }
    const nodes = container.querySelectorAll<HTMLElement>('.row[data-row]');
    if (nodes.length === 0) {
      matchBoxes = [];
      return;
    }
    let minRow = Number.MAX_SAFE_INTEGER;
    let maxRow = -1;
    nodes.forEach((node) => {
      const row = Number(node.dataset.row ?? -1);
      if (Number.isFinite(row) && row >= 0) {
        minRow = Math.min(minRow, row);
        maxRow = Math.max(maxRow, row);
      }
    });
    if (maxRow < 0) {
      matchBoxes = [];
      return;
    }
    const seq = ++matchSeq;
    try {
      const hits = await ipc.matchWindow(
        tabId,
        query,
        caseSensitive,
        mode,
        wholeWord,
        minRow,
        maxRow - minRow + 1,
      );
      if (seq !== matchSeq || !findOpen) return;
      const page = pageEl();
      if (!page) return;
      const pageRect = page.getBoundingClientRect();
      const boxes: Box[] = [];
      for (const hit of hits.slice(0, MATCH_HIGHLIGHT_MAX)) {
        if (bounds && (hit.startRow < bounds[0] || hit.startRow > bounds[1])) continue;
        const lastRow = Math.min(hit.endRow, maxRow);
        for (let row = hit.startRow; row <= lastRow; row += 1) {
          const node = rowNode(row);
          const text = rowText(row);
          if (!node || text === undefined) continue;
          const from = row === hit.startRow ? Math.min(hit.startUtf16, text.length) : 0;
          const to = row === hit.endRow ? Math.min(hit.endUtf16, text.length) : text.length;
          if (to <= from) continue;
          const startAnchor = textAt(node, from);
          const endAnchor = textAt(node, to);
          if (!startAnchor || !endAnchor) continue;
          const range = document.createRange();
          range.setStart(startAnchor.node, startAnchor.local);
          range.setEnd(endAnchor.node, endAnchor.local);
          for (const rect of range.getClientRects()) {
            if (rect.width > 0 || rect.height > 0) {
              boxes.push({
                left: rect.left - pageRect.left,
                top: rect.top - pageRect.top,
                height: rect.height,
                width: rect.width,
              });
            }
          }
        }
      }
      matchBoxes = boxes;
    } catch {
      // 正则语法错误/文档变化等：高亮静默；用户动作路径有 toast 反馈
      matchBoxes = [];
    }
  }

  /** 匹配计数：document = 全文计数；限定范围 = 窗口内命中数（防抖后调用）。 */
  async function refreshCount(): Promise<void> {
    const { query, caseSensitive, mode, wholeWord, scope, range } = liveQuery;
    if (!findOpen || query.length === 0 || !findDefaults.matchCount) {
      findCountText = '';
      return;
    }
    const bounds = boundsOf(scope, range);
    if (bounds === 'invalid') {
      findCountText = '';
      return;
    }
    const seq = ++countSeq;
    try {
      if (bounds === null) {
        const result = await ipc.countMatchesInEdit(tabId, query, caseSensitive, mode, wholeWord);
        if (seq !== countSeq || !findOpen) return;
        findCountText = result.truncated
          ? t('find.countMore')
          : t('find.count', { total: result.total });
      } else {
        const hits = await ipc.matchWindow(
          tabId,
          query,
          caseSensitive,
          mode,
          wholeWord,
          bounds[0],
          Math.max(1, bounds[1] - bounds[0] + 1),
        );
        if (seq !== countSeq || !findOpen) return;
        const inRange = hits.filter(
          (hit) => hit.startRow >= bounds[0] && hit.startRow <= bounds[1],
        );
        findCountText = t('find.count', { total: inRange.length });
      }
    } catch {
      // 正则语法错误等：计数静默
      findCountText = '';
    }
  }

  /** 查找下一个：从当前光标/范围起点起；范围外命中忽略；按设置循环。 */
  async function doFindNext(
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    scope: FindScope,
    range: [number, number] | null,
  ): Promise<void> {
    if (query.length === 0) return;
    const bounds = boundsOf(scope, range);
    if (bounds === 'invalid') {
      toasts.error(t('find.rangeInvalid'));
      return;
    }
    try {
      const head = selection.head;
      const headInside = bounds === null || (head.row >= bounds[0] && head.row <= bounds[1]);
      const startFrom: [number, number] | null = headInside
        ? [head.row, head.utf16]
        : bounds !== null
          ? [bounds[0], 0]
          : null;
      let hit = await ipc.findInEdit(tabId, query, caseSensitive, mode, wholeWord, startFrom);
      if (hit && bounds && (hit.startRow < bounds[0] || hit.startRow > bounds[1])) hit = null;
      if (!hit && findDefaults.wrapAround) {
        const wrapFrom: [number, number] | null = bounds !== null ? [bounds[0], 0] : null;
        hit = await ipc.findInEdit(tabId, query, caseSensitive, mode, wholeWord, wrapFrom);
        if (hit && bounds && (hit.startRow < bounds[0] || hit.startRow > bounds[1])) hit = null;
      }
      if (!hit) {
        toasts.error(t('find.notFound', { query }));
        return;
      }
      selectHit(hit);
      void ensureRow(hit.startRow);
      void ensureRow(hit.endRow);
      void recordFindHistory(query);
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[edit] 查找失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 替换一个：优先替换当前选区起点处的命中；范围内时先定位范围内命中。 */
  async function doReplace(
    query: string,
    replacement: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    scope: FindScope,
    range: [number, number] | null,
  ): Promise<void> {
    if (query.length === 0) return;
    const bounds = boundsOf(scope, range);
    if (bounds === 'invalid') {
      toasts.error(t('find.rangeInvalid'));
      return;
    }
    try {
      let from: [number, number] | null;
      if (bounds !== null) {
        // 范围限定：先定位下一个范围内命中，再以其起点执行替换
        const start = orderedSelection(selection).start;
        const startFrom: [number, number] =
          start.row >= bounds[0] && start.row <= bounds[1] ? [start.row, start.utf16] : [bounds[0], 0];
        const probe = await ipc.findInEdit(tabId, query, caseSensitive, mode, wholeWord, startFrom);
        if (!probe || probe.startRow < bounds[0] || probe.startRow > bounds[1]) {
          toasts.error(t('find.notFound', { query }));
          return;
        }
        from = [probe.startRow, probe.startUtf16];
      } else {
        const start = orderedSelection(selection).start;
        from = [start.row, start.utf16];
      }
      const outcome = await ipc.replaceInEdit(
        tabId,
        query,
        caseSensitive,
        mode,
        wholeWord,
        from,
        replacement,
      );
      if (!outcome) {
        toasts.error(t('find.notFound', { query }));
        return;
      }
      applyOutcome(outcome.applied);
      // 替换会改变文档：把键盘焦点交还编辑器（否则 Ctrl+Z 等编辑按键落在查找条上无效）
      focusEditorProxy();
      if (
        outcome.next &&
        (bounds === null ||
          (outcome.next.startRow >= bounds[0] && outcome.next.startRow <= bounds[1]))
      ) {
        selectHit(outcome.next);
        void ensureRow(outcome.next.startRow);
      }
      void recordFindHistory(query);
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[edit] 替换失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 全部替换：先预览；范围限定=直接替换范围内命中；整文档按设置决定是否二次确认（可剔除）。 */
  async function doReplaceAll(
    query: string,
    replacement: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    scope: FindScope,
    range: [number, number] | null,
  ): Promise<void> {
    if (query.length === 0) return;
    const bounds = boundsOf(scope, range);
    if (bounds === 'invalid') {
      toasts.error(t('find.rangeInvalid'));
      return;
    }
    try {
      const preview = await ipc.previewReplaceAll(
        tabId,
        query,
        caseSensitive,
        mode,
        wholeWord,
        replacement,
      );
      if (preview.total === 0) {
        toasts.error(t('find.notFound', { query }));
        return;
      }
      if (bounds !== null) {
        // 范围限定：直接替换范围内命中（预览剔除流程仅用于整文档；预览被截断时无法枚举，提示收窄范围）
        if (preview.truncated) {
          toasts.error(t('find.countMore'));
          return;
        }
        const selected = preview.items
          .filter((item) => item.startRow >= bounds[0] && item.startRow <= bounds[1])
          .map((item) => item.index);
        if (selected.length === 0) {
          toasts.error(t('find.notFound', { query }));
          return;
        }
        await applyReplaceAll(preview, query, replacement, caseSensitive, mode, wholeWord, selected);
        return;
      }
      if (preview.total === 1 || !findDefaults.replacePreview) {
        await applyReplaceAll(preview, query, replacement, caseSensitive, mode, wholeWord, null);
        return;
      }
      previewData = preview;
      previewArgs = { query, replacement, caseSensitive, mode, wholeWord };
      previewOpen = true;
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[edit] 全部替换预览失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 执行「全部替换」（selected = null 全部；数组 = 预览弹窗勾选后的序号）。 */
  async function applyReplaceAll(
    preview: ReplacePreview,
    query: string,
    replacement: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    selected: number[] | null,
  ): Promise<void> {
    try {
      const outcome = await ipc.applyReplaceAll(
        tabId,
        query,
        caseSensitive,
        mode,
        wholeWord,
        replacement,
        selected,
        preview.stateId,
      );
      if (outcome.applied) applyOutcome(outcome.applied);
      focusEditorProxy();
      toasts.show(t('edit.replaceDone', { count: outcome.replaced }));
      void recordFindHistory(query);
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[edit] 全部替换失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 预览弹窗确认：关闭并执行（truncated 时前端传 null = 全部替换）。 */
  function confirmPreview(selected: number[] | null): void {
    const data = previewData;
    const args = previewArgs;
    previewOpen = false;
    previewData = null;
    previewArgs = null;
    if (!data || !args) return;
    void applyReplaceAll(
      data,
      args.query,
      args.replacement,
      args.caseSensitive,
      args.mode,
      args.wholeWord,
      selected,
    );
  }

  /** 预览弹窗取消。 */
  function cancelPreview(): void {
    previewOpen = false;
    previewData = null;
    previewArgs = null;
    focusEditorProxy();
  }

  /** 应用替换结果：回报父组件 + 光标落到后端权威位置。 */
  function applyOutcome(result: EditApplied): void {
    goalUtf16 = null;
    onApplied(result);
    const clamped = clampPos(
      { row: result.caretRow, utf16: result.caretUtf16 },
      result.rowsTotal,
      (row) => rowText(row)?.length ?? result.caretUtf16,
    );
    setSelection(collapsed(clamped));
  }

  /** 菜单粘贴：读取系统剪贴板（Tauri 剪贴板插件，无浏览器权限弹窗）。 */
  async function doPasteFromMenu(): Promise<void> {
    try {
      const text = await readText();
      await doInsert(text);
    } catch {
      toasts.error(t('edit.pasteFailed'));
    }
  }

  /** 批量序号：请求预览（错误向上抛出，由弹窗就地展示）。 */
  async function previewBatch(config: BatchNumberingConfig): Promise<BatchPreview> {
    return ipc.previewBatchNumbering(tabId, config);
  }

  /** 批量序号：执行（单撤销步；失败先提示再抛出，弹窗保持打开）。 */
  async function applyBatch(config: BatchNumberingConfig): Promise<void> {
    let outcome;
    try {
      outcome = await ipc.applyBatchNumbering(tabId, config);
    } catch (error) {
      const payload = toIpcError(error);
      toasts.error(describeIpcError(payload));
      throw error;
    }
    await applyResult(async () => outcome.applied);
    toasts.show(t('batch.applied', { count: outcome.affected }), 'info');
  }

  /** 行操作：请求预览（错误向上抛出，由弹窗就地展示）。 */
  async function previewLineOps(config: LineOpConfig): Promise<LineOpPreview> {
    return ipc.previewLineOp(tabId, config);
  }

  /** 行操作：执行（单撤销步；失败先提示再抛出，弹窗保持打开）。 */
  async function applyLineOps(config: LineOpConfig): Promise<void> {
    let outcome: LineOpOutcome;
    try {
      outcome = await ipc.applyLineOp(tabId, config);
    } catch (error) {
      const payload = toIpcError(error);
      toasts.error(describeIpcError(payload));
      throw error;
    }
    const applied = outcome.applied;
    if (applied) {
      await applyResult(async () => applied);
    }
    if (outcome.affected > 0) {
      toasts.show(t('lineOps.applied', { count: outcome.affected }), 'info');
    } else if (outcome.warning) {
      toasts.show(outcome.warning, 'info');
    }
  }

  // ---- 剪贴板历史与复制格式 ----

  /** 记录复制文本到历史（历史为辅助功能，失败静默不打扰阅读）。 */
  function recordClipboard(text: string): void {
    if (!text) return;
    void ipc.addClipboardEntry(text).catch(() => {});
  }

  /** 打开剪贴板历史并拉取最新列表。 */
  async function openClipboardHistory(): Promise<void> {
    clipboardOpen = true;
    await refreshClipboardHistory();
  }

  /** 刷新历史列表（后端返回最新列表，前端整体替换）。 */
  async function refreshClipboardHistory(): Promise<void> {
    try {
      clipboardEntries = await ipc.listClipboardHistory();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 从历史插入条目（复用输入链路：选区替换/多光标统一处理）。 */
  async function insertClipboardEntry(text: string): Promise<void> {
    clipboardOpen = false;
    focusEditorProxy();
    await doInsert(text);
  }

  /** 删除历史单条。 */
  async function removeClipboardEntry(index: number): Promise<void> {
    try {
      clipboardEntries = await ipc.removeClipboardEntry(index);
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 清空全部历史。 */
  async function clearClipboardHistory(): Promise<void> {
    try {
      clipboardEntries = await ipc.clearClipboardHistory();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 复制为 HTML：选中行转义为段落，写入 HTML 剪贴板格式并以纯文本兜底；同时记录历史。 */
  async function copyAsHtml(): Promise<void> {
    if (isCollapsed(selection)) return;
    const text = await gatherSelectedText();
    if (text === null) return;
    const html = text
      .split(/\r\n|\r|\n/)
      .map(
        (line) =>
          `<p>${line.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')}</p>`,
      )
      .join('');
    try {
      await writeHtml(html, text);
    } catch {
      toasts.error(t('edit.copyFailed'));
      return;
    }
    recordClipboard(text);
  }

  /** 复制为 Markdown：纯文本内容本身即合法 Markdown，与普通复制等价（同样记录历史）。 */
  async function copyAsMarkdown(): Promise<void> {
    await doCopy(false);
  }

  /** 键入文本：启用自动补对时优先走括号/引号处理。 */
  async function handleTypedText(text: string): Promise<void> {
    if (
      text.length === 1 &&
      autoPairsCfg.enabled &&
      autoPairsCfg.autoClose &&
      extraCarets.length === 0 &&
      !(rectRange && !rectIsCollapsed(rectRange))
    ) {
      if (await tryAutoPair(text)) return;
    }
    await doInsert(text);
  }

  /** 自动补对（返回 true 表示已处理，不再常规插入）。 */
  async function tryAutoPair(ch: string): Promise<boolean> {
    const closer = PAIR_OPENERS[ch];
    if (closer !== undefined) {
      if (!isCollapsed(selection)) {
        const resolved = await resolveSelection();
        const selected = await gatherSelectedText();
        if (selected === null) return false;
        await applyResult(async () =>
          ipc.applyEdits(tabId, [replaceOp(logicalSelection(resolved), ch + selected + closer)]),
        );
        return true;
      }
      const pos = await resolveLoadedPos(selection.head);
      const row = segOf(pos.row);
      if (row && pos.utf16 < row.text.length && row.text.charAt(pos.utf16) === closer) {
        // 下一个字符已是右符号：跳过（不重复插入）
        setSelection(collapsed(moveRight(pos, rowsTotal, rowText)));
        refreshOverlay();
        return true;
      }
      const result = await ipc.applyEdits(tabId, [insertOp(logicalOf(pos), ch + closer)]);
      await applyResult(async () => result);
      // 光标回退到左右符号之间（applyResult 落点在右符号之后）
      setSelection(collapsed({ row: result.caretRow, utf16: Math.max(0, result.caretUtf16 - 1) }));
      await tick();
      refreshOverlay();
      return true;
    }
    if (ch in PAIR_CLOSERS && isCollapsed(selection)) {
      const pos = await resolveLoadedPos(selection.head);
      const row = segOf(pos.row);
      if (row && pos.utf16 < row.text.length && row.text.charAt(pos.utf16) === ch) {
        setSelection(collapsed(moveRight(pos, rowsTotal, rowText)));
        refreshOverlay();
        return true;
      }
    }
    return false;
  }

  /** 回车：启用自动缩进时继承当前行行首空白。 */
  async function handleEnter(): Promise<void> {
    if (
      autoPairsCfg.enabled &&
      autoPairsCfg.autoIndent &&
      extraCarets.length === 0 &&
      !(rectRange && !rectIsCollapsed(rectRange)) &&
      isCollapsed(selection)
    ) {
      const pos = await resolveLoadedPos(selection.head);
      const cur = segOf(pos.row);
      const indent = cur ? /^[ \t]*/.exec(cur.text)?.[0] ?? '' : '';
      if (indent) {
        await doInsert('\n' + indent);
        return;
      }
    }
    await doInsert('\n');
  }

  /** 生成当前时间戳字符串（格式取设置；RFC 3339 为 UTC）。 */
  function formatTimestamp(format: TimestampFormat): string {
    const now = new Date();
    const pad = (value: number, width = 2): string => String(value).padStart(width, '0');
    const y = now.getFullYear();
    const mo = pad(now.getMonth() + 1);
    const d = pad(now.getDate());
    const h = pad(now.getHours());
    const mi = pad(now.getMinutes());
    const s = pad(now.getSeconds());
    switch (format) {
      case 'dateOnly':
        return `${y}-${mo}-${d}`;
      case 'timeOnly':
        return `${h}:${mi}:${s}`;
      case 'iso8601': {
        const offset = -now.getTimezoneOffset();
        const sign = offset >= 0 ? '+' : '-';
        const oh = pad(Math.floor(Math.abs(offset) / 60));
        const om = pad(Math.abs(offset) % 60);
        return `${y}-${mo}-${d}T${h}:${mi}:${s}${sign}${oh}:${om}`;
      }
      case 'rfc3339Utc':
        return now.toISOString().replace(/\.\d{3}Z$/, 'Z');
      case 'localDateTime':
      default:
        return `${y}-${mo}-${d} ${h}:${mi}:${s}`;
    }
  }

  /** 插入日期时间（多光标/矩形按现有插入路径批量处理）。 */
  async function insertTimestamp(): Promise<void> {
    await doInsert(formatTimestamp(insertCfg.timestampFormat));
  }

  /** 清理类操作：执行单个行操作（单撤销步），返回有效变更数。 */
  async function runCleanup(op: LineOpConfig['op'], report: boolean): Promise<number> {
    const base = lineDefaults ?? FALLBACK_LINE_DEFAULTS;
    const outcome = await ipc.applyLineOp(tabId, {
      op,
      scope: { kind: 'all' },
      sortOrder: base.sortMode,
      sortSeed: 1,
      dedupeMode: base.dedupeMode,
      dedupeIgnoreCase: base.dedupeIgnoreCase,
      dedupeFuzzy: base.dedupeFuzzy,
      indentWidth: base.indentWidth,
      indentStyle: base.indentStyle,
      caseMode: base.caseDefault,
      widthDirection: 'toHalf',
      text: '',
      count: 1,
      delimiter: base.columnDelimiter,
      delimiterTo: ',',
      skipEmpty: false,
      previewLines: 10,
    });
    const applied = outcome.applied;
    if (applied) {
      await applyResult(async () => applied);
    }
    if (report) {
      toasts.show(t(outcome.affected > 0 ? 'cleanup.done' : 'cleanup.none'));
    }
    return outcome.affected;
  }

  /** 一键清理：按设置勾选依次执行（每项各自为一个撤销步）。 */
  async function runCleanupAll(): Promise<void> {
    let affected = 0;
    if (cleanupCfg.trailingWhitespace) affected += await runCleanup('trimTrailingWhitespace', false);
    if (cleanupCfg.collapseBlankLines) affected += await runCleanup('collapseEmptyLines', false);
    if (cleanupCfg.trailingNewline) affected += await runCleanup('ensureTrailingNewline', false);
    toasts.show(t(affected > 0 ? 'cleanup.done' : 'cleanup.none'));
  }

  /** 外部动作分发（菜单触发）。 */
  /** 当前选区（逻辑坐标）；无有效文档时为 null。 */
  function currentLogicalSelection(): { start: CaretPos; end: CaretPos } | null {
    if (rowsTotal <= 0) return null;
    try {
      return orderedSelection(logicalSelection(selection));
    } catch {
      return null;
    }
  }

  /** 书签：切换（同一显示行已有书签则移除）。 */
  async function toggleBookmarkAnnotation(): Promise<void> {
    const sel = currentLogicalSelection();
    if (!sel) return;
    try {
      const [row, utf16] = await ipc.editDisplayPos(tabId, sel.end.row, sel.end.utf16);
      const existing = annotations.forTab(tabId)?.bookmarks.find((b) => b.row === row);
      if (existing) {
        await annotations.removeBookmark(tabId, existing.id);
        toasts.show(t('annot.bookmarkRemoved'));
      } else {
        await annotations.addBookmark(tabId, row, utf16);
        toasts.show(t('annot.bookmarkAdded'));
      }
      focusEditorProxy();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 高亮：选中文本（限同一行）→ 高亮。 */
  async function highlightSelectionAnnotation(): Promise<void> {
    const sel = currentLogicalSelection();
    if (!sel || sel.start.row !== sel.end.row || (sel.start.row === sel.end.row && sel.start.utf16 === sel.end.utf16)) {
      toasts.show(t('annot.highlightSingleRow'));
      return;
    }
    try {
      const [startRow, startUtf16] = await ipc.editDisplayPos(tabId, sel.start.row, sel.start.utf16);
      const [endRow, endUtf16] = await ipc.editDisplayPos(tabId, sel.end.row, sel.end.utf16);
      if (startRow !== endRow) {
        toasts.show(t('annot.highlightSingleRow'));
        return;
      }
      await annotations.addHighlight(tabId, startRow, startUtf16, endUtf16);
      toasts.show(t('annot.highlightAdded'));
      focusEditorProxy();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 注释/待办：打开输入弹窗（确认后写入）。 */
  function openNoteDialog(kind: 'note' | 'todo'): void {
    if (rowsTotal <= 0) return;
    noteDialog = { kind };
  }

  async function confirmNote(text: string): Promise<void> {
    const pending = noteDialog;
    noteDialog = null;
    if (!pending) return;
    const sel = currentLogicalSelection();
    if (!sel) return;
    try {
      const [row, utf16] = await ipc.editDisplayPos(tabId, sel.start.row, sel.start.utf16);
      let endUtf16: number | null = null;
      if (sel.start.row === sel.end.row && sel.start.utf16 !== sel.end.utf16) {
        const [endRow, end] = await ipc.editDisplayPos(tabId, sel.end.row, sel.end.utf16);
        if (endRow === row) endUtf16 = end;
      }
      await annotations.addNote(tabId, row, utf16, endUtf16, text, pending.kind as NoteKind);
      toasts.show(pending.kind === 'todo' ? t('annot.todoAdded') : t('annot.noteAdded'));
      focusEditorProxy();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  function handleAction(type: EditActionType): void {
    switch (type) {
      case 'undo':
        void doUndoRedo(false);
        break;
      case 'redo':
        void doUndoRedo(true);
        break;
      case 'cut':
        void doCopy(true);
        break;
      case 'copy':
        void doCopy(false);
        break;
      case 'paste':
        void doPasteFromMenu();
        break;
      case 'selectAll':
        selectAll();
        break;
      case 'find':
        openFind(false);
        break;
      case 'replace':
        openFind(true);
        break;
      case 'batchNumbering':
        batchOpen = true;
        break;
      case 'lineOps':
        lineOpsOpen = true;
        break;
      case 'toggleBookmark':
        void toggleBookmarkAnnotation();
        break;
      case 'highlightSelection':
        void highlightSelectionAnnotation();
        break;
      case 'addNote':
        openNoteDialog('note');
        break;
      case 'addTodo':
        openNoteDialog('todo');
        break;
      case 'clipboardHistory':
        void openClipboardHistory();
        break;
      case 'copyHtml':
        void copyAsHtml();
        break;
      case 'copyMarkdown':
        void copyAsMarkdown();
        break;
      case 'insertTimestamp':
        void insertTimestamp();
        break;
      case 'cleanupTrailingWhitespace':
        void runCleanup('trimTrailingWhitespace', true);
        break;
      case 'cleanupCollapseBlankLines':
        void runCleanup('collapseEmptyLines', true);
        break;
      case 'cleanupTrailingNewline':
        void runCleanup('ensureTrailingNewline', true);
        break;
      case 'cleanupAll':
        void runCleanupAll();
        break;
    }
  }

  // ---- 输入事件 ----

  /** 键盘（导航键与编辑快捷键；可打印字符走 beforeinput）。 */
  function handleKeydown(event: KeyboardEvent): void {
    if (event.isComposing) return;
    const mod = event.ctrlKey || event.metaKey;
    const head = selection.head;
    const extend = event.shiftKey;
    let handled = true;
    // Esc：清空多光标/矩形选择（无多光标时不消费，交还上层）
    if (event.key === 'Escape' && (extraCarets.length > 0 || rectAnchor !== null || rectHead !== null)) {
      clearMulti();
      event.preventDefault();
      return;
    }
    // 导航键：收起多光标状态（与主流编辑器一致）
    if (
      ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown'].includes(
        event.key,
      ) &&
      (extraCarets.length > 0 || rectAnchor !== null)
    ) {
      clearMulti();
    }
    switch (event.key) {
      case 'ArrowLeft':
        goalUtf16 = null;
        setHead(moveLeft(head, rowText), extend);
        break;
      case 'ArrowRight':
        goalUtf16 = null;
        setHead(moveRight(head, rowsTotal, rowText), extend);
        break;
      case 'ArrowUp':
        moveVertically(-1, extend);
        break;
      case 'ArrowDown':
        moveVertically(1, extend);
        break;
      case 'Home':
        goalUtf16 = null;
        setHead(mod ? { row: 0, utf16: 0 } : moveHome(head), extend);
        break;
      case 'End':
        goalUtf16 = null;
        setHead(
          mod ? { row: Math.max(0, rowsTotal - 1), utf16: UTF16_END } : moveEnd(head),
          extend,
        );
        break;
      case 'PageUp':
        moveVertically(-pageRowCount(), extend);
        break;
      case 'PageDown':
        moveVertically(pageRowCount(), extend);
        break;
      case 'Backspace':
        void doBackspace();
        break;
      case 'Delete':
        void doDeleteForward();
        break;
      case 'Enter':
        void handleEnter();
        break;
      case 'Tab':
        void doInsert('\t');
        break;
      default:
        if (mod && (event.key === 'a' || event.key === 'A')) selectAll();
        else if (mod && (event.key === 'z' || event.key === 'Z')) void doUndoRedo(event.shiftKey);
        else if (mod && (event.key === 'y' || event.key === 'Y')) void doUndoRedo(true);
      else if (mod && (event.key === 'c' || event.key === 'C')) void doCopy(false);
      else if (mod && (event.key === 'x' || event.key === 'X')) void doCopy(true);
        else handled = false;
    }
    if (handled) event.preventDefault();
  }

  /** beforeinput：非组合期的文本/删除输入统一走引擎（阻止 textarea 自身修改）。 */
  function handleBeforeInput(event: InputEvent): void {
    if (event.isComposing || composing) return;
    switch (event.inputType) {
      case 'insertText':
        if (event.data) {
          event.preventDefault();
          void handleTypedText(event.data);
        }
        break;
      case 'insertLineBreak':
      case 'insertParagraph':
        event.preventDefault();
        void handleEnter();
        break;
      case 'deleteContentBackward':
        event.preventDefault();
        void doBackspace();
        break;
      case 'deleteContentForward':
        event.preventDefault();
        void doDeleteForward();
        break;
      case 'insertFromPaste':
        // 粘贴统一由 paste 事件处理（此处兜底防止重复插入）
        event.preventDefault();
        break;
      case 'historyUndo':
        event.preventDefault();
        void doUndoRedo(false);
        break;
      case 'historyRedo':
        event.preventDefault();
        void doUndoRedo(true);
        break;
      default:
        break;
    }
  }

  /** 粘贴（IME 之外唯一读取剪贴板的合法路径，无需额外权限）。 */
  async function handlePaste(event: ClipboardEvent): Promise<void> {
    event.preventDefault();
    const text = event.clipboardData?.getData('text/plain') ?? '';
    await doInsert(text);
  }

  function handleCompositionStart(): void {
    composing = true;
    preedit = '';
  }

  function handleCompositionUpdate(event: CompositionEvent): void {
    preedit = event.data;
  }

  /** 组合结束：提交文本（textarea 仅作暂存，立即清空保持受控）。 */
  async function handleCompositionEnd(event: CompositionEvent): Promise<void> {
    composing = false;
    preedit = '';
    const text = event.data ?? '';
    if (textarea) textarea.value = '';
    if (text.length > 0) await doInsert(text);
  }

  // ---- 鼠标 ----

  /** 由屏幕坐标解析编辑器位置。
   *  注意：不能用 caretRangeFromPoint——它命中的是本交互层（最顶层、透明但可命中），
   *  拿不到底层行文本；改用 elementsFromPoint 取行元素，再在行内按字符盒二分定位。 */
  function posFromPoint(clientX: number, clientY: number): CaretPos | null {
    const stack = document.elementsFromPoint(clientX, clientY);
    let rowEl: HTMLElement | null = null;
    for (const el of stack) {
      if (el instanceof HTMLElement && el.classList.contains('row')) {
        rowEl = el;
        break;
      }
    }
    if (!rowEl) {
      // 空白区：取垂直最近的行，按 x 是否越界落 0/行尾
      const page = pageEl();
      let best: HTMLElement | null = null;
      let bestDistance = Number.POSITIVE_INFINITY;
      for (const el of page?.querySelectorAll<HTMLElement>('.row[data-row]') ?? []) {
        const rect = el.getBoundingClientRect();
        const distance = Math.abs(rect.top + rect.height / 2 - clientY);
        if (distance < bestDistance) {
          bestDistance = distance;
          best = el;
        }
      }
      if (!best) return null;
      const row = Number(best.dataset.row ?? -1);
      if (!Number.isFinite(row) || row < 0 || row >= rowsTotal) return null;
      const rect = best.getBoundingClientRect();
      const text = rowText(row) ?? best.querySelector('.txt')?.textContent ?? '';
      const offset = clientX <= rect.left ? 0 : clientX >= rect.right ? text.length : 0;
      return { row, utf16: Math.min(offset, text.length) };
    }
    const row = Number(rowEl.dataset.row ?? -1);
    if (!Number.isFinite(row) || row < 0 || row >= rowsTotal) return null;
    const text = rowText(row) ?? rowEl.querySelector('.txt')?.textContent ?? '';
    const range = document.createRange();
    const charRect = (index: number): DOMRect => {
      const a = textAt(rowEl, index);
      const b = textAt(rowEl, Math.min(index + 1, text.length));
      if (!a || !b) return new DOMRect();
      range.setStart(a.node, a.local);
      range.setEnd(b.node, b.local);
      return range.getBoundingClientRect();
    };
    let lo = 0;
    let hi = text.length;
    while (lo < hi) {
      const mid = Math.floor((lo + hi) / 2);
      const rect = charRect(mid);
      if (rect.width === 0 && rect.height === 0) {
        lo = mid + 1;
      } else if (rect.right <= clientX) {
        lo = mid + 1;
      } else {
        hi = mid;
      }
    }
    let offset = lo;
    if (offset > 0 && offset <= text.length) {
      const prev = charRect(offset - 1);
      if (prev.width > 0 && clientX < prev.left + prev.width / 2) {
        offset -= 1;
      }
    }
    // 代理对中点吸附（避免落在低代理码元上）
    if (offset > 0 && offset < text.length) {
      const prevCode = text.charCodeAt(offset - 1);
      const code = text.charCodeAt(offset);
      if (prevCode >= 0xd800 && prevCode <= 0xdbff && code >= 0xdc00 && code <= 0xdfff) {
        offset -= 1;
      }
    }
    return { row, utf16: Math.min(offset, text.length) };
  }

  function handleMouseDown(event: MouseEvent): void {
    if (event.button !== 0) return;
    focusInput();
    const pos = posFromPoint(event.clientX, event.clientY);
    if (!pos) return;
    event.preventDefault();
    if (rectModifierActive(event)) {
      // 修饰键：单击=追加/移除光标；拖动=矩形选择（拖动开始时才清空附加光标）
      dragging = true;
      dragRect = true;
      rectDragMoved = false;
      rectAnchor = pos;
      rectHead = pos;
      return;
    }
    if (event.shiftKey) {
      clearMulti();
      setSelection({ anchor: selection.anchor, head: pos });
      return;
    }
    clearMulti();
    dragging = true;
    goalUtf16 = null;
    setSelection(collapsed(pos));
  }

  function handleMouseMove(event: MouseEvent): void {
    if (!dragging) return;
    const pos = posFromPoint(event.clientX, event.clientY);
    if (!pos) return;
    if (dragRect && rectAnchor) {
      if (!rectDragMoved) {
        rectDragMoved = true;
        extraCarets = [];
      }
      rectHead = pos;
      return;
    }
    setSelection({ anchor: selection.anchor, head: pos });
  }

  function handleMouseUp(): void {
    if (dragRect && rectAnchor && !rectDragMoved) {
      // 修饰键单击：视作追加/移除附加光标（非矩形）
      extraCarets = toggleCaret(extraCarets, rectAnchor, Math.max(2, multi.maxCount));
      rectAnchor = null;
      rectHead = null;
    }
    dragging = false;
    dragRect = false;
    rectDragMoved = false;
  }

  /** 聚焦隐藏输入框（IME 候选窗跟随其位置）。 */
  function focusInput(): void {
    textarea?.focus();
  }

  /** 焦点守卫：编辑中点击工具栏/菜单/状态栏后，把键盘焦点归还输入代理。
   *  弹窗、菜单组、标签（保持键盘导航）与输入代理自身不抢焦点。 */
  function handleFocusIn(event: FocusEvent): void {
    const target = event.target;
    if (!(target instanceof HTMLElement)) return;
    if (
      target.closest(
        '[role="dialog"],[role="alertdialog"],[role="menu"],[role="search"],.tab,.input-proxy,.find-bar',
      )
    ) {
      return;
    }
    textarea?.focus();
  }

  // ---- 响应式 ----

  // 渲染/行数/选区/组合/多光标变化 → 重算叠加层
  $effect(() => {
    void revision;
    void rowsTotal;
    void selection;
    void composing;
    void preedit;
    void extraCarets;
    void rectAnchor;
    void rectHead;
    refreshOverlay();
  });

  // 挂载（或输入框重建）后聚焦
  $effect(() => {
    textarea?.focus();
  });

  // 外部动作信号（菜单）：seq 变化执行一次
  $effect(() => {
    const action = editorAction;
    if (!action || action.seq === lastActionSeq) return;
    lastActionSeq = action.seq;
    handleAction(action.type);
  });

  // 查询条件/渲染窗口/查找条开关变化 → 防抖刷新文档高亮
  // （version 经 revision prop 传入：滚动换窗、行数据到达、编辑应用都会自增）
  $effect(() => {
    const snapshot = liveQuery;
    void snapshot;
    void revision;
    void findOpen;
    untrack(() => {
      if (matchTimer) clearTimeout(matchTimer);
      if (!findOpen) {
        matchBoxes = [];
        findCountText = '';
        return;
      }
      matchTimer = setTimeout(() => {
        void refreshMatches();
        void refreshCount();
      }, 150);
    });
  });

  // 匹配高亮颜色：设置非空时覆盖主题内置色（空串恢复内置）
  $effect(() => {
    const color = findDefaults.highlightColor.trim();
    void revision;
    const page = pageEl();
    if (!page) return;
    if (color.length > 0) page.style.setProperty('--find-highlight', color);
    else page.style.removeProperty('--find-highlight');
  });

  // 卸载时清理高亮防抖定时器
  $effect(() => {
    return () => {
      if (matchTimer) clearTimeout(matchTimer);
    };
  });
</script>

<NoteDialog
  open={noteDialog !== null}
  title={noteDialog?.kind === 'todo' ? t('menu.edit.addTodo') : t('menu.edit.addNote')}
  onConfirm={(text) => void confirmNote(text)}
  onCancel={() => (noteDialog = null)}
/>

<svelte:window onfocusin={handleFocusIn} />

<div
  class="edit-surface"
  role="presentation"
  onmousedown={handleMouseDown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
  onmouseleave={handleMouseUp}
></div>

{#each matchBoxes as box, index (index)}
  <div
    class="match"
    style="left: {box.left}px; top: {box.top}px; width: {box.width}px; height: {box.height}px"
  ></div>
{/each}

{#each bracketBoxes as box, index (index)}
  <div
    class="bracket-match"
    style="left: {box.left}px; top: {box.top}px; width: {box.width}px; height: {box.height}px"
  ></div>
{/each}

{#each rectBoxes as box, index (index)}
  <div
    class="selection rect"
    style="left: {box.left}px; top: {box.top}px; width: {box.width}px; height: {box.height}px"
  ></div>
{/each}

{#each selBoxes as box, index (index)}
  <div
    class="selection"
    style="left: {box.left}px; top: {box.top}px; width: {box.width}px; height: {box.height}px"
  ></div>
{/each}

{#each extraBoxes as box, index (index)}
  <div
    class="caret extra"
    style="left: {box.left}px; top: {box.top}px; height: {box.height}px"
  ></div>
{/each}

{#if caretBox}
  <div
    class="caret"
    class:composing
    style="left: {caretBox.left}px; top: {caretBox.top}px; height: {caretBox.height}px"
  ></div>
{/if}

{#if composing && preedit && caretBox}
  <div class="preedit" style="left: {caretBox.left}px; top: {caretBox.top}px">{preedit}</div>
{/if}

{#if findOpen}
  <FindBar
    replaceMode={replaceOpen}
    focusSignal={findFocusSeq}
    anchor={getContainer}
    defaults={findDefaults}
    countText={findCountText}
    history={findHistory}
    onFindNext={(q, cs, mode, ww, scope, range) => void doFindNext(q, cs, mode, ww, scope, range)}
    onReplace={(q, r, cs, mode, ww, scope, range) => void doReplace(q, r, cs, mode, ww, scope, range)}
    onReplaceAll={(q, r, cs, mode, ww, scope, range) => void doReplaceAll(q, r, cs, mode, ww, scope, range)}
    onQueryChange={handleQueryChange}
    onClearHistory={() => void clearFindHistory()}
    onClose={closeFind}
  />
{/if}

{#if batchOpen}
  <BatchNumberingDialog
    selectionRows={batchSelectionRows}
    onPreview={previewBatch}
    onApply={applyBatch}
    onClose={() => {
      batchOpen = false;
      focusEditorProxy();
    }}
  />
{/if}
{#if lineOpsOpen}
  <LineOpsDialog
    selectionRows={batchSelectionRows}
    defaults={lineDefaults ?? FALLBACK_LINE_DEFAULTS}
    onPreview={previewLineOps}
    onApply={applyLineOps}
    onClose={() => {
      lineOpsOpen = false;
      focusEditorProxy();
    }}
  />
{/if}
{#if clipboardOpen}
  <ClipboardHistoryDialog
    entries={clipboardEntries}
    onInsert={insertClipboardEntry}
    onRemove={removeClipboardEntry}
    onClear={clearClipboardHistory}
    onClose={() => {
      clipboardOpen = false;
      focusEditorProxy();
    }}
  />
{/if}
{#if previewOpen && previewData}
  <ReplacePreviewDialog
    total={previewData.total}
    truncated={previewData.truncated}
    items={previewData.items}
    onConfirm={confirmPreview}
    onCancel={cancelPreview}
  />
{/if}

<textarea
  bind:this={textarea}
  class="input-proxy"
  style="left: {caretBox?.left ?? 0}px; top: {caretBox?.top ?? 0}px"
            aria-label={t('edit.ariaInput')}
  autocomplete="off"
  autocapitalize="off"
  spellcheck="false"
  onkeydown={handleKeydown}
  onbeforeinput={handleBeforeInput}
  onpaste={handlePaste}
  oncompositionstart={handleCompositionStart}
  oncompositionupdate={handleCompositionUpdate}
  oncompositionend={handleCompositionEnd}
></textarea>

<style>
  .edit-surface {
    position: absolute;
    inset: 0;
    cursor: text;
  }

  .match {
    position: absolute;
    z-index: -2;
    background: var(--find-highlight, rgba(255, 193, 7, 0.38));
    border-radius: 2px;
    pointer-events: none;
  }

  .selection {
    position: absolute;
    z-index: -1;
    background: rgba(59, 110, 165, 0.28);
    pointer-events: none;
  }

  .bracket-match {
    position: absolute;
    outline: 1px solid var(--accent);
    outline-offset: -1px;
    pointer-events: none;
  }
  .selection.rect {
    background: rgba(59, 110, 165, 0.2);
  }

  .caret {
    position: absolute;
    width: 2px;
    background: var(--ink);
    pointer-events: none;
    animation: srt-blink 1.06s step-end infinite;
  }

  .caret.extra {
    opacity: 0.85;
    animation: none;
  }

  .caret.composing {
    animation: none;
  }

  .preedit {
    position: absolute;
    z-index: 2;
    padding: 0 1px;
    color: var(--ink);
    text-decoration: underline;
    text-underline-offset: 3px;
    background: rgba(59, 110, 165, 0.12);
    white-space: pre;
    pointer-events: none;
  }

  .input-proxy {
    position: absolute;
    width: 2px;
    height: 2px;
    margin: 0;
    padding: 0;
    border: 0;
    outline: none;
    resize: none;
    overflow: hidden;
    opacity: 0.01;
    background: transparent;
    color: transparent;
  }

  @keyframes srt-blink {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0;
    }
  }
</style>
