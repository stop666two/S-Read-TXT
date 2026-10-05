<!--
  LineOpsDialog — 行操作弹窗。
  职责：选择 26 种行操作之一，按操作族显示相关参数（作用范围/排序/去重/缩进/
        大小写/全半角/文本/计数/分隔符），预览前 N 条，确认后由父层下发（单撤销步）。
  坐标：范围与选区均为「显示行序号」（与编辑引擎一致）。
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '../i18n/index.svelte';
  import type { MessageKey } from '../i18n/zh-CN';
  import {
    toIpcError,
    type EditorLinesSettings,
    type LineCaseMode,
    type LineDedupeMode,
    type LineIndentStyle,
    type LineOp,
    type LineOpConfig,
    type LineOpPreview,
    type LineScopeKind,
    type LineSortOrder,
    type LineWidthDirection,
    type BatchScope,
  } from '../ipc';

  interface Props {
    /** 当前选区的显示行范围（无选区时 from==to==当前行） */
    selectionRows: { from: number; to: number };
    /** 设置中的行操作默认值（编辑器设置页可配） */
    defaults: EditorLinesSettings;
    /** 请求预览（错误向上抛出，由本弹窗就地展示） */
    onPreview: (config: LineOpConfig) => Promise<LineOpPreview>;
    /** 执行（单撤销步；成功后由父层提示；失败抛出以保持弹窗打开） */
    onApply: (config: LineOpConfig) => Promise<void>;
    /** 关闭 */
    onClose: () => void;
  }
  let { selectionRows, defaults, onPreview, onApply, onClose }: Props = $props();

  /** 26 种行操作（顺序即下拉展示顺序，按操作族分组排列）。 */
  const OPS: LineOp[] = [
    'moveUp',
    'moveDown',
    'duplicate',
    'delete',
    'merge',
    'split',
    'sort',
    'reverse',
    'dedupe',
    'removeEmptyLines',
    'trimLines',
    'trimTrailingWhitespace',
    'indent',
    'outdent',
    'tabsToSpaces',
    'spacesToTabs',
    'case',
    'widthConvert',
    'prependText',
    'appendText',
    'deleteHead',
    'deleteTail',
    'extractColumn',
    'delimiterConvert',
    'collapseEmptyLines',
    'ensureTrailingNewline',
  ];
  const SORT_ORDERS: LineSortOrder[] = ['lex', 'natural', 'length', 'random'];
  const DEDUPE_MODES: LineDedupeMode[] = ['keepFirst', 'keepLast'];
  const INDENT_STYLES: LineIndentStyle[] = ['spaces', 'tab'];
  const CASE_MODES: LineCaseMode[] = ['upper', 'lower', 'title'];
  const WIDTH_DIRECTIONS: LineWidthDirection[] = ['toHalf', 'toFull'];
  const SCOPES: LineScopeKind[] = ['all', 'currentLine', 'rowRange', 'nonEmpty', 'selection'];

  // 说明：默认值仅在弹窗打开瞬间取一次快照（untrack 显式表达「不跟随 props 变化」意图）。
  let op = $state<LineOp>('trimLines');
  let scopeKind = $state<LineScopeKind>(untrack(() => defaults.defaultScope));
  let rangeFrom = $state(0);
  let rangeTo = $state(0);
  let sortOrder = $state<LineSortOrder>(untrack(() => defaults.sortMode));
  let sortSeed = $state(1);
  let dedupeMode = $state<LineDedupeMode>(untrack(() => defaults.dedupeMode));
  let dedupeIgnoreCase = $state(untrack(() => defaults.dedupeIgnoreCase));
  let dedupeFuzzy = $state(untrack(() => defaults.dedupeFuzzy));
  let indentWidth = $state(untrack(() => defaults.indentWidth));
  let indentStyle = $state(untrack(() => defaults.indentStyle));
  let caseMode = $state<LineCaseMode>(untrack(() => defaults.caseDefault));
  let widthDirection = $state<LineWidthDirection>('toHalf');
  let text = $state('');
  let count = $state(1);
  let delimiter = $state(untrack(() => defaults.columnDelimiter));
  let delimiterTo = $state(',');
  let skipEmpty = $state(untrack(() => defaults.skipEmptyLines));
  let busy = $state(false);
  let errorText = $state('');
  let preview = $state<LineOpPreview | null>(null);

  // ---- 操作族参数显隐（各操作只读取自己的字段；未列出的字段不参与） ----
  const needsScope = $derived(
    !['ensureTrailingNewline'].includes(op),
  );
  /** 支持「跳过空行」的内容类操作（与引擎逐行映射语义一致）。 */
  const SKIP_EMPTY_OPS: LineOp[] = [
    'split',
    'trimLines',
    'trimTrailingWhitespace',
    'indent',
    'outdent',
    'tabsToSpaces',
    'spacesToTabs',
    'case',
    'widthConvert',
    'prependText',
    'appendText',
    'deleteHead',
    'deleteTail',
    'extractColumn',
    'delimiterConvert',
  ];
  const needsSort = $derived(op === 'sort');
  const needsDedupe = $derived(op === 'dedupe');
  const needsIndent = $derived(
    ['indent', 'outdent', 'tabsToSpaces', 'spacesToTabs'].includes(op),
  );
  const needsIndentStyle = $derived(['indent', 'spacesToTabs'].includes(op));
  const needsCase = $derived(op === 'case');
  const needsWidth = $derived(op === 'widthConvert');
  const needsText = $derived(['prependText', 'appendText'].includes(op));
  const needsCount = $derived(['deleteHead', 'deleteTail', 'extractColumn'].includes(op));
  const needsDelimiter = $derived(['split', 'extractColumn', 'delimiterConvert'].includes(op));
  const needsDelimiterTo = $derived(op === 'delimiterConvert');
  const showSkipEmpty = $derived(SKIP_EMPTY_OPS.includes(op));

  /** 组装请求配置（数值取整并钳制到合法下界）。 */
  function config(): LineOpConfig {
    let scope: BatchScope;
    if (!needsScope) {
      scope = { kind: 'all' };
    } else {
      switch (scopeKind) {
        case 'currentLine':
          scope = { kind: 'currentLine', row: selectionRows.from };
          break;
        case 'rowRange':
          scope = {
            kind: 'rowRange',
            from: Math.max(0, Math.trunc(rangeFrom)),
            to: Math.max(0, Math.trunc(rangeTo)),
          };
          break;
        case 'nonEmpty':
          scope = { kind: 'nonEmpty' };
          break;
        case 'selection':
          scope = { kind: 'selection', from: selectionRows.from, to: selectionRows.to };
          break;
        default:
          scope = { kind: 'all' };
      }
    }
    return {
      op,
      scope,
      sortOrder,
      sortSeed: Math.max(0, Math.trunc(sortSeed)),
      dedupeMode,
      dedupeIgnoreCase,
      dedupeFuzzy,
      indentWidth: Math.min(16, Math.max(1, Math.trunc(indentWidth))),
      indentStyle,
      caseMode,
      widthDirection,
      text,
      count: Math.max(0, Math.trunc(count)),
      delimiter,
      delimiterTo,
      skipEmpty,
      previewLines: 10,
    };
  }

  /** 错误对象 → 展示文本（IpcError 载荷或未知错误）。 */
  function errorMessage(error: unknown): string {
    return toIpcError(error).message;
  }

  /** 请求预览（错误就地展示；上次预览结果保留至下次成功）。 */
  async function runPreview(): Promise<void> {
    if (busy) return;
    busy = true;
    errorText = '';
    try {
      preview = await onPreview(config());
    } catch (error) {
      errorText = errorMessage(error);
    } finally {
      busy = false;
    }
  }

  /** 应用（成功后由父层关闭；失败保持打开并提示）。 */
  async function runApply(): Promise<void> {
    if (busy) return;
    busy = true;
    errorText = '';
    try {
      await onApply(config());
      onClose();
    } catch (error) {
      errorText = errorMessage(error);
    } finally {
      busy = false;
    }
  }

  /** Esc 关闭（输入期间同样生效；不触发全局快捷键）。 */
  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.stopPropagation();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="lines-mask">
  <div
    class="lines-dialog"
    role="dialog"
    aria-modal="true"
    aria-label={t('lineOps.title')}
    data-lineops-dialog
  >
    <header class="head">
      <span class="title">{t('lineOps.title')}</span>
      <button class="close" aria-label={t('lineOps.close')} onclick={onClose}>×</button>
    </header>
    <div class="body">
      <div class="grid">
        <label class="field wide">
          <span>{t('lineOps.param.op')}</span>
          <select bind:value={op} data-setting="lines.op">
            {#each OPS as value}
              <option value={value}>{t(`lineOps.op.${value}` as MessageKey)}</option>
            {/each}
          </select>
        </label>
        {#if needsScope}
          <label class="field">
            <span>{t('lineOps.param.scope')}</span>
            <select bind:value={scopeKind} data-setting="lines.scope">
              {#each SCOPES as value}
                <option value={value}>{t(`batch.scope.${value}` as MessageKey)}</option>
              {/each}
            </select>
          </label>
        {/if}
        {#if needsScope && scopeKind === 'rowRange'}
          <label class="field">
            <span>{t('batch.rangeFrom')}</span>
            <input type="number" min="0" bind:value={rangeFrom} data-setting="lines.rangeFrom" />
          </label>
          <label class="field">
            <span>{t('batch.rangeTo')}</span>
            <input type="number" min="0" bind:value={rangeTo} data-setting="lines.rangeTo" />
          </label>
        {/if}
        {#if needsSort}
          <label class="field">
            <span>{t('lineOps.param.sortOrder')}</span>
            <select bind:value={sortOrder} data-setting="lines.sortOrder">
              {#each SORT_ORDERS as value}
                <option value={value}>{t(`lineOps.enum.sortOrder.${value}` as MessageKey)}</option>
              {/each}
            </select>
          </label>
          {#if sortOrder === 'random'}
            <label class="field">
              <span>{t('lineOps.param.sortSeed')}</span>
              <input type="number" min="0" bind:value={sortSeed} data-setting="lines.sortSeed" />
            </label>
          {/if}
        {/if}
        {#if needsDedupe}
          <label class="field">
            <span>{t('lineOps.param.dedupeMode')}</span>
            <select bind:value={dedupeMode} data-setting="lines.dedupeMode">
              {#each DEDUPE_MODES as value}
                <option value={value}>{t(`lineOps.enum.dedupeMode.${value}` as MessageKey)}</option>
              {/each}
            </select>
          </label>
          <label class="field check">
            <input type="checkbox" bind:checked={dedupeIgnoreCase} data-setting="lines.dedupeIgnoreCase" />
            <span>{t('lineOps.param.dedupeIgnoreCase')}</span>
          </label>
          <label class="field check">
            <input type="checkbox" bind:checked={dedupeFuzzy} data-setting="lines.dedupeFuzzy" />
            <span>{t('lineOps.param.dedupeFuzzy')}</span>
          </label>
        {/if}
        {#if needsIndent}
          <label class="field">
            <span>{t('lineOps.param.indentWidth')}</span>
            <input type="number" min="1" max="16" bind:value={indentWidth} data-setting="lines.indentWidth" />
          </label>
        {/if}
        {#if needsIndentStyle}
          <label class="field">
            <span>{t('lineOps.param.indentStyle')}</span>
            <select bind:value={indentStyle} data-setting="lines.indentStyle">
              {#each INDENT_STYLES as value}
                <option value={value}>{t(`lineOps.enum.indentStyle.${value}` as MessageKey)}</option>
              {/each}
            </select>
          </label>
        {/if}
        {#if needsCase}
          <label class="field">
            <span>{t('lineOps.param.caseMode')}</span>
            <select bind:value={caseMode} data-setting="lines.caseMode">
              {#each CASE_MODES as value}
                <option value={value}>{t(`lineOps.enum.caseMode.${value}` as MessageKey)}</option>
              {/each}
            </select>
          </label>
        {/if}
        {#if needsWidth}
          <label class="field">
            <span>{t('lineOps.param.widthDirection')}</span>
            <select bind:value={widthDirection} data-setting="lines.widthDirection">
              {#each WIDTH_DIRECTIONS as value}
                <option value={value}>{t(`lineOps.enum.widthDirection.${value}` as MessageKey)}</option>
              {/each}
            </select>
          </label>
        {/if}
        {#if needsText}
          <label class="field wide">
            <span>{t('lineOps.param.text')}</span>
            <input type="text" bind:value={text} data-setting="lines.text" />
          </label>
        {/if}
        {#if needsCount}
          <label class="field">
            <span>{t('lineOps.param.count')}</span>
            <input type="number" min="0" bind:value={count} data-setting="lines.count" />
          </label>
        {/if}
        {#if needsDelimiter}
          <label class="field">
            <span>{t('lineOps.param.delimiter')}</span>
            <input type="text" bind:value={delimiter} data-setting="lines.delimiter" />
          </label>
        {/if}
        {#if needsDelimiterTo}
          <label class="field">
            <span>{t('lineOps.param.delimiterTo')}</span>
            <input type="text" bind:value={delimiterTo} data-setting="lines.delimiterTo" />
          </label>
        {/if}
        {#if showSkipEmpty}
          <label class="field check">
            <input type="checkbox" bind:checked={skipEmpty} data-setting="lines.skipEmpty" />
            <span>{t('lineOps.param.skipEmpty')}</span>
          </label>
        {/if}
      </div>
      {#if errorText}
        <p class="err" data-lineops-error>{errorText}</p>
      {/if}
      {#if preview}
        <div class="preview" data-lineops-preview>
          <div class="preview-title">
            {t('lineOps.previewTitle', { shown: preview.items.length, total: preview.affectedRows })}
          </div>
          {#if preview.warning}
            <p class="warn" data-lineops-warning>{preview.warning}</p>
          {/if}
          {#if preview.items.length === 0}
            <p class="empty">{t('lineOps.previewEmpty')}</p>
          {:else}
            {#each preview.items as item (item.row)}
              <div class="item" data-lineops-item>
                <span class="no">{item.row}</span>
                <span class="text">{item.text}</span>
              </div>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
    <footer class="foot">
      <button class="btn" disabled={busy} data-setting="lines.preview" onclick={() => void runPreview()}>
        {t('lineOps.preview')}
      </button>
      <button class="btn primary" disabled={busy} data-setting="lines.apply" onclick={() => void runApply()}>
        {t('lineOps.apply')}
      </button>
      <button class="btn" disabled={busy} data-setting="lines.close" onclick={onClose}>
        {t('lineOps.close')}
      </button>
    </footer>
  </div>
</div>

<style>
  .lines-mask {
    position: fixed;
    inset: var(--h-titlebar) 0 0 0;
    background: rgb(0 0 0 / 0.32);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 40;
  }

  .lines-dialog {
    width: min(640px, calc(100% - 48px));
    max-height: min(680px, calc(100% - 48px));
    display: flex;
    flex-direction: column;
    background: var(--surface);
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgb(0 0 0 / 0.18);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
  }

  .title {
    font-size: 13px;
    font-weight: 600;
  }

  .close {
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    font-size: 16px;
    cursor: pointer;
  }

  .close:hover {
    background: var(--hover);
    color: var(--ink);
  }

  .body {
    padding: 12px 14px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 12px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 12px;
    color: var(--muted);
  }

  .field.wide {
    grid-column: span 2;
  }

  .field.check {
    flex-direction: row;
    align-items: center;
    gap: 6px;
    padding-top: 16px;
  }

  .field input,
  .field select {
    height: 26px;
    background: var(--base);
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 0 6px;
    font-size: 12px;
    font-family: inherit;
  }

  .err {
    margin: 0;
    font-size: 12px;
    color: #c0392b;
  }

  .preview {
    border: 1px solid var(--line);
    border-radius: 8px;
    max-height: 220px;
    overflow: auto;
  }

  .preview-title {
    position: sticky;
    top: 0;
    padding: 6px 10px;
    font-size: 12px;
    color: var(--muted);
    background: var(--surface);
    border-bottom: 1px solid var(--line);
  }

  .warn {
    margin: 0;
    padding: 6px 10px;
    font-size: 12px;
    color: #b26a00;
  }

  .empty {
    margin: 0;
    padding: 10px;
    font-size: 12px;
    color: var(--muted);
  }

  .item {
    display: flex;
    gap: 8px;
    padding: 4px 10px;
    font-size: 12px;
    border-bottom: 1px dashed var(--line);
  }

  .item:last-child {
    border-bottom: none;
  }

  .no {
    color: var(--muted);
    min-width: 52px;
    text-align: right;
  }

  .text {
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid var(--line);
  }

  .btn {
    height: 28px;
    padding: 0 14px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--base);
    color: var(--ink);
    font-size: 12px;
    cursor: pointer;
  }

  .btn:hover:not(:disabled) {
    background: var(--hover);
  }

  .btn:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
</style>
