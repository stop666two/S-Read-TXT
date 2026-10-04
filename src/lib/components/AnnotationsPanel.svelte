<!--
  AnnotationsPanel — 标注面板（P2-3）：书签 / 高亮 / 注释（含待办）分区列表。
  交互：点击条目跳转（父组件经 jumpStore 定位）；逐条删除；待办可勾选完成；可清空本文件全部标注。
  契约：data-annotations-panel / [data-ann-section] / [data-ann-item] / [data-ann-del] / [data-ann-done] / [data-ann-close] / [data-ann-clear]
-->
<script lang="ts">
  import { t } from '../../lib/i18n/index.svelte';
  import { describeIpcError, toIpcError } from '../../lib/ipc';
  import { annotations } from '../../lib/state/annotations.svelte';
  import { toasts } from '../../lib/state/toasts.svelte';
  import ConfirmDialog from './ConfirmDialog.svelte';

  interface Props {
    /** 面板可见性 */
    open: boolean;
    /** 目标标签（0 表示无活动标签） */
    tabId: number;
    /** 关闭面板 */
    onClose: () => void;
    /** 跳转到指定显示行（0 基） */
    onJump: (row: number) => void;
  }
  let { open, tabId, onClose, onJump }: Props = $props();

  let confirmClear = $state(false);

  const data = $derived(tabId ? annotations.forTab(tabId) : null);

  /** 统一错误上报（删除/勾选/清空失败）。 */
  function report(error: unknown): void {
    toasts.error(describeIpcError(toIpcError(error)));
  }

  async function removeBookmark(id: number): Promise<void> {
    try {
      await annotations.removeBookmark(tabId, id);
    } catch (error) {
      report(error);
    }
  }

  async function removeHighlight(id: number): Promise<void> {
    try {
      await annotations.removeHighlight(tabId, id);
    } catch (error) {
      report(error);
    }
  }

  async function removeNote(id: number): Promise<void> {
    try {
      await annotations.removeNote(tabId, id);
    } catch (error) {
      report(error);
    }
  }

  async function toggleDone(id: number, text: string, done: boolean): Promise<void> {
    try {
      await annotations.updateNote(tabId, id, text, !done);
    } catch (error) {
      report(error);
    }
  }

  async function clearAll(): Promise<void> {
    confirmClear = false;
    try {
      await annotations.clear(tabId);
      toasts.show(t('annot.cleared'));
    } catch (error) {
      report(error);
    }
  }
</script>

