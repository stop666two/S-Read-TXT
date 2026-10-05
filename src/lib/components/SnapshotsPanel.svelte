<script lang="ts">
  /**
   * 版本历史面板：列出当前文件快照，支持恢复/删除/立即快照。
   *
   * - 恢复由上层（App → EditLayer）执行（需编辑态，结果走单撤销步）；
   * - 删除与列表刷新在面板内完成（`refreshKey` 由上层在恢复后递增触发）。
   */
  import ConfirmDialog from './ConfirmDialog.svelte';
  import { formatBytes } from '../format';
  import { t } from '../i18n/index.svelte';
  import { i18n } from '../i18n/index.svelte';
  import { ipc, toIpcError, type SnapshotInfo } from '../ipc';
  import { toasts } from '../state/toasts.svelte';

  interface Props {
    open: boolean;
    tabId: number | null;
    /** 上层触发刷新的键（恢复成功后递增） */
    refreshKey: number;
    /** 当前标签是否处于编辑态（恢复入口可用性） */
    canRestore: boolean;
    onRestore: (name: string) => void;
    onClose: () => void;
  }

  let { open, tabId, refreshKey, canRestore, onRestore, onClose }: Props = $props();

  let items = $state<SnapshotInfo[]>([]);
  let busy = $state(false);
  let loadSeq = 0;
  let confirmDelete = $state<SnapshotInfo | null>(null);

  $effect(() => {
    const id = tabId;
    void refreshKey;
    if (!open || id === null) return;
    const seq = ++loadSeq;
    busy = true;
    void ipc
      .listSnapshots(id)
      .then((next) => {
        if (seq === loadSeq) items = next;
      })
      .catch(() => {
        if (seq === loadSeq) items = [];
      })
      .finally(() => {
        if (seq === loadSeq) busy = false;
      });
  });

  function formatTime(millis: number): string {
    try {
      return new Intl.DateTimeFormat(i18n.locale, {
        dateStyle: 'short',
        timeStyle: 'medium',
      }).format(new Date(millis));
    } catch {
      return String(millis);
    }
  }

  async function reload(): Promise<void> {
    if (tabId === null) return;
    const seq = ++loadSeq;
    const next = await ipc.listSnapshots(tabId);
    if (seq === loadSeq) items = next;
  }

  async function snapshotNow(): Promise<void> {
    if (tabId === null || busy) return;
    busy = true;
    try {
      const created = await ipc.createSnapshot(tabId);
      toasts.show(t(created ? 'snapshot.created' : 'snapshot.unchanged'));
      await reload();
    } catch (error) {
      toasts.error(toIpcError(error).message);
    } finally {
      busy = false;
    }
  }

  async function confirmDeleteNow(): Promise<void> {
    const target = confirmDelete;
    confirmDelete = null;
    if (!target || tabId === null) return;
    try {
      await ipc.deleteSnapshot(tabId, target.name);
      toasts.show(t('snapshot.deleted'));
      await reload();
    } catch (error) {
      toasts.error(toIpcError(error).message);
    }
  }
</script>

{#if open}
  <div class="mask" onclick={onClose} role="presentation"></div>
  <aside class="panel" data-snapshots-panel aria-label={t('snapshot.title')}>
    <header>
      <h2>{t('snapshot.title')}</h2>
      <button class="close" data-snap-close aria-label={t('common.close')} onclick={onClose}>×</button>
    </header>
    <div class="actions">
      <button class="now" data-snap-now disabled={busy} onclick={() => void snapshotNow()}>{t('snapshot.now')}</button>
    </div>
    <div class="list">
      {#if items.length === 0}
        <p class="empty" data-snap-empty>{busy ? '…' : t('snapshot.empty')}</p>
      {:else}
        {#each items as item (item.name)}
          <div class="item" data-snap-item data-snap-name={item.name}>
            <span class="meta">{formatTime(item.createdMillis)} · {formatBytes(item.bytes)}</span>
            <span class="btns">
              <button
                class="btn"
                data-snap-restore
                disabled={!canRestore}
                title={canRestore ? undefined : t('snapshot.needEdit')}
                onclick={() => onRestore(item.name)}
              >
                {t('snapshot.restore')}
              </button>
              <button class="btn danger" data-snap-del onclick={() => (confirmDelete = item)}>{t('snapshot.delete')}</button>
            </span>
          </div>
        {/each}
      {/if}
    </div>
  </aside>
  <ConfirmDialog
    open={confirmDelete !== null}
    title={t('snapshot.deleteTitle')}
    message={t('snapshot.deleteMessage')}
    confirmLabel={t('snapshot.delete')}
    onConfirm={() => void confirmDeleteNow()}
    onCancel={() => (confirmDelete = null)}
  />
{/if}

<style>
  .mask {
    position: fixed;
    inset: var(--h-titlebar) 0 0 0;
    background: rgba(0, 0, 0, 0.18);
    z-index: 40;
  }
  .panel {
    position: fixed;
    top: var(--h-titlebar);
    right: 0;
    bottom: 0;
    width: 340px;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-left: 1px solid var(--line);
    z-index: 41;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
  }
  h2 {
    font-size: 14px;
    margin: 0;
  }
  .close {
    border: none;
    background: none;
    color: var(--muted);
    font-size: 16px;
    cursor: pointer;
  }
  .close:hover {
    color: var(--ink);
  }
  .actions {
    padding: 8px 12px;
    border-bottom: 1px solid var(--line);
  }
  .now {
    font: inherit;
    font-size: 13px;
    padding: 4px 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--hover);
    color: var(--ink);
    cursor: pointer;
  }
  .now:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .list {
    flex: 1;
    overflow: auto;
  }
  .item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 12px;
    font-size: 13px;
  }
  .item:hover {
    background: var(--hover);
  }
  .meta {
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .btns {
    display: flex;
    gap: 6px;
    flex: none;
  }
  .btn {
    font: inherit;
    font-size: 12px;
    padding: 2px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: none;
    color: var(--ink);
    cursor: pointer;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .btn.danger:hover {
    border-color: #c0392b;
    color: #c0392b;
  }
  .empty {
    color: var(--muted);
    font-size: 13px;
    padding: 12px;
  }
</style>
