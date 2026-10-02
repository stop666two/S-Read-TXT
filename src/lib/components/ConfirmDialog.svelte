<!--
  ConfirmDialog — 通用确认弹窗（应用内自定义弹窗）。
  用于：外部修改冲突（覆盖/取消）等破坏性操作的二次确认。
-->
<script lang="ts">
  interface Props {
    /** 是否显示 */
    open: boolean;
    /** 标题 */
    title: string;
    /** 正文说明 */
    message: string;
    /** 确认按钮文案 */
    confirmLabel: string;
    /** 取消按钮文案（默认「取消」） */
    cancelLabel?: string;
    /** 确认回调 */
    onConfirm: () => void;
    /** 取消/关闭回调 */
    onCancel: () => void;
  }
  let {
    open,
    title,
    message,
    confirmLabel,
    cancelLabel = '取消',
    onConfirm,
    onCancel,
  }: Props = $props();
</script>

{#if open}
  <div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onCancel()}>
    <div class="dialog" role="alertdialog" aria-modal="true" aria-label={title}>
      <h2>{title}</h2>
      <p>{message}</p>
      <div class="actions">
        <button class="btn" type="button" onclick={onCancel}>{cancelLabel}</button>
        <button class="btn danger" type="button" onclick={onConfirm}>{confirmLabel}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    /* 顶部让位给标题栏：弹窗打开时窗口仍可拖拽、标题栏按钮可用 */
    inset: var(--h-titlebar) 0 0 0;
    z-index: 45;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.28);
  }

  .dialog {
    width: 400px;
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
    padding: 0 16px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--base);
    color: var(--ink);
    font-size: 13px;
    cursor: pointer;
  }

  .btn.danger {
    border-color: #b4432f;
    background: #b4432f;
    color: #fff;
  }
</style>
