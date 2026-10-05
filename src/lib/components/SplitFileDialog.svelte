<script lang="ts">
  // SplitFileDialog —— 「工具 → 拆分文件…」对话框：
  // 填写源文件与拆分方式（行数 / 标记），预览分片后执行；结果可一键打开第一份。
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { formatBytes } from '../format';
  import { t } from '../i18n/index.svelte';
  import { ipc, toIpcError, type SplitMode, type SplitPreview, type SplitResult } from '../ipc';
  import { toasts } from '../state/toasts.svelte';

  interface Props {
    /** 当前激活标签的磁盘路径（无则空字符串） */
    currentPath: string | null;
    onClose: () => void;
  }

  let { currentPath, onClose }: Props = $props();

  let sourcePath = $state(currentPath ?? '');
  let mode = $state<'lines' | 'marker'>('lines');
  let linesPerFile = $state(1000);
  let marker = $state('');
  let isRegex = $state(false);
  let outDir = $state('');
  let preview = $state<SplitPreview | null>(null);
  let result = $state<SplitResult | null>(null);
  let busy = $state(false);
  let error = $state('');

  const buildMode = (): SplitMode =>
    mode === 'lines'
      ? { kind: 'lines', linesPerFile: Math.max(1, Math.floor(linesPerFile) || 1) }
      : { kind: 'marker', marker, isRegex };

  async function useCurrent(): Promise<void> {
    if (currentPath) sourcePath = currentPath;
  }

  async function pickFile(): Promise<void> {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: t('app.filter.text'), extensions: ['txt', 'log', 'md'] }],
    });
    if (typeof picked === 'string') sourcePath = picked;
  }

  async function pickDir(): Promise<void> {
    const picked = await openDialog({ directory: true, multiple: false });
    if (typeof picked === 'string') outDir = picked;
  }

  async function doPreview(): Promise<void> {
    if (busy) return;
    const path = sourcePath.trim();
    if (!path) {
      error = t('tools.split.sourceRequired');
      return;
    }
    busy = true;
    error = '';
    result = null;
    try {
      preview = await ipc.previewSplit(path, buildMode(), outDir.trim() || undefined);
    } catch (caught) {
      preview = null;
      error = toIpcError(caught).message;
    } finally {
      busy = false;
    }
  }

  async function doApply(): Promise<void> {
    if (busy) return;
    if (!preview) {
      error = t('tools.split.previewFirst');
      return;
    }
    busy = true;
    error = '';
    try {
      result = await ipc.applySplit(sourcePath.trim(), buildMode(), outDir.trim() || undefined);
      toasts.show(
        t('tools.split.done', { count: result.files.length, bytes: formatBytes(result.bytes) }),
      );
    } catch (caught) {
      error = toIpcError(caught).message;
    } finally {
      busy = false;
    }
  }

  function openFirst(): void {
    const first = result?.files[0];
    if (first) void window.__srt?.openPath(first);
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && !busy) {
      event.preventDefault();
      event.stopPropagation();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="mask"
  role="dialog"
  aria-modal="true"
  aria-label={t('tools.split.title')}
  data-split-dialog
  onmousedown={(event) => {
    if (event.target === event.currentTarget && !busy) onClose();
  }}
>
  <div class="panel">
    <h2>{t('tools.split.title')}</h2>

    <label class="field">
      <span>{t('tools.split.source')}</span>
      <input
        type="text"
        data-split-path
        placeholder={t('tools.split.sourcePlaceholder')}
        bind:value={sourcePath}
        disabled={busy}
      />
    </label>
    <div class="row">
      <button type="button" data-split-use-current disabled={busy || !currentPath} onclick={useCurrent}>
        {t('tools.split.useCurrent')}
      </button>
      <button type="button" data-split-pick-file disabled={busy} onclick={() => void pickFile()}>
        {t('tools.split.pickFile')}
      </button>
    </div>

    <fieldset class="mode">
      <legend>{t('tools.split.mode')}</legend>
      <label>
        <input type="radio" value="lines" bind:group={mode} data-split-mode="lines" disabled={busy} />
        <span>{t('tools.split.modeLines')}</span>
      </label>
      <label>
        <input type="radio" value="marker" bind:group={mode} data-split-mode="marker" disabled={busy} />
        <span>{t('tools.split.modeMarker')}</span>
      </label>
    </fieldset>

    {#if mode === 'lines'}
      <label class="field">
        <span>{t('tools.split.linesPerFile')}</span>
        <input type="number" min="1" max="1000000" data-split-lines bind:value={linesPerFile} disabled={busy} />
      </label>
    {:else}
      <label class="field">
        <span>{t('tools.split.marker')}</span>
        <input
          type="text"
          data-split-marker
          placeholder={t('tools.split.markerPlaceholder')}
          bind:value={marker}
          disabled={busy}
        />
      </label>
      <label class="check">
        <input type="checkbox" data-split-regex bind:checked={isRegex} disabled={busy} />
        <span>{t('tools.split.markerRegex')}</span>
      </label>
    {/if}

    <label class="field">
      <span>{t('tools.split.outDir')}</span>
      <input
        type="text"
        data-split-outdir
        placeholder={t('tools.split.outDirPlaceholder')}
        bind:value={outDir}
        disabled={busy}
      />
    </label>
    <div class="row">
      <button type="button" data-split-pick-dir disabled={busy} onclick={() => void pickDir()}>
        {t('tools.split.pickDir')}
      </button>
    </div>

    {#if error}
      <div class="error" data-split-error>{error}</div>
    {/if}

    {#if preview}
      <div class="summary" data-split-summary>
        <span>{t('tools.split.totalParts', { count: preview.totalParts })}</span>
        <span>·</span>
        <span>{formatBytes(preview.totalBytes)}</span>
        <span>·</span>
        <span>{t('tools.split.outDir')}: {preview.outDir}</span>
      </div>
      <ul class="parts" data-split-preview>
        {#each preview.parts as part (part.index)}
          <li data-split-part>
            <span class="name">{part.name}</span>
            <span class="meta">{t('tools.split.partLines', { count: part.lines })} · {formatBytes(part.bytes)}</span>
            {#if part.overwrites}<span class="warn">{t('tools.split.overwrite')}</span>{/if}
          </li>
        {/each}
        {#if preview.totalParts > preview.parts.length}
          <li class="more">{t('tools.split.moreParts', { shown: preview.parts.length })}</li>
        {/if}
      </ul>
    {/if}

    {#if result}
      <div class="result" data-split-result aria-label={t('tools.split.resultAria')}>
        <span>{t('tools.split.done', { count: result.files.length, bytes: formatBytes(result.bytes) })}</span>
        <button type="button" data-split-open-first onclick={openFirst}>
          {t('tools.split.openFirst')}
        </button>
      </div>
    {/if}

    <div class="actions">
      <button type="button" data-split-preview-btn disabled={busy} onclick={() => void doPreview()}>
        {busy ? t('tools.split.previewing') : t('tools.split.preview')}
      </button>
      <button
        type="button"
        class="primary"
        data-split-apply
        disabled={busy || preview === null}
        onclick={() => void doApply()}
      >
        {busy ? t('tools.split.applying') : t('tools.split.apply')}
      </button>
      <button type="button" data-split-close disabled={busy} onclick={onClose}>
        {t('tools.split.cancel')}
      </button>
    </div>
  </div>
</div>

<style>
  .mask {
    position: fixed;
    inset: var(--h-titlebar) 0 0 0;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgb(0 0 0 / 0.32);
  }

  .panel {
    width: 560px;
    max-width: calc(100vw - 48px);
    max-height: calc(100vh - var(--h-titlebar) - 48px);
    overflow: auto;
    padding: 16px 18px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  h2 {
    margin: 0;
    font-size: 14px;
    color: var(--ink);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12.5px;
    color: var(--ink-dim);
  }

  .field input[type='text'],
  .field input[type='number'] {
    height: 28px;
    padding: 0 8px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 5px;
    color: var(--ink);
    font: inherit;
  }

  .row {
    display: flex;
    gap: 8px;
  }

  .mode {
    display: flex;
    align-items: center;
    gap: 14px;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px 10px;
    font-size: 12.5px;
    color: var(--ink);
  }

  .mode legend {
    font-size: 11.5px;
    color: var(--ink-dim);
  }

  .mode label,
  .check {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .check {
    font-size: 12.5px;
    color: var(--ink);
  }

  .error {
    color: #c0392b;
    font-size: 12.5px;
  }

  .summary {
    display: flex;
    gap: 6px;
    font-size: 12px;
    color: var(--ink-dim);
  }

  .parts {
    margin: 0;
    padding: 0;
    list-style: none;
    max-height: 180px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  .parts li {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 4px 8px;
    font-size: 12px;
    border-bottom: 1px solid var(--line);
  }

  .parts li:last-child {
    border-bottom: none;
  }

  .parts .name {
    color: var(--ink);
    min-width: 180px;
  }

  .parts .meta {
    color: var(--ink-dim);
  }

  .parts .warn {
    color: #c0392b;
  }

  .parts .more {
    color: var(--ink-dim);
    justify-content: center;
  }

  .result {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12.5px;
    color: var(--ink);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }

  .actions button {
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: 5px;
    background: var(--bg);
    color: var(--ink);
    font: inherit;
    cursor: pointer;
  }

  .actions button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .actions button:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
