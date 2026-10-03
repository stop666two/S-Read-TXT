<!--
  EditLayer — 编辑交互层（阶段 4b）。
  职责：光标/选区叠加层渲染、隐藏输入框承接键入与 IME、键盘与鼠标交互、
        编辑操作下发（每次输入/按键 = 单个撤销步）。
  坐标：位置 = (行号, 行内 UTF-16 偏移)，与编辑引擎一致；叠加层坐标相对 .page。
-->
<script lang="ts">
  import { tick, untrack } from 'svelte';

  import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager';

  import { describeIpcError, ipc, toIpcError, type EditApplied, type FindHit, type ReplacePreview, type SearchMode } from '../ipc';
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
  import { planBackspace, planDeleteForward, type SegMeta } from '../edit/longline';
  import { deleteOp, deleteRangeOp, insertOp, replaceOp } from '../edit/ops';
  import { t } from '../i18n/index.svelte';
  import { toasts } from '../state/toasts.svelte';
  import FindBar from './FindBar.svelte';
  import ReplacePreviewDialog from './ReplacePreviewDialog.svelte';
  import type { EditActionType, EditorAction } from '../edit/actions';

  interface Props {
    /** 当前标签 id */
    tabId: number;
    /** 总行数（编辑后随父组件更新） */
    rowsTotal: number;
    /** 渲染版本号（文本到达/测量变化时驱动叠加层重算） */
    revision: number;
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
    onApplied,
    editorAction,
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
  /** 最近一次查询条件（FindBar 上报；驱动高亮刷新） */
  let liveQuery = $state<{ query: string; caseSensitive: boolean; mode: SearchMode }>({
    query: '',
    caseSensitive: false,
    mode: 'literal',
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
  } | null = null;
  /** 预览弹窗开关 */
  let previewOpen = $state(false);

  // ---- 坐标与渲染 ----

  /** .page 元素（叠加层定位基准）。 */
  function pageEl(): HTMLElement | null {
    const page = getContainer()?.querySelector('.page');
    return page instanceof HTMLElement ? page : null;
  }

  /** 光标盒（相对 .page）；结点缺失或空文档时为 null。 */
  function caretRect(pageRect: DOMRect): Box | null {
    const node = rowNode(selection.head.row);
    if (!node) return null;
    const text = rowText(selection.head.row);
    const offset = text === undefined ? 0 : Math.min(Math.max(0, selection.head.utf16), text.length);
    const range = document.createRange();
    const textNode = node.firstChild;
    if (textNode instanceof Text) {
      range.setStart(textNode, Math.min(offset, textNode.length));
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

  /** 选区盒列表（仅当前已渲染行；跨行逐段测量）。 */
  function selectionRects(pageRect: DOMRect): Box[] {
    if (isCollapsed(selection)) return [];
    const { start, end } = orderedSelection(selection);
    const boxes: Box[] = [];
    const lastRow = Math.min(end.row, rowsTotal - 1);
    for (let row = start.row; row <= lastRow; row += 1) {
      const node = rowNode(row);
      const text = rowText(row);
      if (!node || text === undefined) continue;
      const textNode = node.firstChild;
      if (!(textNode instanceof Text)) continue;
      const from = row === start.row ? Math.min(start.utf16, text.length) : 0;
      const to = row === end.row ? Math.min(end.utf16, text.length) : text.length;
      if (to <= from) continue;
      const range = document.createRange();
      range.setStart(textNode, from);
      range.setEnd(textNode, to);
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
    return boxes;
  }

  /** 重算光标与选区叠加层。 */
  function refreshOverlay(): void {
    const page = pageEl();
    if (!page) {
      caretBox = null;
      selBoxes = [];
      return;
    }
    const pageRect = page.getBoundingClientRect();
    caretBox = caretRect(pageRect);
    selBoxes = selectionRects(pageRect);
  }

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

  /** 插入文本（含选区替换；每次 = 单个撤销步）。 */
  async function doInsert(text: string): Promise<void> {
    if (text.length === 0) return;
    if (!isCollapsed(selection)) {
      const resolved = await resolveSelection();
      await applyResult(() => ipc.applyEdits(tabId, [replaceOp(logicalSelection(resolved), text)]));
      return;
    }
    const pos = await resolveLoadedPos(selection.head);
    await applyResult(() => ipc.applyEdits(tabId, [insertOp(logicalOf(pos), text)]));
  }

  /** 退格（选区删除或前一个字符/合并上一行）。 */
  async function doBackspace(): Promise<void> {
    if (!isCollapsed(selection)) {
      const resolved = await resolveSelection();
      await applyResult(() => ipc.applyEdits(tabId, [deleteOp(logicalSelection(resolved))]));
      return;
    }
    const pos = await resolveLoadedPos(selection.head);
    const cur = segOf(pos.row);
    if (!cur) return;
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

  /** 前向删除（选区删除或后一个字符/合并下一行）。 */
  async function doDeleteForward(): Promise<void> {
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

  /** 复制/剪切选区（写系统剪贴板；经 Tauri 剪贴板插件，无浏览器权限弹窗）。 */
  async function doCopy(cut: boolean): Promise<void> {
    if (isCollapsed(selection)) return;
    const text = await gatherSelectedText();
    if (text === null) return;
    try {
      await writeText(text);
    } catch {
      toasts.error(t('edit.copyFailed'));
      return;
    }
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
  }

  /** 关闭查找条、清除高亮并归还键盘焦点。 */
  function closeFind(): void {
    findOpen = false;
    matchBoxes = [];
    focusInput();
  }

  /** 把命中设为选区（显示坐标）并滚动可见。 */
  function selectHit(hit: FindHit): void {
    setSelection({
      anchor: { row: hit.startRow, utf16: hit.startUtf16 },
      head: { row: hit.endRow, utf16: hit.endUtf16 },
    });
  }

  /** 查询条件变化（FindBar 上报）：记录，高亮由防抖 effect 统一刷新。 */
  function handleQueryChange(query: string, caseSensitive: boolean, mode: SearchMode): void {
    liveQuery = { query, caseSensitive, mode };
  }

  /** 可见行窗口内全部命中 → 高亮盒（正则语法错误等静默；动作路径有 toast 反馈）。 */
  async function refreshMatches(): Promise<void> {
    const { query, caseSensitive, mode } = liveQuery;
    if (!findOpen || query.length === 0) {
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
        minRow,
        maxRow - minRow + 1,
      );
      if (seq !== matchSeq || !findOpen) return;
      const page = pageEl();
      if (!page) return;
      const pageRect = page.getBoundingClientRect();
      const boxes: Box[] = [];
      for (const hit of hits.slice(0, MATCH_HIGHLIGHT_MAX)) {
        const lastRow = Math.min(hit.endRow, maxRow);
        for (let row = hit.startRow; row <= lastRow; row += 1) {
          const node = rowNode(row);
          const text = rowText(row);
          if (!node || text === undefined || !(node.firstChild instanceof Text)) continue;
          const from = row === hit.startRow ? Math.min(hit.startUtf16, text.length) : 0;
          const to = row === hit.endRow ? Math.min(hit.endUtf16, text.length) : text.length;
          if (to <= from) continue;
          const range = document.createRange();
          range.setStart(node.firstChild, from);
          range.setEnd(node.firstChild, to);
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

  /** 查找下一个：从当前光标起；到文末未命中时从头再试一次。 */
  async function doFindNext(
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
  ): Promise<void> {
    if (query.length === 0) return;
    try {
      const head = selection.head;
      let hit = await ipc.findInEdit(tabId, query, caseSensitive, mode, [head.row, head.utf16]);
      if (!hit) hit = await ipc.findInEdit(tabId, query, caseSensitive, mode, null);
      if (!hit) {
        toasts.error(t('find.notFound', { query }));
        return;
      }
      selectHit(hit);
      void ensureRow(hit.startRow);
      void ensureRow(hit.endRow);
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[edit] 查找失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 替换一个：优先替换当前选区起点处的命中，否则替换光标后的第一个；随后选中下一个。 */
  async function doReplace(
    query: string,
    replacement: string,
    caseSensitive: boolean,
    mode: SearchMode,
  ): Promise<void> {
    if (query.length === 0) return;
    try {
      const start = orderedSelection(selection).start;
      const outcome = await ipc.replaceInEdit(
        tabId,
        query,
        caseSensitive,
        mode,
        [start.row, start.utf16],
        replacement,
      );
      if (!outcome) {
        toasts.error(t('find.notFound', { query }));
        return;
      }
      applyOutcome(outcome.applied);
      // 替换会改变文档：把键盘焦点交还编辑器（否则 Ctrl+Z 等编辑按键落在查找条上无效）
      focusEditorProxy();
      if (outcome.next) {
        selectHit(outcome.next);
        void ensureRow(outcome.next.startRow);
      }
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[edit] 替换失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 全部替换：先预览；命中 =0 提示，=1 直接执行，≥2 弹二次确认（可逐条剔除）。 */
  async function doReplaceAll(
    query: string,
    replacement: string,
    caseSensitive: boolean,
    mode: SearchMode,
  ): Promise<void> {
    if (query.length === 0) return;
    try {
      const preview = await ipc.previewReplaceAll(tabId, query, caseSensitive, mode, replacement);
      if (preview.total === 0) {
        toasts.error(t('find.notFound', { query }));
        return;
      }
      if (preview.total === 1) {
        await applyReplaceAll(preview, query, replacement, caseSensitive, mode, null);
        return;
      }
      previewData = preview;
      previewArgs = { query, replacement, caseSensitive, mode };
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
    selected: number[] | null,
  ): Promise<void> {
    try {
      const outcome = await ipc.applyReplaceAll(
        tabId,
        query,
        caseSensitive,
        mode,
        replacement,
        selected,
        preview.stateId,
      );
      if (outcome.applied) applyOutcome(outcome.applied);
      focusEditorProxy();
      toasts.show(t('edit.replaceDone', { count: outcome.replaced }));
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

  /** 外部动作分发（菜单触发）。 */
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
        void doInsert('\n');
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
          void doInsert(event.data);
        }
        break;
      case 'insertLineBreak':
      case 'insertParagraph':
        event.preventDefault();
        void doInsert('\n');
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

  /** 由屏幕坐标解析编辑器位置（caretRangeFromPoint + 最近行元素）。 */
  function posFromPoint(clientX: number, clientY: number): CaretPos | null {
    const range = document.caretRangeFromPoint(clientX, clientY);
    if (!range) return null;
    let element: Node | null = range.startContainer;
    if (element.nodeType === Node.TEXT_NODE) element = element.parentElement;
    const rowEl = element instanceof Element ? element.closest('.row') : null;
    if (!(rowEl instanceof HTMLElement)) return null;
    const row = Number(rowEl.dataset.row ?? -1);
    if (!Number.isFinite(row) || row < 0 || row >= rowsTotal) return null;
    const text = rowText(row) ?? rowEl.textContent ?? '';
    let offset = 0;
    if (range.startContainer === rowEl.firstChild) offset = range.startOffset;
    else if (range.startContainer === rowEl) offset = range.startOffset > 0 ? text.length : 0;
    return { row, utf16: Math.min(offset, text.length) };
  }

  function handleMouseDown(event: MouseEvent): void {
    if (event.button !== 0) return;
    focusInput();
    const pos = posFromPoint(event.clientX, event.clientY);
    if (!pos) return;
    event.preventDefault();
    if (event.shiftKey) {
      setSelection({ anchor: selection.anchor, head: pos });
      return;
    }
    dragging = true;
    goalUtf16 = null;
    setSelection(collapsed(pos));
  }

  function handleMouseMove(event: MouseEvent): void {
    if (!dragging) return;
    const pos = posFromPoint(event.clientX, event.clientY);
    if (pos) setSelection({ anchor: selection.anchor, head: pos });
  }

  function handleMouseUp(): void {
    dragging = false;
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

  // 渲染/行数/选区/组合变化 → 重算叠加层
  $effect(() => {
    void revision;
    void rowsTotal;
    void selection;
    void composing;
    void preedit;
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
        return;
      }
      matchTimer = setTimeout(() => {
        void refreshMatches();
      }, 150);
    });
  });

  // 卸载时清理高亮防抖定时器
  $effect(() => {
    return () => {
      if (matchTimer) clearTimeout(matchTimer);
    };
  });
</script>

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

{#each selBoxes as box, index (index)}
  <div
    class="selection"
    style="left: {box.left}px; top: {box.top}px; width: {box.width}px; height: {box.height}px"
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
    onFindNext={(q, cs, mode) => void doFindNext(q, cs, mode)}
    onReplace={(q, r, cs, mode) => void doReplace(q, r, cs, mode)}
    onReplaceAll={(q, r, cs, mode) => void doReplaceAll(q, r, cs, mode)}
    onQueryChange={handleQueryChange}
    onClose={closeFind}
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
    background: rgba(255, 193, 7, 0.38);
    border-radius: 2px;
    pointer-events: none;
  }

  .selection {
    position: absolute;
    z-index: -1;
    background: rgba(59, 110, 165, 0.28);
    pointer-events: none;
  }

  .caret {
    position: absolute;
    width: 2px;
    background: var(--ink);
    pointer-events: none;
    animation: srt-blink 1.06s step-end infinite;
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
