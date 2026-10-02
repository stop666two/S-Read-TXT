<!--
  EditLayer — 编辑交互层（阶段 4b）。
  职责：光标/选区叠加层渲染、隐藏输入框承接键入与 IME、键盘与鼠标交互、
        编辑操作下发（每次输入/按键 = 单个撤销步）。
  坐标：位置 = (行号, 行内 UTF-16 偏移)，与编辑引擎一致；叠加层坐标相对 .page。
-->
<script lang="ts">
  import { tick } from 'svelte';

  import { describeIpcError, ipc, toIpcError, type EditApplied } from '../ipc';
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
  import {
    backspaceOp,
    deleteForwardOp,
    deleteOp,
    insertOp,
    replaceOp,
    selectionText,
  } from '../edit/ops';
  import { toasts } from '../state/toasts.svelte';

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
    /** 确保行文本已加载（返回加载后的文本） */
    ensureRow: (row: number) => Promise<string | undefined>;
    /** 滚动容器查询 */
    getContainer: () => HTMLElement | null;
    /** 编辑结果回报（父组件失效缓存、刷新并同步标签信息） */
    onApplied: (result: EditApplied) => void;
  }
  let { tabId, rowsTotal, revision, rowNode, rowText, ensureRow, getContainer, onApplied }: Props =
    $props();

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

  /** 下发操作并在完成后落定光标、刷新。 */
  async function applyResult(apply: () => Promise<EditApplied>, next: CaretPos): Promise<void> {
    try {
      const result = await apply();
      goalUtf16 = null;
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
      const { start } = orderedSelection(resolved);
      await applyResult(() => ipc.applyEdits(tabId, [replaceOp(resolved, text)]), start);
      return;
    }
    const pos = await resolveLoadedPos(selection.head);
    await applyResult(() => ipc.applyEdits(tabId, [insertOp(pos, text)]), advancePos(pos, text));
  }

  /** 插入后的光标位置（文本含换行时落到末段行）。 */
  function advancePos(pos: CaretPos, text: string): CaretPos {
    const lines = text.split('\n');
    if (lines.length === 1) return { row: pos.row, utf16: pos.utf16 + text.length };
    return { row: pos.row + lines.length - 1, utf16: lines[lines.length - 1].length };
  }

  /** 退格（选区删除或前一个字符/合并上一行）。 */
  async function doBackspace(): Promise<void> {
    if (!isCollapsed(selection)) {
      const resolved = await resolveSelection();
      const { start } = orderedSelection(resolved);
      await applyResult(() => ipc.applyEdits(tabId, [deleteOp(resolved)]), start);
      return;
    }
    const pos = await resolveLoadedPos(selection.head);
    const current = rowText(pos.row) ?? '';
    let prev: string | undefined;
    if (pos.utf16 === 0 && pos.row > 0) {
      prev = rowText(pos.row - 1) ?? (await ensureRow(pos.row - 1)) ?? undefined;
    }
    const op = backspaceOp(pos, current, prev);
    if (!op) return;
    await applyResult(() => ipc.applyEdits(tabId, [op]), {
      row: op.startRow,
      utf16: op.startUtf16,
    });
  }

  /** 前向删除（选区删除或后一个字符/合并下一行）。 */
  async function doDeleteForward(): Promise<void> {
    if (!isCollapsed(selection)) {
      const resolved = await resolveSelection();
      const { start } = orderedSelection(resolved);
      await applyResult(() => ipc.applyEdits(tabId, [deleteOp(resolved)]), start);
      return;
    }
    const pos = await resolveLoadedPos(selection.head);
    const current = rowText(pos.row) ?? '';
    const op = deleteForwardOp(pos, current, pos.row + 1 < rowsTotal);
    if (!op) return;
    await applyResult(() => ipc.applyEdits(tabId, [op]), pos);
  }

  /** 撤销/重做（结果落定到受影响行首）。 */
  async function doUndoRedo(redo: boolean): Promise<void> {
    try {
      const result = redo ? await ipc.redoEdit(tabId) : await ipc.undoEdit(tabId);
      if (!result) return;
      goalUtf16 = null;
      onApplied(result);
      setSelection(
        collapsed(clampPos({ row: result.touchedRow, utf16: 0 }, result.rowsTotal, () => 0)),
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

  /** 收集选区文本（缺行时按需加载；超上限提示分段复制）。 */
  async function gatherSelectedText(): Promise<string | null> {
    const { start, end } = orderedSelection(selection);
    if (end.row - start.row + 1 > COPY_MAX_ROWS) {
      toasts.error('选区过大，请分段复制');
      return null;
    }
    const texts: string[] = [];
    const lastRow = Math.min(end.row, rowsTotal - 1);
    for (let row = start.row; row <= lastRow; row += 1) {
      const text = rowText(row) ?? (await ensureRow(row));
      if (text === undefined) return null;
      texts.push(text);
    }
    return selectionText(selection, rowsTotal, (row) => texts[row - start.row]);
  }

  /** 复制/剪切选区（写系统剪贴板）。 */
  async function doCopy(cut: boolean): Promise<void> {
    if (isCollapsed(selection)) return;
    const text = await gatherSelectedText();
    if (text === null) return;
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      toasts.error('复制失败：系统剪贴板不可用');
      return;
    }
    if (cut) {
      const resolved = await resolveSelection();
      const { start } = orderedSelection(resolved);
      await applyResult(() => ipc.applyEdits(tabId, [deleteOp(resolved)]), start);
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
      target.closest('[role="dialog"],[role="alertdialog"],[role="menu"],.tab,.input-proxy')
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

<textarea
  bind:this={textarea}
  class="input-proxy"
  style="left: {caretBox?.left ?? 0}px; top: {caretBox?.top ?? 0}px"
  aria-label="文本编辑输入"
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
