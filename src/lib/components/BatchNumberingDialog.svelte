<!--
  BatchNumberingDialog — 批量插入序号弹窗。
  职责：配置编号格式/起始/步长/补零/分隔符/后缀/插入位置/作用范围/模板，
        预览前 N 条，确认后由父层下发（单撤销步）。
  坐标：范围与选区均为「显示行序号」（与编辑引擎一致）。
-->
<script lang="ts">
  import { t } from '../i18n/index.svelte';
  import type { MessageKey } from '../i18n/zh-CN';
  import {
    toIpcError,
    type BatchInsertPosition,
    type BatchNumberFormat,
    type BatchNumberingConfig,
    type BatchPreview,
    type BatchScope,
  } from '../ipc';

  interface Props {
    /** 当前选区的显示行范围（无选区时 from==to==当前行） */
    selectionRows: { from: number; to: number };
    /** 请求预览（错误向上抛出，由本弹窗就地展示） */
    onPreview: (config: BatchNumberingConfig) => Promise<BatchPreview>;
    /** 执行（单撤销步；成功后由父层提示；失败抛出以保持弹窗打开） */
    onApply: (config: BatchNumberingConfig) => Promise<void>;
    /** 关闭 */
    onClose: () => void;
  }
  let { selectionRows, onPreview, onApply, onClose }: Props = $props();

  const FORMATS: BatchNumberFormat[] = [
    'arabic',
    'zeroPad',
    'chineseLower',
    'chineseUpper',
    'parenthesized',
    'bracketed',
    'circled',
    'circledFilled',
    'romanUpper',
    'romanLower',
  ];
  const POSITIONS: BatchInsertPosition[] = ['lineStart', 'lineEnd'];
  const SCOPES = ['all', 'currentLine', 'rowRange', 'nonEmpty', 'selection'] as const;
  type ScopeKind = (typeof SCOPES)[number];

  let format = $state<BatchNumberFormat>('arabic');
  let start = $state(1);
  let step = $state(1);
  let zeroPadWidth = $state(2);
  let separator = $state('.');
  let suffix = $state('');
  let position = $state<BatchInsertPosition>('lineStart');
  let scopeKind = $state<ScopeKind>('all');
  let rangeFrom = $state(0);
  let rangeTo = $state(0);
  let skipEmpty = $state(true);
  let template = $state('');
  let busy = $state(false);
  let errorText = $state('');
  let preview = $state<BatchPreview | null>(null);

  /** 组装请求配置（模板空串 → null；数值取整并钳制到合法下界）。 */
  function config(): BatchNumberingConfig {
    let scope: BatchScope;
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
    const trimmed = template.trim();
    return {
      format,
      start: Math.max(0, Math.trunc(start)),
      step: Math.max(1, Math.trunc(step)),
      zeroPadWidth: Math.max(1, Math.trunc(zeroPadWidth)),
      separator,
      suffix,
      position,
      scope,
      skipEmpty,
      template: trimmed.length > 0 ? trimmed : null,
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

<div class="batch-mask">
  <div class="batch-dialog" role="dialog" aria-modal="true" aria-label={t('batch.title')} data-batch-dialog>
    <header class="head">
      <span class="title">{t('batch.title')}</span>
      <button class="close" aria-label={t('batch.close')} onclick={onClose}>×</button>
    </header>
    <div class="body">
      <div class="grid">
        <label class="field">
          <span>{t('batch.format')}</span>
          <select bind:value={format} data-setting="batch.format">
            {#each FORMATS as value}
              <option value={value}>{t(`batch.format.${value}` as MessageKey)}</option>
            {/each}
          </select>
        </label>
        <label class="field">
          <span>{t('batch.position')}</span>
          <select bind:value={position} data-setting="batch.position">
            {#each POSITIONS as value}
              <option value={value}>{t(`batch.position.${value}` as MessageKey)}</option>
            {/each}
          </select>
        </label>
        <label class="field">
          <span>{t('batch.start')}</span>
          <input type="number" min="0" bind:value={start} data-setting="batch.start" />
        </label>
        <label class="field">
          <span>{t('batch.step')}</span>
          <input type="number" min="1" bind:value={step} data-setting="batch.step" />
        </label>
        {#if format === 'zeroPad'}
          <label class="field">
            <span>{t('batch.zeroPad')}</span>
            <input type="number" min="1" bind:value={zeroPadWidth} data-setting="batch.zeroPad" />
          </label>
        {/if}
        <label class="field">
          <span>{t('batch.separator')}</span>
          <input type="text" bind:value={separator} data-setting="batch.separator" />
        </label>
        <label class="field">
          <span>{t('batch.suffix')}</span>
          <input type="text" bind:value={suffix} data-setting="batch.suffix" />
        </label>
        <label class="field">
          <span>{t('batch.scope')}</span>
          <select bind:value={scopeKind} data-setting="batch.scope">
            {#each SCOPES as value}
              <option value={value}>{t(`batch.scope.${value}` as MessageKey)}</option>
            {/each}
          </select>
        </label>
        {#if scopeKind === 'rowRange'}
          <label class="field">
            <span>{t('batch.rangeFrom')}</span>
            <input type="number" min="0" bind:value={rangeFrom} data-setting="batch.rangeFrom" />
          </label>
          <label class="field">
            <span>{t('batch.rangeTo')}</span>
            <input type="number" min="0" bind:value={rangeTo} data-setting="batch.rangeTo" />
          </label>
        {/if}
        <label class="field check">
          <input type="checkbox" bind:checked={skipEmpty} data-setting="batch.skipEmpty" />
          <span>{t('batch.skipEmpty')}</span>
        </label>
      </div>
      <label class="field">
        <span>{t('batch.template')}</span>
        <input type="text" bind:value={template} data-setting="batch.template" />
      </label>
      <p class="hint">{t('batch.templateHint')}</p>
      {#if errorText}
        <p class="err" data-batch-error>{errorText}</p>
      {/if}
      {#if preview}
        <div class="preview" data-batch-preview>
          <div class="preview-title">
            {t('batch.previewTitle', { shown: preview.items.length, total: preview.totalRows })}
          </div>
          {#if preview.items.length === 0}
            <p class="empty">{t('batch.previewEmpty')}</p>
          {:else}
            {#each preview.items as item (item.row)}
              <div class="item" data-batch-item>
                <span class="no">{item.row}</span>
                <span class="badge">{item.insertText}</span>
                <span class="after">{item.after}</span>
              </div>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
    <footer class="foot">
      <button class="btn" disabled={busy} data-setting="batch.preview" onclick={() => void runPreview()}>
        {t('batch.preview')}
      </button>
      <button class="btn primary" disabled={busy} data-setting="batch.apply" onclick={() => void runApply()}>
        {t('batch.apply')}
      </button>
      <button class="btn" disabled={busy} data-setting="batch.close" onclick={onClose}>
        {t('batch.close')}
      </button>
    </footer>
  </div>
</div>

<style>
  .batch-mask {
    position: fixed;
    inset: var(--h-titlebar) 0 0 0;
    background: rgb(0 0 0 / 0.32);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 40;
  }

  .batch-dialog {
    width: min(600px, calc(100% - 48px));
    max-height: min(640px, calc(100% - 48px));
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

  .hint {
    margin: 0;
    font-size: 11px;
    color: var(--muted);
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

  .badge {
    background: var(--hover);
    color: var(--ink);
    border-radius: 4px;
    padding: 0 4px;
    white-space: nowrap;
  }

  .after {
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
