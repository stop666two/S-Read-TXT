<script lang="ts">
  // 工作区（多文件）查找与替换弹窗（P1-8b；规格 F-12/F-13）。
  // 数据来源：`search_workspace`（全部已打开标签；编辑态标签跨行语义与查找条
  // 一致，只读标签逐行匹配）；命中点击经 onJump 回调交给 App（切换标签 +
  // jumpStore 广播定位）。
  // 挂载即打开（App 按需创建，关闭即销毁），无 open prop。
  import { describeIpcError, ipc, toIpcError, type SearchMode, type WorkspaceFileResult, type WorkspaceHit, type WorkspaceReplaceResponse, type WorkspaceSearchResponse } from '../../lib/ipc';
  import { t } from '../../lib/i18n/index.svelte';
  import ConfirmDialog from './ConfirmDialog.svelte';

  interface Props {
    /** 关闭弹窗 */
    onClose: () => void;
    /** 命中跳转（App：切换活动标签 + 广播定位） */
    onJump: (file: WorkspaceFileResult, hit: WorkspaceHit) => void;
  }
  let { onClose, onJump }: Props = $props();

  let query = $state('');
  let replacement = $state('');
  let caseSensitive = $state(false);
  let wholeWord = $state(false);
  let mode = $state<SearchMode>('literal');
  let results = $state<WorkspaceSearchResponse | null>(null);
  let replaced = $state<WorkspaceReplaceResponse | null>(null);
  let error = $state('');
  let busy = $state(false);
  let confirmOpen = $state(false);
  let queryInput = $state<HTMLInputElement | null>(null);

  const total = $derived(results?.totalMatches ?? 0);
  const fileCount = $derived(
    results?.files.filter((file) => file.total > 0 || file.error).length ?? 0,
  );

  /** 执行搜索（覆盖上次结果；替换后自动调用刷新）。 */
  async function runSearch(): Promise<void> {
    if (query.trim() === '' || busy) return;
    busy = true;
    error = '';
    try {
      results = await ipc.searchWorkspace(query, caseSensitive, mode, wholeWord);
    } catch (cause) {
      error = describeIpcError(toIpcError(cause));
      results = null;
    } finally {
      busy = false;
    }
  }

  /** 全部替换（确认后执行；完成后自动重扫以刷新结果）。 */
  async function runReplaceAll(): Promise<void> {
    confirmOpen = false;
    if (query.trim() === '' || busy) return;
    busy = true;
    error = '';
    try {
      replaced = await ipc.replaceWorkspace(query, caseSensitive, mode, wholeWord, replacement);
      // 替换完成后重扫刷新结果（先释放 busy，否则 runSearch 会被自身守卫拦截）。
      busy = false;
      await runSearch();
    } catch (cause) {
      error = describeIpcError(toIpcError(cause));
    } finally {
      busy = false;
    }
  }

  /** 快捷键：Esc 关闭；Ctrl/⌘+Enter 搜索。 */
  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.stopPropagation();
      onClose();
      return;
    }
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void runSearch();
    }
  }

  $effect(() => {
    queryInput?.focus();
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="ws-mask" role="dialog" aria-modal="true" aria-label={t('ws.title')}>
  <div class="ws-panel">
    <header class="ws-head">
      <span class="ws-title">{t('ws.title')}</span>
      <button class="icon-btn" data-setting="ws.close" title={t('ws.close')} onclick={onClose}>
        ×
      </button>
    </header>

    <div class="ws-form">
      <input
        class="ws-input"
        bind:this={queryInput}
        bind:value={query}
        data-setting="ws.query"
        placeholder={t('ws.query')}
        spellcheck="false"
      />
      <input
        class="ws-input"
        bind:value={replacement}
        data-setting="ws.replace"
        placeholder={t('ws.replace')}
        spellcheck="false"
      />
      <div class="ws-toggles">
        <label class="ws-toggle">
          <input
            type="checkbox"
            data-setting="ws.regex"
            checked={mode === 'regex'}
            onchange={(event) => (mode = event.currentTarget.checked ? 'regex' : 'literal')}
          />{t('ws.regex')}
        </label>
        <label class="ws-toggle">
          <input type="checkbox" data-setting="ws.case" bind:checked={caseSensitive} />{t('ws.caseSensitive')}
        </label>
        <label class="ws-toggle">
          <input type="checkbox" data-setting="ws.wholeWord" bind:checked={wholeWord} />{t('ws.wholeWord')}
        </label>
      </div>
      <div class="ws-actions">
        <button class="btn primary" data-setting="ws.search" disabled={busy} onclick={() => void runSearch()}>
          {busy ? t('ws.searching') : t('ws.search')}
        </button>
        <button
          class="btn"
          data-setting="ws.replaceAll"
          disabled={busy || total === 0}
          onclick={() => (confirmOpen = true)}
        >
          {t('ws.replaceAll')}
        </button>
      </div>
    </div>

    {#if error !== ''}
      <p class="ws-error" data-ws-error>{error}</p>
    {/if}
    {#if replaced !== null}
      <p class="ws-replaced" data-ws-replaced>
        {t('ws.replaceDone', { replaced: replaced.totalReplaced, skipped: replaced.skipped })}
      </p>
    {/if}
    {#if results !== null}
      <p class="ws-summary" data-ws-summary>
        {t('ws.summary', { files: fileCount, matches: total })}
      </p>
    {/if}

    <div class="ws-results">
      {#each results?.files ?? [] as file (file.tabId)}
        {#if file.total > 0 || file.error}
          <section class="ws-file">
            <header class="ws-file-head" data-ws-file>
              <span class="ws-fname">{file.name}</span>
              <span class="ws-badge">{file.editing ? t('ws.editingBadge') : t('ws.readonlyBadge')}</span>
              <span class="ws-count">{t('ws.fileCount', { count: file.total })}</span>
              {#if file.truncated}
                <span class="ws-note">{t('ws.truncated', { count: file.matches.length })}</span>
              {/if}
              {#if file.timedOut}
                <span class="ws-note warn">{t('ws.timedOut')}</span>
              {/if}
              {#if file.error}
                <span class="ws-note err">{t('ws.fileError', { message: file.error })}</span>
              {/if}
            </header>
            {#if file.matches.length > 0}
              <ul class="ws-hits">
                {#each file.matches as hit (hit.row + ':' + hit.startUtf16)}
                  <li>
                    <button class="ws-hit" data-ws-hit onclick={() => onJump(file, hit)}>
                      <span class="ws-rowno">{t('ws.hitRow', { row: hit.row + 1 })}</span>
                      <span class="ws-preview">{hit.preview}</span>
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          </section>
        {/if}
      {/each}
      {#if results !== null && total === 0 && error === ''}
        <p class="ws-empty">{t('ws.noResults')}</p>
      {/if}
    </div>
  </div>
</div>

<ConfirmDialog
  open={confirmOpen}
  title={t('ws.replaceTitle')}
  message={t('ws.replaceMessage', { count: total })}
  confirmLabel={t('ws.replaceAll')}
  onConfirm={() => void runReplaceAll()}
  onCancel={() => (confirmOpen = false)}
/>

<style>
  .ws-mask {
    position: fixed;
    inset: var(--h-titlebar) 0 0 0;
    background: rgb(0 0 0 / 35%);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 48px;
    z-index: 60;
  }
  .ws-panel {
    width: min(720px, calc(100vw - 96px));
    max-height: calc(100vh - var(--h-titlebar) - 96px);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 12px 32px rgb(0 0 0 / 18%);
    overflow: hidden;
  }
  .ws-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
  }
  .ws-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--ink);
  }
  .icon-btn {
    border: none;
    background: transparent;
    color: var(--muted);
    font-size: 16px;
    line-height: 1;
    padding: 4px 8px;
    border-radius: 6px;
    cursor: pointer;
  }
  .icon-btn:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .ws-form {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--line);
  }
  .ws-input {
    background: var(--base);
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--ink);
    font-size: 13px;
    padding: 6px 10px;
    min-width: 0;
  }
  .ws-input:focus {
    outline: none;
    border-color: var(--accent);
  }
  .ws-toggles {
    grid-column: 1 / -1;
    display: flex;
    gap: 16px;
  }
  .ws-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
  }
  .ws-actions {
    grid-column: 1 / -1;
    display: flex;
    gap: 8px;
  }
  .btn {
    background: var(--hover);
    border: 1px solid var(--line);
    color: var(--ink);
    font-size: 13px;
    padding: 6px 14px;
    border-radius: 6px;
    cursor: pointer;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--surface);
  }
  .ws-error {
    margin: 8px 14px 0;
    color: #c0392b;
    font-size: 12px;
  }
  .ws-replaced {
    margin: 8px 14px 0;
    color: var(--accent);
    font-size: 12px;
  }
  .ws-summary {
    margin: 8px 14px 0;
    color: var(--muted);
    font-size: 12px;
  }
  .ws-results {
    overflow-y: auto;
    padding: 8px 14px 14px;
  }
  .ws-file {
    margin-bottom: 10px;
  }
  .ws-file-head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
    padding: 6px 0;
  }
  .ws-fname {
    font-size: 13px;
    font-weight: 600;
    color: var(--ink);
  }
  .ws-badge {
    font-size: 11px;
    color: var(--muted);
    border: 1px solid var(--line);
    border-radius: 4px;
    padding: 0 4px;
  }
  .ws-count {
    font-size: 12px;
    color: var(--accent);
  }
  .ws-note {
    font-size: 11px;
    color: var(--muted);
  }
  .ws-note.warn {
    color: #b7791f;
  }
  .ws-note.err {
    color: #c0392b;
  }
  .ws-hits {
    list-style: none;
    margin: 0;
    padding: 0 0 0 10px;
    border-left: 2px solid var(--line);
  }
  .ws-hit {
    display: flex;
    gap: 10px;
    width: 100%;
    text-align: left;
    background: transparent;
    border: none;
    border-radius: 4px;
    padding: 3px 6px;
    cursor: pointer;
    font-size: 12px;
  }
  .ws-hit:hover {
    background: var(--hover);
  }
  .ws-rowno {
    color: var(--muted);
    flex: none;
    font-variant-numeric: tabular-nums;
  }
  .ws-preview {
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ws-empty {
    color: var(--muted);
    font-size: 13px;
    padding: 12px 0;
  }
</style>
