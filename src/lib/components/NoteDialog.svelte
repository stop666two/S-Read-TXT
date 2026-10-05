<!--
  NoteDialog — 注释/待办输入弹窗。
  单文本框 + 确定/取消；Ctrl+Enter 快速确认，Esc 取消。
  契约：data-note-dialog / textarea[data-note-input] / [data-note-ok] / [data-note-cancel]。
-->
<script lang="ts">
  import { untrack } from 'svelte';

  import { t } from '../../lib/i18n/index.svelte';

  interface Props {
    /** 弹窗是否可见（父组件控制挂载条件亦可） */
    open: boolean;
    /** 标题（如「添加注释」/「添加待办」） */
    title: string;
    /** 初始文本（编辑既有注释时使用） */
    initial?: string;
    /** 确认回调（文本已去首尾空白且非空） */
    onConfirm: (text: string) => void;
    /** 取消回调（Esc / 取消按钮 / 点击遮罩） */
    onCancel: () => void;
  }
  let { open, title, initial = '', onConfirm, onCancel }: Props = $props();

  // 仅取初始值（意图明确：外部后续变更不覆盖输入；避免 state_referenced_locally 告警）
  let value = $state(untrack(() => initial));

  function confirm(): void {
    const text = value.trim();
    if (!text) return;
    onConfirm(text);
    value = '';
  }
</script>

{#if open}
  <div
    class="mask"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) onCancel();
    }}
  >
    <div class="note-dialog" role="dialog" aria-modal="true" aria-label={title} data-note-dialog>
      <h3>{title}</h3>
      <textarea
        data-note-input
        rows="4"
        placeholder={t('annot.notePlaceholder')}
        bind:value
        onkeydown={(event) => {
          if (event.key === 'Escape') {
            event.stopPropagation();
            onCancel();
          } else if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
            event.preventDefault();
            confirm();
          }
        }}
      ></textarea>
      <div class="actions">
        <button class="btn" type="button" data-note-ok disabled={!value.trim()} onclick={confirm}>
          {t('common.ok')}
        </button>
        <button class="mini" type="button" data-note-cancel onclick={onCancel}>
          {t('common.cancel')}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .mask {
    position: fixed;
    inset: var(--h-titlebar) 0 0 0;
    z-index: 130;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.35);
  }

  .note-dialog {
    width: min(420px, calc(100vw - 48px));
    padding: 14px 16px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--surface);
    color: var(--ink);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.18);
  }

  h3 {
    margin: 0 0 8px;
    font-size: 13px;
    font-weight: 600;
  }

  textarea {
    width: 100%;
    box-sizing: border-box;
    padding: 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--base);
    color: var(--ink);
    font: inherit;
    font-size: 12px;
    resize: vertical;
    outline: none;
  }

  textarea:focus {
    border-color: var(--accent);
  }

  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 10px;
  }

  .btn {
    height: 26px;
    padding: 0 12px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--accent);
    color: #fff;
    font: inherit;
    font-size: 12px;
    cursor: default;
  }

  .btn:disabled {
    opacity: 0.5;
  }

  .mini {
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: 12px;
    cursor: default;
  }

  .mini:hover {
    background: var(--hover);
    color: var(--ink);
  }
</style>
