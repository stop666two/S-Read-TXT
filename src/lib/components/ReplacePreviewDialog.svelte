<!--
  ReplacePreviewDialog — 「全部替换」二次确认弹窗（查找替换 v2）。
  职责：展示各命中行号与前后文本（被替换字符红色高亮、替换后文本绿色），
        支持逐条剔除（勾选/取消、全选/全不选）；确认后仅替换勾选项。
  截断（truncated）：命中超过列举上限时仅展示汇总，只能整体替换。
-->
<script lang="ts">
  import type { ReplacePreviewItem } from '../ipc';

  interface Props {
    /** 命中总数 */
    total: number;
    /** 是否因超过列举上限而截断（截断时不支持逐条剔除） */
    truncated: boolean;
    /** 列举的命中条目 */
    items: ReplacePreviewItem[];
    /** 确认：selected = 勾选序号（升序）；截断时为 null（全部替换） */
    onConfirm: (selected: number[] | null) => void;
    /** 取消 */
    onCancel: () => void;
  }
  let { total, truncated, items, onConfirm, onCancel }: Props = $props();

  /** 被剔除（不替换）的条目序号集合 */
  let excluded = $state<Set<number>>(new Set());

  /** 勾选数 */
  const selectedCount = $derived(total - excluded.size);

  /** 切换单条勾选状态（“删除”某一条待替换项）。 */
  function toggle(index: number): void {
    const next = new Set(excluded);
    if (next.has(index)) next.delete(index);
    else next.add(index);
    excluded = next;
  }

  /** 全选（清空剔除集合）。 */
  function selectAll(): void {
    excluded = new Set();
  }

  /** 全不选（剔除全部列举项）。 */
  function excludeAll(): void {
    excluded = new Set(items.map((item) => item.index));
  }

  /** 确认：截断时整体替换；否则仅替换勾选项（后端校验 stateId）。 */
  function confirm(): void {
    if (truncated) {
      onConfirm(null);
      return;
    }
    const selected = items.map((item) => item.index).filter((index) => !excluded.has(index));
    if (selected.length === 0) return;
    onConfirm(selected);
  }

  /** 把行文本按命中原文拆为 前/中/后 三段（命中段做红色高亮；找不到则整行普通展示）。 */
  function splitLine(item: ReplacePreviewItem): { before: string; hit: string; after: string } | null {
    if (item.matchedText.length === 0) return null;
    const at = item.lineText.indexOf(item.matchedText);
    if (at < 0) return null;
    return {
      before: item.lineText.slice(0, at),
      hit: item.matchedText,
      after: item.lineText.slice(at + item.matchedText.length),
    };
  }
</script>

<div
  class="backdrop"
  role="presentation"
  onmousedown={(event) => event.target === event.currentTarget && onCancel()}
>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="全部替换预览"
    tabindex="-1"
    onkeydown={(event) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onCancel();
      }
    }}
  >
    <h2>全部替换确认</h2>
    <p class="summary">
      共命中 <strong>{total}</strong> 处{truncated ? '' : `，已勾选 ${selectedCount} 处`}。
      {#if truncated}
        命中过多，仅列出前 {items.length} 条；未列出的命中将一并替换。
      {/if}
    </p>

    <div class="list">
      {#each items as item (item.index)}
        {@const parts = splitLine(item)}
        <div class="item" class:excluded={excluded.has(item.index)}>
          {#if !truncated}
            <input
              type="checkbox"
              checked={!excluded.has(item.index)}
              aria-label={`替换第 ${item.startRow + 1} 行`}
              onchange={() => toggle(item.index)}
            />
          {/if}
          <div class="body">
            <div class="line">
              <span class="no">第 {item.startRow + 1} 行</span>
              {#if parts}
                <span class="text">{parts.before}<mark class="old">{parts.hit}</mark>{parts.after}</span>
              {:else}
                <span class="text">{item.lineText}</span>
              {/if}
            </div>
            <div class="line replacement">
              <span class="no">替换为</span>
              <span class="text"><mark class="new">{item.replacementText}</mark></span>
            </div>
          </div>
        </div>
      {/each}
    </div>

    <div class="actions">
      {#if !truncated}
        <button class="btn" type="button" onclick={selectAll}>全选</button>
        <button class="btn" type="button" onclick={excludeAll}>全不选</button>
      {/if}
      <span class="spacer"></span>
      <button class="btn" type="button" onclick={onCancel}>取消</button>
      <button
        class="btn primary"
        type="button"
        disabled={!truncated && selectedCount === 0}
        onclick={confirm}
      >
        {truncated ? `全部替换 ${total} 处` : `替换已选 ${selectedCount} 处`}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    /* 顶部让位给标题栏：弹窗打开时窗口仍可拖拽、标题栏按钮可用 */
    inset: var(--h-titlebar) 0 0 0;
    z-index: 55;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.3);
  }

  .dialog {
    display: flex;
    flex-direction: column;
    width: min(560px, calc(100vw - 48px));
    max-height: min(520px, calc(100vh - 96px));
    padding: 18px 20px 14px;
    background: var(--surface);
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.18);
  }

  h2 {
    margin: 0 0 8px;
    font-size: 15px;
    font-weight: 600;
  }

  .summary {
    margin: 0 0 10px;
    font-size: 13px;
    line-height: 1.6;
    color: var(--muted);
  }

  .summary strong {
    color: var(--ink);
  }

  .list {
    flex: 1;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--base);
  }

  .item {
    display: flex;
    gap: 8px;
    padding: 7px 10px;
    border-bottom: 1px solid var(--line);
  }

  .item:last-child {
    border-bottom: none;
  }

  .item.excluded {
    opacity: 0.45;
  }

  .item input[type='checkbox'] {
    margin-top: 3px;
    accent-color: var(--accent);
  }

  .body {
    flex: 1;
    min-width: 0;
  }

  .line {
    display: flex;
    gap: 8px;
    font-size: 12.5px;
    line-height: 1.7;
  }

  .no {
    flex: none;
    min-width: 52px;
    color: var(--muted);
    text-align: right;
  }

  .text {
    min-width: 0;
    overflow-wrap: anywhere;
    white-space: pre-wrap;
  }

  mark.old {
    background: rgba(196, 43, 28, 0.22);
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
  }

  mark.new {
    background: rgba(22, 128, 57, 0.2);
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
  }

  .replacement .text {
    color: var(--ink);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }

  .spacer {
    flex: 1;
  }

  .btn {
    height: 30px;
    padding: 0 14px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--base);
    color: var(--ink);
    font-size: 13px;
    cursor: pointer;
  }

  .btn.primary {
    border-color: var(--accent);
    background: var(--accent);
    color: #fff;
  }

  .btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