{#if open}
  <div class="mask" role="presentation" onclick={(event) => event.target === event.currentTarget && onClose()}>
    <div class="panel" role="dialog" aria-modal="false" aria-label={t('annot.title')} data-annotations-panel>
      <header>
        <h2>{t('annot.title')}</h2>
        <button class="icon" title={t('common.close')} aria-label={t('common.close')} data-ann-close onclick={onClose}>×</button>
      </header>

      {#if !data || (data.bookmarks.length === 0 && data.highlights.length === 0 && data.notes.length === 0)}
        <p class="empty" data-ann-empty>{t('annot.empty')}</p>
      {:else}
        {#if data.bookmarks.length > 0}
          <section data-ann-section="bookmarks">
            <h3>{t('annot.bookmarks')} <small>{data.bookmarks.length}</small></h3>
            {#each data.bookmarks as item (item.id)}
              <div class="item" data-ann-item="bookmark" role="button" tabindex="0" onclick={() => onJump(item.row)} onkeydown={(event) => (event.key === 'Enter' || event.key === ' ') && onJump(item.row)}>
                <span class="excerpt">{item.label || item.excerpt || t('annot.row', { row: item.row + 1 })}</span>
                <span class="meta">{t('annot.row', { row: item.row + 1 })}</span>
                <button class="del" title={t('annot.delete')} aria-label={t('annot.delete')} data-ann-del onclick={(event) => { event.stopPropagation(); void removeBookmark(item.id); }}>✕</button>
              </div>
            {/each}
          </section>
        {/if}

        {#if data.highlights.length > 0}
          <section data-ann-section="highlights">
            <h3>{t('annot.highlights')} <small>{data.highlights.length}</small></h3>
            {#each data.highlights as item (item.id)}
              <div class="item" data-ann-item="highlight" role="button" tabindex="0" onclick={() => onJump(item.row)} onkeydown={(event) => (event.key === 'Enter' || event.key === ' ') && onJump(item.row)}>
                <span class="dot" style={item.color ? `background:${item.color}` : ''} aria-hidden="true"></span>
                <span class="excerpt">{item.excerpt || t('annot.row', { row: item.row + 1 })}</span>
                <span class="meta">{t('annot.row', { row: item.row + 1 })}</span>
                <button class="del" title={t('annot.delete')} aria-label={t('annot.delete')} data-ann-del onclick={(event) => { event.stopPropagation(); void removeHighlight(item.id); }}>✕</button>
              </div>
            {/each}
          </section>
        {/if}

        {#if data.notes.length > 0}
          <section data-ann-section="notes">
            <h3>{t('annot.notes')} <small>{data.notes.length}</small></h3>
            {#each data.notes as item (item.id)}
              <div class="item" data-ann-item="note" role="button" tabindex="0" onclick={() => onJump(item.row)} onkeydown={(event) => (event.key === 'Enter' || event.key === ' ') && onJump(item.row)}>
                {#if item.kind === 'todo'}
                  <input
                    type="checkbox"
                    class="done"
                    data-ann-done
                    checked={item.done}
                    title={item.done ? t('annot.undone') : t('annot.done')}
                    aria-label={item.done ? t('annot.undone') : t('annot.done')}
                    onclick={(event) => event.stopPropagation()}
                    onchange={() => void toggleDone(item.id, item.text, item.done)}
                  />
                {:else}
                  <span class="dot note" aria-hidden="true"></span>
                {/if}
                <span class="excerpt" class:done={item.done}>{item.text}</span>
                <span class="meta">{t('annot.row', { row: item.row + 1 })}</span>
                <button class="del" title={t('annot.delete')} aria-label={t('annot.delete')} data-ann-del onclick={(event) => { event.stopPropagation(); void removeNote(item.id); }}>✕</button>
              </div>
            {/each}
          </section>
        {/if}
      {/if}

      <footer>
        <button class="danger-mini" type="button" data-ann-clear disabled={!data} onclick={() => (confirmClear = true)}>
          {t('annot.clear')}
        </button>
      </footer>
    </div>
  </div>
{/if}

{#if confirmClear}
  <ConfirmDialog
    open
    title={t('annot.clearTitle')}
    message={t('annot.clearMessage')}
    confirmLabel={t('annot.clear')}
    onConfirm={() => void clearAll()}
    onCancel={() => (confirmClear = false)}
  />
{/if}

<style>
  .mask {
    position: fixed;
    inset: var(--h-titlebar) 0 0 0;
    z-index: 120;
    background: rgba(0, 0, 0, 0.25);
  }

  .panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    display: flex;
    width: 320px;
    flex-direction: column;
    border-left: 1px solid var(--line);
    background: var(--surface);
    color: var(--ink);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
  }

  h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }

  .icon {
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--muted);
    font-size: 15px;
    line-height: 1;
    cursor: default;
  }

  .icon:hover {
    background: var(--hover);
    color: var(--ink);
  }

  .empty {
    margin: 24px 12px;
    color: var(--muted);
    font-size: 12px;
    text-align: center;
  }

  section {
    padding: 6px 0;
  }

  h3 {
    margin: 6px 12px 2px;
    color: var(--muted);
    font-size: 11px;
    font-weight: 600;
  }

  h3 small {
    margin-left: 4px;
    font-weight: 400;
  }

  .item {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 5px 12px;
    cursor: default;
  }

  .item:hover {
    background: var(--hover);
  }

  .excerpt {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--ink);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .excerpt.done {
    color: var(--muted);
    text-decoration: line-through;
  }

  .meta {
    flex-shrink: 0;
    color: var(--muted);
    font-size: 11px;
  }

  .dot {
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--warning, #8a5200);
  }

  .dot.note {
    border-radius: 50%;
    background: var(--accent);
  }

  .done {
    flex-shrink: 0;
    margin: 0;
  }

  .del {
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--muted);
    font-size: 11px;
    line-height: 1;
    cursor: default;
    visibility: hidden;
  }

  .item:hover .del {
    visibility: visible;
  }

  .del:hover {
    background: var(--hover);
    color: var(--danger, #c0392b);
  }

  footer {
    margin-top: auto;
    padding: 10px 12px;
    border-top: 1px solid var(--line);
  }

  .danger-mini {
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: transparent;
    color: var(--danger, #c0392b);
    font: inherit;
    font-size: 12px;
    cursor: default;
  }

  .danger-mini:disabled {
    opacity: 0.5;
  }

  .danger-mini:hover:not(:disabled) {
    background: var(--hover);
  }
</style>
