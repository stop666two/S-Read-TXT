<!--
  UnsavedDialog — 未保存修改三态弹窗（保存 / 不保存 / 取消）。
  用于：关闭脏标签、退出应用（多标签时逐个询问保存）。
  设计依据：设计 D19「脏关闭弹窗（保存/不保存/取消）」。
-->
<script lang="ts">
  import { t } from '../i18n/index.svelte';

  interface Props {
    /** 是否显示 */
    open: boolean;
    /** 标题 */
    title: string;
    /** 正文说明（如包含文件名与影响范围） */
    message: string;
    /** 保存（可异步：多标签时逐个保存，由调用方队列控制） */
    onSave: () => void;
    /** 不保存并继续 */
    onDiscard: () => void;
    /** 取消（中止关闭） */
    onCancel: () => void;
  }
  let { open, title, message, onSave, onDiscard, onCancel }: Props = $props();
</script>

{#if open}
  <div
    class="backdrop"
    role="presentation"
    onmousedown={(e) => e.target === e.currentTarget && onCancel()}
  >
    <div class="dialog" role="alertdialog" aria-modal="true" aria-label={title}>
      <h2>{title}</h2>
      <p>{message}</p>
      <div class="actions">
        <button class="btn" type="button" onclick={onCancel}>{t('common.cancel')}</button>
        <button class="btn danger" type="button" onclick={onDiscard}>{t('unsaved.discard')}</button>
        <button class="btn primary" type="button" onclick={onSave}>{t('common.save')}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    /* 顶部让位给标题栏：弹窗打开时窗口仍可拖拽、标题栏按钮可用 */
    inset: var(--h-titlebar) 0 0 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.28);
  }

  .dialog {
    width: 420px;
    padding: 20px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    color: var(--ink);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.18);
  }

  h2 {
    margin: 0 0 10px;
    font-size: 15px;
    font-weight: 600;
  }

  p {
    margin: 0;
    font-size: 13px;
    line-height: 1.7;
    color: var(--muted);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 20px;
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

  .btn.danger {
    border-color: #b4432f;
    color: #b4432f;
  }
</style>
