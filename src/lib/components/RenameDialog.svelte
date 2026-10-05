<script lang="ts">
  // RenameDialog —— 「工具 → 批量重命名…」对话框：
  // 扫描目录 → 组合规则（查找替换 / 前后缀 / 序号）→ 预览（冲突/重名/非法标红）→ 执行；
  // 执行成功后提供「撤销本次重命名」（后端撤销日志）。
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { untrack } from 'svelte';
  import { t } from '../i18n/index.svelte';
  import {
    ipc,
    toIpcError,
    type RenameEntry,
    type RenameLog,
    type RenameNumberAt,
    type RenamePair,
    type RenameRules,
    type RenameStatus,
  } from '../ipc';
  import { toasts } from '../state/toasts.svelte';

  interface Props {
    /** 默认目录（激活标签父目录；无则 null） */
    initialDir: string | null;
    onClose: () => void;
  }

  let { initialDir, onClose }: Props = $props();

  let dir = $state(untrack(() => initialDir ?? ''));
  let extensions = $state('txt,log');
  let find = $state('');
  let replace = $state('');
  let prefix = $state('');
  let suffix = $state('');
  let numberOn = $state(false);
  let start = $state(1);
  let step = $state(1);
  let digits = $state(2);
  let at = $state<RenameNumberAt>('suffix');
  let files = $state<string[]>([]);
  let entries = $state<RenameEntry[]>([]);
  let log = $state<RenameLog | null>(null);
  let busy = $state(false);
  let error = $state('');

  const blocking = $derived(
    entries.some(
      (entry) =>
        entry.status === 'conflict' || entry.status === 'exists' || entry.status === 'invalid',
    ),
  );
  const okPairs = $derived(
    entries
      .filter((entry) => entry.status === 'ok')
      .map((entry): RenamePair => ({ old: entry.old, new: entry.new })),
  );
  const counts = $derived.by(() => {
    const result: Record<RenameStatus, number> = {
      ok: 0,
      unchanged: 0,
      conflict: 0,
      exists: 0,
      invalid: 0,
    };
    for (const entry of entries) result[entry.status] += 1;
    return result;
  });

  const buildRules = (): RenameRules => ({
    find: find.trim() === '' ? null : find,
    replace,
    prefix: prefix.trim() === '' ? null : prefix,
    suffix: suffix.trim() === '' ? null : suffix,
    numbering: numberOn
      ? {
          start: Math.max(0, Math.floor(start) || 0),
          step: Math.max(1, Math.floor(step) || 1),
          digits: Math.min(6, Math.max(2, Math.floor(digits) || 2)),
          at,
        }
      : null,
  });

  const parseExtensions = (): string[] =>
    extensions
      .split(',')
      .map((item) => item.trim().replace(/^\./, ''))
      .filter((item) => item.length > 0);

  async function loadLog(): Promise<void> {
    try {
      log = await ipc.readRenameLog();
    } catch {
      log = null;
    }
  }
  void loadLog();

  async function pickDir(): Promise<void> {
    const picked = await openDialog({ directory: true, multiple: false });
    if (typeof picked === 'string') dir = picked;
  }

  async function doScan(): Promise<void> {
    if (busy) return;
    const target = dir.trim();
    if (!target) {
      error = t('rename.dirRequired');
      return;
    }
    busy = true;
    error = '';
    entries = [];
    try {
      const items = await ipc.scanRenameDir(target, parseExtensions());
      files = items.map((item) => item.name);
    } catch (caught) {
      files = [];
      error = toIpcError(caught).message;
    } finally {
      busy = false;
    }
  }

  async function doPreview(): Promise<void> {
    if (busy) return;
    if (files.length === 0) {
      error = t('rename.needScan');
      return;
    }
    busy = true;
    error = '';
    try {
      entries = await ipc.previewRename(dir.trim(), files, buildRules());
    } catch (caught) {
      entries = [];
      error = toIpcError(caught).message;
    } finally {
      busy = false;
    }
  }

  async function doApply(): Promise<void> {
    if (busy) return;
    if (okPairs.length === 0) {
      error = t('rename.needPreview');
      return;
    }
    busy = true;
    error = '';
    try {
      const applied = await ipc.applyRename(dir.trim(), okPairs);
      toasts.show(t('rename.done', { count: applied.pairs.length }));
      if (!applied.logSaved) toasts.show(t('rename.logNotSaved'), 'warn');
      entries = [];
      await loadLog();
      await doScan();
    } catch (caught) {
      error = toIpcError(caught).message;
    } finally {
      busy = false;
    }
  }

  async function doUndo(): Promise<void> {
    if (busy || !log) return;
    busy = true;
    error = '';
    try {
      await ipc.undoRename(log.dir, log.pairs);
      toasts.show(t('rename.undone'));
      const undoneDir = log.dir;
      log = null;
      if (dir.trim() === undoneDir) await doScan();
    } catch (caught) {
      error = toIpcError(caught).message;
    } finally {
      busy = false;
    }
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
  tabindex="-1"
  aria-modal="true"
  aria-label={t('rename.title')}
  data-rename-dialog
  onmousedown={(event) => {
    if (event.target === event.currentTarget && !busy) onClose();
  }}
>
  <div class="panel">
    <h2>{t('rename.title')}</h2>

    <label class="field">
      <span>{t('rename.dir')}</span>
      <input
        type="text"
        data-rename-dir
        placeholder={t('rename.dirPlaceholder')}
        bind:value={dir}
        disabled={busy}
      />
    </label>
    <div class="row">
      <button type="button" data-rename-dir-pick disabled={busy} onclick={() => void pickDir()}>
        {t('rename.pickDir')}
      </button>
    </div>

    <label class="field">
      <span>{t('rename.ext')}</span>
      <input
        type="text"
        data-rename-ext
        placeholder={t('rename.extPlaceholder')}
        bind:value={extensions}
        disabled={busy}
      />
    </label>

    <fieldset class="mode">
      <legend>{t('rename.rules')}</legend>
      <label class="field inline">
        <span>{t('rename.find')}</span>
        <input
          type="text"
          data-rename-find
          placeholder={t('rename.findPlaceholder')}
          bind:value={find}
          disabled={busy}
        />
      </label>
      <label class="field inline">
        <span>{t('rename.replace')}</span>
        <input
          type="text"
          data-rename-replace
          placeholder={t('rename.replacePlaceholder')}
          bind:value={replace}
          disabled={busy}
        />
      </label>
      <label class="field inline">
        <span>{t('rename.prefix')}</span>
        <input type="text" data-rename-prefix bind:value={prefix} disabled={busy} />
      </label>
      <label class="field inline">
        <span>{t('rename.suffix')}</span>
        <input type="text" data-rename-suffix bind:value={suffix} disabled={busy} />
      </label>
    </fieldset>

    <fieldset class="mode">
      <legend>
        <label class="check">
          <input
            type="checkbox"
            data-rename-numbering
            bind:checked={numberOn}
            disabled={busy}
          />
          <span>{t('rename.numbering')}</span>
        </label>
      </legend>
      <label class="field inline">
        <span>{t('rename.start')}</span>
        <input
          type="number"
          min="0"
          data-rename-start
          bind:value={start}
          disabled={busy || !numberOn}
        />
      </label>
      <label class="field inline">
        <span>{t('rename.step')}</span>
        <input
          type="number"
          min="1"
          data-rename-step
          bind:value={step}
          disabled={busy || !numberOn}
        />
      </label>
      <label class="field inline">
        <span>{t('rename.digits')}</span>
        <input
          type="number"
          min="2"
          max="6"
          data-rename-digits
          bind:value={digits}
          disabled={busy || !numberOn}
        />
      </label>
      <span class="radios">
        <label class="check">
          <input
            type="radio"
            value="prefix"
            bind:group={at}
            data-rename-at-prefix
            disabled={busy || !numberOn}
          />
          <span>{t('rename.atPrefix')}</span>
        </label>
        <label class="check">
          <input
            type="radio"
            value="suffix"
            bind:group={at}
            data-rename-at-suffix
            disabled={busy || !numberOn}
          />
          <span>{t('rename.atSuffix')}</span>
        </label>
      </span>
    </fieldset>

    <div class="row">
      <button type="button" data-rename-scan disabled={busy} onclick={() => void doScan()}>
        {busy ? t('rename.scanning') : t('rename.scan')}
      </button>
      <button
        type="button"
        data-rename-preview
        disabled={busy || files.length === 0}
        onclick={() => void doPreview()}
      >
        {t('rename.preview')}
      </button>
      <span class="count">{t('rename.filesCount', { count: files.length })}</span>
    </div>

    {#if error}
      <div class="error" data-rename-error>{error}</div>
    {/if}

    {#if log}
      <div class="undo-bar" data-rename-undo-info>
        <span>{t('rename.undoInfo', { dir: log.dir, count: log.pairs.length })}</span>
        <button type="button" data-rename-undo disabled={busy} onclick={() => void doUndo()}>
          {t('rename.undo')}
        </button>
      </div>
    {/if}

    {#if entries.length > 0}
      <div class="summary" data-rename-stats>
        <span>{t('rename.statsOk', { count: counts.ok })}</span>
        <span>·</span>
        <span>{t('rename.statsUnchanged', { count: counts.unchanged })}</span>
        {#if counts.conflict > 0}<span>·</span><span class="bad">{t('rename.status.conflict')} {counts.conflict}</span>{/if}
        {#if counts.exists > 0}<span>·</span><span class="bad">{t('rename.status.exists')} {counts.exists}</span>{/if}
        {#if counts.invalid > 0}<span>·</span><span class="bad">{t('rename.status.invalid')} {counts.invalid}</span>{/if}
      </div>
      <ul class="rename-list" data-rename-preview-list>
        {#each entries as entry (entry.old)}
          <li data-rename-row data-status={entry.status} class={entry.status}>
            <span class="old">{entry.old}</span>
            <span class="arrow">→</span>
            <span class="new">{entry.new}</span>
            <span class="status">{t(`rename.status.${entry.status}`)}</span>
          </li>
        {/each}
      </ul>
      {#if blocking}
        <div class="error" data-rename-blocked>{t('rename.blocked')}</div>
      {/if}
    {/if}

    <div class="actions">
      <button
        type="button"
        class="primary"
        data-rename-apply
        disabled={busy || okPairs.length === 0 || blocking}
        onclick={() => void doApply()}
      >
        {busy ? t('rename.applying') : t('rename.apply')}
      </button>
      <button type="button" data-rename-close disabled={busy} onclick={onClose}>
        {t('rename.cancel')}
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
    width: 620px;
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

  .field.inline {
    flex-direction: row;
    align-items: center;
    gap: 8px;
  }

  .field.inline input[type='text'],
  .field.inline input[type='number'] {
    width: 120px;
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
    align-items: center;
    gap: 8px;
  }

  .row button,
  .undo-bar button {
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: 5px;
    background: var(--bg);
    color: var(--ink);
    font: inherit;
    cursor: pointer;
  }

  .count {
    font-size: 12px;
    color: var(--ink-dim);
  }

  .mode {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px 10px;
    font-size: 12.5px;
    color: var(--ink);
    margin: 0;
  }

  .mode legend {
    font-size: 11.5px;
    color: var(--ink-dim);
    padding: 0 4px;
  }

  .radios {
    display: flex;
    gap: 10px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 5px;
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

  .summary .bad {
    color: #c0392b;
  }

  .undo-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
    font-size: 12px;
    color: var(--ink-dim);
  }

  .rename-list {
    margin: 0;
    padding: 0;
    list-style: none;
    max-height: 200px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  .rename-list li {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 4px 8px;
    font-size: 12px;
    border-bottom: 1px solid var(--line);
  }

  .rename-list li:last-child {
    border-bottom: none;
  }

  .rename-list .old {
    color: var(--ink-dim);
    min-width: 180px;
  }

  .rename-list .arrow {
    color: var(--ink-dim);
  }

  .rename-list .new {
    color: var(--ink);
    min-width: 180px;
  }

  .rename-list .status {
    margin-left: auto;
    color: var(--ink-dim);
  }

  .rename-list li.conflict .new,
  .rename-list li.conflict .status,
  .rename-list li.exists .new,
  .rename-list li.exists .status,
  .rename-list li.invalid .new,
  .rename-list li.invalid .status {
    color: #c0392b;
  }

  .rename-list li.unchanged .new,
  .rename-list li.unchanged .status {
    opacity: 0.6;
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

  .actions button:disabled,
  .row button:disabled,
  .undo-bar button:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
