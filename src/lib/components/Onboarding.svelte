<!--
  Onboarding — 首启引导：介绍打开文件 / 多标签 / 历史记录 / 快捷键。
  关闭时可勾选「不再显示」（写入 settings.showOnboarding=false）。
-->
<script lang="ts">
  import { t } from '../i18n/index.svelte';

  interface Props {
    /** 关闭回调；参数 = 是否勾选「不再显示」 */
    onClose: (dontShowAgain: boolean) => void;
  }
  let { onClose }: Props = $props();

  /** 是否勾选「不再显示」 */
  let dontShow = $state(false);
</script>

<div class="overlay" role="dialog" aria-modal="true" aria-label={t('onboarding.aria')}>
  <div class="card">
    <h2>{t('onboarding.title')}</h2>
    <ul>
      <li><strong>{t('onboarding.openFile')}</strong>{t('onboarding.openFileDesc')}</li>
      <li><strong>{t('onboarding.tabs')}</strong>{t('onboarding.tabsDesc')}</li>
      <li><strong>{t('onboarding.history')}</strong>{t('onboarding.historyDesc')}</li>
      <li><strong>{t('onboarding.shortcuts')}</strong>{t('onboarding.shortcutsDesc')}</li>
    </ul>
    <div class="actions">
      <label class="dont-show">
        <input type="checkbox" bind:checked={dontShow} />
        {t('onboarding.dontShow')}
      </label>
      <button type="button" onclick={() => onClose(dontShow)}>{t('onboarding.start')}</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    /* 顶部让位给标题栏：弹窗打开时窗口仍可拖拽、标题栏按钮可用 */
    inset: var(--h-titlebar) 0 0 0;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgb(0 0 0 / 0.35);
  }

  .card {
    width: 460px;
    max-width: calc(100vw - 48px);
    box-sizing: border-box;
    padding: 22px 24px 18px;
    background: var(--surface);
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: 12px;
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.18);
  }

  h2 {
    margin: 0 0 12px;
    font-size: 17px;
    font-weight: 600;
  }

  ul {
    margin: 0;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 13px;
    line-height: 1.6;
  }

  .actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 18px;
  }

  .dont-show {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    font-size: 12.5px;
  }

  button {
    padding: 6px 18px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--accent);
    color: #fff;
    cursor: pointer;
    font-size: 13px;
  }

  button:hover {
    filter: brightness(1.05);
  }
</style>
