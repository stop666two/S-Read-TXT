<!--
  ClipboardHistoryDialog — 剪贴板历史。
  职责：展示历史条目（最新在前），支持插入到编辑器、删除单条、清空全部。
  说明：插入复用编辑层输入链路（选区替换/多光标统一处理）；历史为空时不显示列表。
-->
<script lang="ts">
  import { t, i18n } from '../i18n/index.svelte';
  import type { ClipboardEntry } from '../ipc';
  import ConfirmDialog from './ConfirmDialog.svelte';

  interface Props {
    /** 历史条目（最新在前；后端返回的顺序） */
    entries: ClipboardEntry[];
    /** 插入指定文本到编辑区 */
    onInsert: (text: string) => void;
    /** 删除指定下标条目 */
    onRemove: (index: number) => void;
    /** 清空全部历史 */
    onClear: () => void;
    /** 关闭弹窗 */
    onClose: () => void;
  }
  let { entries, onInsert, onRemove, onClear, onClose }: Props = $props();

  /** 清空确认弹窗可见性 */
  let confirmClear = $state(false);

  /** 预览截断（完整文本保留在条目内，仅渲染截断以控制长文本开销） */
  const PREVIEW_MAX_CHARS = 200;

  /** 生成条目预览文本（截断 + 省略号） */
  function previewOf(text: string): string {
    const normalized = text.replace(/\r\n?/g, '\n');
    return normalized.length > PREVIEW_MAX_CHARS
      ? `${normalized.slice(0, PREVIEW_MAX_CHARS)}…`
      : normalized;
  }

  /** 条目元信息（时间 · 字符数；时间按当前界面语言格式化） */
  function metaOf(entry: ClipboardEntry): string {
    const time = new Date(entry.at).toLocaleString(i18n.locale);
    return t('clipboardHistory.meta', { time, chars: String(entry.text.length) });
  }

  /** Esc 关闭（确认弹窗打开时交给其自身处理） */
  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && !confirmClear) {
      event.stopPropagation();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onClose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label={t('clipboardHistory.title')}>
    <h2>{t('clipboardHistory.title')}</h2>
    {#if entries.length === 0}
      <p class="empty" data-clipboard-empty>{t('clipboardHistory.empty')}</p>
    {:else}
      <ul class="items" data-clipboard-list>
        {#each entries as entry, index (entry.at + index)}
          <li class="item" data-clipboard-item>
            <pre class="preview">{previewOf(entry.text)}</pre>
            <div class="row">
              <span class="meta">{metaOf(entry)}</span>
              <span class="spacer"></span>
              <button
                class="btn ghost"
                type="button"
                data-clipboard-insert
                onclick={() => onInsert(entry.text)}
              >
                {t('clipboardHistory.insert')}
              </button>
              <button
                class="btn ghost danger"
                type="button"
                data-clipboard-remove
                onclick={() => onRemove(index)}
              >
                {t('clipboardHistory.remove')}
              </button>
            </div>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="actions">
      <button
        class="btn danger"
        type="button"
        data-setting="clipboardHistory.clear"
        disabled={entries.length === 0}
        onclick={() => (confirmClear = true)}
      >
        {t('clipboardHistory.clear')}
      </button>
      <span class="spacer"></span>
      <button class="btn" type="button" data-setting="clipboardHistory.close" onclick={onClose}>
        {t('common.close')}
      </button>
    </div>
  </div>
</div>

<ConfirmDialog
  open={confirmClear}
  title={t('clipboardHistory.clearTitle')}
  message={t('clipboardHistory.clearMessage', { count: String(entries.length) })}
  confirmLabel={t('clipboardHistory.clear')}
  onConfirm={() => {
    confirmClear = false;
    onClear();
  }}
  onCancel={() => (confirmClear = false)}
/>

<style>
  .backdrop {
    position: fixed;
    inset: var(--h-titlebar) 0 0 0;
    background: color-mix(in srgb, var(--ink) 24%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 40;
  }

  .dialog {
    width: min(560px, calc(100vw - 48px));
    max-height: min(640px, calc(100vh - var(--h-titlebar) - 48px));
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 16px;
    box-shadow: 0 8px 28px color-mix(in srgb, var(--ink) 18%, transparent);
  }

  h2 {
    margin: 0 0 10px;
    font-size: 15px;
    font-weight: 600;
    color: var(--ink);
  }

  .empty {
    margin: 18px 0;
    color: var(--muted);
    font-size: 13px;
    text-align: center;
  }

  .items {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    flex: 1;
  }

  .item {
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 8px 10px;
    margin-bottom: 8px;
    background: var(--base);
  }

  .preview {
    margin: 0 0 6px;
    font-family: var(--font-reading), system-ui, sans-serif;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--ink);
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 68px;
    overflow: hidden;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .meta {
    font-size: 12px;
    color: var(--muted);
  }

  .spacer {
    flex: 1;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-top: 10px;
    border-top: 1px solid var(--line);
    margin-top: 4px;
  }

  .btn {
    font-size: 13px;
    padding: 5px 12px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--ink);
    cursor: pointer;
  }

  .btn:hover:not(:disabled) {
    background: var(--hover);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .btn.ghost {
    padding: 3px 10px;
    font-size: 12px;
  }

  .btn.danger {
    color: #c0392b;
    border-color: color-mix(in srgb, #c0392b 45%, var(--line));
  }

  .btn.danger:hover:not(:disabled) {
    background: color-mix(in srgb, #c0392b 12%, var(--surface));
  }
</style>
