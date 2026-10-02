<!--
  HistoryPanel —— 历史记录侧滑面板（阶段 6）。
  能力：全量历史（虚拟列表，支持万级条目）、搜索过滤、点击重开并恢复进度、单条删除、
  清空（二次确认）。数据源为共享 store（historyStore）。
-->
<script lang="ts">
  import { formatBytes } from '../format';
  import type { HistoryEntry } from '../ipc';
  import { historyStore } from '../state/history.svelte';
  import ConfirmDialog from './ConfirmDialog.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    /** 是否显示（App 控制） */
    open: boolean;
    /** 请求关闭面板 */
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  /** 虚拟列表：固定行高 + 视口外预渲染 */
  const ITEM_HEIGHT = 58;
  const OVERSCAN = 6;

  /** 搜索关键字（文件名/路径，大小写不敏感） */
  let query = $state('');
  /** 滚动容器 */
  let scroller = $state<HTMLDivElement | null>(null);
  /** 滚动位置（虚拟窗口依据） */
  let scrollTop = $state(0);
  /** 容器视口高度 */
  let viewportHeight = $state(0);
  /** 清空二次确认 */
  let confirmClear = $state(false);

  /** 过滤后的条目（保持后端顺序：最近在前） */
  const filtered = $derived.by(() => {
    const keyword = query.trim().toLowerCase();
    if (!keyword) return historyStore.entries;
    return historyStore.entries.filter(
      (entry) =>
        entry.name.toLowerCase().includes(keyword) || entry.path.toLowerCase().includes(keyword),
    );
  });

  /** 首个渲染条目下标（含预渲染） */
  const firstIndex = $derived(Math.max(0, Math.floor(scrollTop / ITEM_HEIGHT) - OVERSCAN));
  /** 渲染条目数 */
  const visibleCount = $derived(Math.ceil(viewportHeight / ITEM_HEIGHT) + OVERSCAN * 2);
  /** 当前渲染窗口 */
  const visible = $derived(filtered.slice(firstIndex, firstIndex + visibleCount));

  /** 打开时刷新数据并重置视图 */
  $effect(() => {
    if (!open) return;
    void historyStore.load();
    query = '';
    scrollTop = 0;
    requestAnimationFrame(() => {
      if (scroller) {
        scroller.scrollTop = 0;
        viewportHeight = scroller.clientHeight;
      }
    });
  });

  /** 视口尺寸跟随（窗口缩放/面板尺寸变化） */
  $effect(() => {
    const el = scroller;
    if (!el) return;
    viewportHeight = el.clientHeight;
    const observer = new ResizeObserver(() => {
      viewportHeight = el.clientHeight;
    });
    observer.observe(el);
    return () => observer.disconnect();
  });

  /** 时间展示（本地时区，失败回退原文）。 */
  function formatTime(iso: string): string {
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return iso;
    return date.toLocaleString('zh-CN', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      hour12: false,
    });
  }

  /** 打开条目：重开文件（恢复进度）后收起面板。 */
  function openEntry(entry: HistoryEntry): void {
    void historyStore.openEntry(entry);
    onClose();
  }
</script>

