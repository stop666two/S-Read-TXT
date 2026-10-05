<!--
  DataDirDialog.svelte —— 「数据目录不可写」引导弹窗。
  触发：启动探测到程序目录/data 不可写（便携模式无法保存历史/设置/会话）。
  交互：选择可写目录（推荐；仅本次运行有效）或仅本次只读运行（不保存任何数据）。
-->
<script lang="ts">
  import { t } from '../i18n/index.svelte';

  interface Props {
    /** 不可写目录（展示给用户定位问题） */
    dir: string;
    /** 失败原因（来自后端探测的系统错误信息） */
    message?: string | null;
    /** 选择可写目录（打开系统目录选择器并应用；由 App 实现） */
    onChoose: () => void | Promise<void>;
    /** 仅本次只读运行（不保存任何数据） */
    onSkip: () => void;
  }

  let { dir, message = null, onChoose, onSkip }: Props = $props();
  /** 目录选择进行中（防重复点击；等待系统选择器与后端校验） */
  let busy = $state(false);

  /** 触发目录选择流程（结果由 App 处理） */
  async function choose(): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      await onChoose();
    } finally {
      busy = false;
    }
  }
</script>

<div class="ddl-backdrop">
  <div class="ddl" role="alertdialog" aria-modal="true" aria-labelledby="ddl-title">
    <h2 id="ddl-title">{t('dataDir.title')}</h2>
    <p class="ddl-line">{t('dataDir.line')}</p>
    <p class="ddl-dir">{dir}</p>
    {#if message}
      <p class="ddl-msg">{message}</p>
    {/if}
    <p class="ddl-line">
      {t('dataDir.chooseHint1')}<strong>{t('dataDir.chooseHintStrong')}</strong>{t('dataDir.chooseHint2')}
    </p>
    <div class="ddl-actions">
      <button class="ddl-primary" onclick={() => void choose()} disabled={busy}>
        {busy ? t('dataDir.busy') : t('dataDir.choose')}
      </button>
      <button class="ddl-secondary" onclick={onSkip} disabled={busy}>{t('dataDir.skip')}</button>
    </div>
  </div>
</div>

<style>
  .ddl-backdrop {
    position: fixed;
    /* 顶部让位给标题栏：弹窗打开时窗口仍可拖拽、标题栏按钮可用 */
    inset: var(--h-titlebar) 0 0 0;
    z-index: 60;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgb(0 0 0 / 0.35);
  }
  .ddl {
    width: min(460px, calc(100vw - 48px));
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 20px 22px;
    color: var(--ink);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 8px 28px rgb(0 0 0 / 0.18);
  }
  .ddl h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .ddl-line {
    margin: 0;
    font-size: 13px;
    line-height: 1.6;
    color: var(--muted);
  }
  .ddl-dir {
    margin: 0;
    padding: 6px 8px;
    font-size: 12px;
    word-break: break-all;
    background: var(--base);
    border: 1px solid var(--line);
    border-radius: 4px;
  }
  .ddl-msg {
    margin: 0;
    font-size: 12px;
    color: #b45309;
  }
  .ddl-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  .ddl-actions button {
    height: 30px;
    padding: 0 14px;
    font-size: 13px;
    cursor: pointer;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--surface);
    color: var(--ink);
  }
  .ddl-actions button:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .ddl-actions .ddl-primary {
    color: #fff;
    background: var(--accent);
    border-color: var(--accent);
  }
</style>
