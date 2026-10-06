<!--
  PrivacySection — 隐私数据清除（历史/会话/快照/批注/剪贴板/日志）。
  勾选范围 → 二次确认 → 清除并刷新用量；被占用文件计入 skipped 并在 Toast 说明。
  契约：data-setting="clearPrivacy"、复选框 data-privacy-check / 计数 data-privacy-count / 行 data-privacy-item。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import ConfirmDialog from '../../lib/components/ConfirmDialog.svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import type { MessageKey } from '../../lib/i18n/zh-CN';
  import type { PrivacyScope } from '../../lib/ipc';
  import { settings } from './store.svelte';

  /** 范围键 → 语言包键 */
  const SCOPE_KEYS: Record<keyof PrivacyScope, MessageKey> = {
    history: 'settings.privacy.history',
    session: 'settings.privacy.session',
    snapshots: 'settings.privacy.snapshots',
    annotations: 'settings.privacy.annotations',
    clipboard: 'settings.privacy.clipboard',
    logs: 'settings.privacy.logs',
  };

  const SCOPES = Object.keys(SCOPE_KEYS) as (keyof PrivacyScope)[];

  /** 勾选状态（默认与当前一致：全部勾选） */
  let checked = $state<PrivacyScope>({
    history: true,
    session: true,
    snapshots: true,
    annotations: true,
    clipboard: true,
    logs: true,
  });

  let confirmOpen = $state(false);

  const usage = $derived(settings.privacyUsage);
  const anyChecked = $derived(SCOPES.some((key) => checked[key]));

  onMount(() => {
    void settings.loadPrivacyUsage();
  });

  function confirmClear(): void {
    confirmOpen = false;
    void settings.clearPrivacy({ ...checked });
  }
</script>

<p class="section-title">{t('settings.privacy.title')}</p>
<div class="rows">
  {#each SCOPES as scope (scope)}
    <div class="row" data-privacy-item={scope}>
      <span class="label" id={`privacy-lbl-${scope}`}>{t(SCOPE_KEYS[scope])}</span>
      <span class="control privacy-control">
        <span class="privacy-count" data-privacy-count={scope}>
          {usage ? usage[scope] : t('settings.loading')}
        </span>
        <input
          type="checkbox"
          data-privacy-check={scope}
          aria-labelledby={`privacy-lbl-${scope}`}
          bind:checked={checked[scope]}
        />
      </span>
    </div>
  {/each}
  <div class="row">
    <span class="label">{t('settings.privacy.hint')}</span>
    <span class="control">
      <button
        class="btn danger"
        type="button"
        data-setting="clearPrivacy"
        disabled={!anyChecked}
        onclick={() => (confirmOpen = true)}
      >
        {t('settings.privacy.clearButton')}
      </button>
    </span>
  </div>
</div>

{#if confirmOpen}
  <ConfirmDialog
    open={true}
    title={t('settings.privacy.clearTitle')}
    message={t('settings.privacy.clearMessage')}
    confirmLabel={t('settings.privacy.clearConfirm')}
    onCancel={() => (confirmOpen = false)}
    onConfirm={confirmClear}
  />
{/if}

<style>
  .privacy-control {
    gap: 12px;
  }

  .privacy-count {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    font-size: 12.5px;
  }
</style>
