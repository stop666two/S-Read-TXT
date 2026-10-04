<script lang="ts">
  /**
   * 大纲面板（P2-6c V-09）：列出当前文档章节，点击跳转。
   *
   * 数据来自 `ipc.outlineItems`（后端解析 `display.outlinePatterns` 正则；
   * 空列表回退内置默认）。打开期间 tab 变化或 `refreshKey` 变化时重新拉取。
   */
  import { ipc, toIpcError } from '../ipc';
  import { t } from '../i18n/index.svelte';
  import type { OutlineItem } from '../ipc';

  interface Props {
    /** 目标标签（null 时无内容） */
    tabId: number | null;
    /** 面板可见性 */
    open: boolean;
    /** 内容刷新键（标签 / 行数变化时重新拉取） */
    refreshKey: string;
    /** 点击条目跳转（文件显示行号） */
    onJump: (row: number) => void;
    /** 关闭面板 */
    onClose: () => void;
  }

  let { tabId, open, refreshKey, onJump, onClose }: Props = $props();

  let items = $state<OutlineItem[]>([]);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let loadSeq = 0;

  $effect(() => {
    const id = tabId;
    void refreshKey;
    if (!open || id === null) return;
    const seq = ++loadSeq;
    busy = true;
    error = null;
    void ipc
      .outlineItems(id)
      .then((next) => {
        if (seq === loadSeq) items = next;
      })
      .catch((err) => {
        if (seq === loadSeq) {
          items = [];
          error = toIpcError(err).message;
        }
      })
      .finally(() => {
        if (seq === loadSeq) busy = false;
      });
  });
</script>

{#if open}
  <div class="mask" onclick={onClose} role="presentation"></div>
  <aside class="panel" data-outline-panel aria-label={t('outline.title')}>
    <header>
      <h2>{t('outline.title')}</h2>
      <button class="close" data-outline-close aria-label={t('common.close')} onclick={onClose}>×</button>
    </header>
    <div class="list">
      {#if items.length === 0}
        <p class="empty" data-outline-empty>{busy ? '…' : (error ?? t('outline.empty'))}</p>
      {:else}
        {#each items as item (item.row)}
          <button
            class="item"
            data-outline-item
            data-outline-row={item.row}
            style="padding-left: {12 + item.level * 14}px"
            title={item.title}
            onclick={() => onJump(item.row)}
          >
            {item.title}
          </button>
        {/each}
      {/if}
    </div>
  </aside>
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
    width: 320px;
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
  .list {
    flex: 1;
    overflow: auto;
    padding: 6px 0;
  }
  .item {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: none;
    color: var(--ink);
    font: inherit;
    font-size: 13px;
    padding: 6px 12px;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item:hover {
    background: var(--hover);
  }
  .empty {
    color: var(--muted);
    font-size: 13px;
    padding: 12px;
  }
</style>