{#if open}
  <div
    class="mask"
    role="presentation"
    onmousedown={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div class="panel" role="dialog" aria-modal="false" aria-label="历史记录">
      <header class="head">
        <h2>历史记录</h2>
        <input
          class="search"
          type="search"
          placeholder="搜索文件名或路径"
          aria-label="搜索历史记录"
          bind:value={query}
        />
        <button class="icon" aria-label="关闭历史面板" onclick={onClose}>
          <Icon name="close" size={14} />
        </button>
      </header>

      <div
        class="list"
        bind:this={scroller}
        onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
      >
        {#if filtered.length === 0}
          <p class="empty">
            {historyStore.entries.length === 0 ? '暂无历史记录' : '没有匹配的记录'}
          </p>
        {:else}
          <div class="phantom" style="height: {filtered.length * ITEM_HEIGHT}px;">
            {#each visible as entry, index (entry.path)}
              <div
                class="entry"
                style="top: {(firstIndex + index) * ITEM_HEIGHT}px; height: {ITEM_HEIGHT}px;"
                role="button"
                tabindex="0"
                title={entry.path}
                onclick={() => openEntry(entry)}
                onkeydown={(event) => {
                  if (event.key === 'Enter' || event.key === ' ') {
                    event.preventDefault();
                    openEntry(entry);
                  }
                }}
              >
                <div class="row1">
                  <span class="name">{entry.name}</span>
                  <button
                    class="del"
                    aria-label={`删除历史：${entry.name}`}
                    title="删除这条记录"
                    onclick={(event) => {
                      event.stopPropagation();
                      void historyStore.remove(entry.path);
                    }}
                  >
                    <Icon name="close" size={12} />
                  </button>
                </div>
                <div class="row2">{entry.path}</div>
                <div class="row3">
                  {formatBytes(entry.size)} · {formatTime(entry.openedAt)} · {entry.lastPercent > 0
                    ? `读到 ${Math.round(entry.lastPercent)}%`
                    : '未记录进度'}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <footer class="foot">
        <span class="count">共 {filtered.length} 条（显示 {historyStore.entries.length} 条中）</span>
        <button
          class="clear"
          disabled={historyStore.entries.length === 0}
          onclick={() => (confirmClear = true)}
        >
          清空历史
        </button>
      </footer>
    </div>
  </div>
{/if}

<ConfirmDialog
  open={confirmClear}
  title="清空历史记录"
  message="将删除全部历史记录与阅读进度，且不可恢复。继续吗？"
  confirmLabel="清空"
  onConfirm={() => {
    confirmClear = false;
    void historyStore.clear();
  }}
  onCancel={() => (confirmClear = false)}
/>

<style>
  .mask {
    position: fixed;
    /* 顶部让位给标题栏：面板打开时窗口仍可拖拽、标题栏按钮可用 */
    inset: var(--h-titlebar) 0 0 0;
    z-index: 45;
    background: rgb(0 0 0 / 0.18);
  }

  .panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(440px, 92vw);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    color: var(--ink);
    border-left: 1px solid var(--line);
    box-shadow: -8px 0 24px rgb(0 0 0 / 0.12);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
  }

  .head h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    white-space: nowrap;
  }

  .search {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0 8px;
    font-size: 12.5px;
    color: var(--ink);
    background: var(--base);
    border: 1px solid var(--line);
    border-radius: 4px;
    outline: none;
  }

  .search:focus {
    border-color: var(--accent);
  }

  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--muted);
    cursor: default;
  }

  .icon:hover {
    background: var(--hover);
    color: var(--ink);
  }

  .list {
    position: relative;
    flex: 1;
    overflow-y: auto;
  }

  .empty {
    margin: 40px 0;
    text-align: center;
    font-size: 13px;
    color: var(--muted);
  }

  .phantom {
    position: relative;
  }

  .entry {
    position: absolute;
    left: 0;
    right: 0;
    box-sizing: border-box;
    padding: 7px 12px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    border-bottom: 1px solid var(--line);
    cursor: default;
  }

  .entry:hover,
  .entry:focus-visible {
    background: var(--hover);
    outline: none;
  }

  .row1 {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 13px;
    font-weight: 500;
  }

  .del {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--muted);
    cursor: default;
    opacity: 0;
  }

  .entry:hover .del,
  .entry:focus-within .del {
    opacity: 1;
  }

  .del:hover {
    background: var(--base);
    color: var(--ink);
  }

  .row2 {
    font-size: 11.5px;
    color: var(--muted);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    direction: rtl;
    text-align: left;
  }

  .row3 {
    font-size: 11.5px;
    color: var(--muted);
  }

  .foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 12px;
    border-top: 1px solid var(--line);
  }

  .count {
    font-size: 12px;
    color: var(--muted);
  }

  .clear {
    height: 26px;
    padding: 0 12px;
    font-size: 12.5px;
    color: #b91c1c;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 4px;
    cursor: default;
  }

  .clear:hover:not(:disabled) {
    background: var(--hover);
  }

  .clear:disabled {
    color: var(--muted);
    opacity: 0.6;
  }
</style>
