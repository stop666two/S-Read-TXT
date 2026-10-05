<!--
  DiskSection — 磁盘占用与缓存清理。
  分项展示（日志 / WebView 缓存 / 字体 / 备份 / 数据文件 / 其他）+ 总量；
  清理动作（日志 / WebView 缓存 / 备份文件）各自二次确认；被占用文件在 Toast 中提示 skipped。
  契约：data-disk-item / data-disk-total 供自动化断言；清理按钮 data-setting=clearLogs|clearWebview|clearBackups。
-->
<script lang="ts">
  import ConfirmDialog from '../../lib/components/ConfirmDialog.svelte';
  import { formatBytes } from '../../lib/format';
  import { t } from '../../lib/i18n/index.svelte';
  import type { MessageKey } from '../../lib/i18n/zh-CN';
  import { settings } from './store.svelte';

  /** 可清理范围（与后端 ClearScope 字符串键一致） */
  type Scope = 'logs' | 'webview' | 'backups';

  /** 分项 → 语言包键 */
  const ITEM_KEYS: Record<string, MessageKey> = {
    logs: 'settings.disk.logs',
    webview: 'settings.disk.webview',
    fonts: 'settings.disk.fonts',
    backups: 'settings.disk.backups',
    files: 'settings.disk.files',
    others: 'settings.disk.others',
  };

  /** 清理动作 → 语言包键 */
  const CLEAR_KEYS: Record<Scope, MessageKey> = {
    logs: 'settings.disk.clearLogs',
    webview: 'settings.disk.clearWebview',
    backups: 'settings.disk.clearBackups',
  };

  const disk = $derived(settings.disk);

  /** 待确认的清理范围（null = 无确认框） */
  let pending = $state<Scope | null>(null);

  function scopeLabel(scope: Scope): string {
    return t(CLEAR_KEYS[scope]);
  }

  function confirmClear(): void {
    const scope = pending;
    pending = null;
    if (scope) void settings.clearCache(scope);
  }
</script>

<p class="section-title">{t('settings.disk.title')}</p>
<div class="rows">
  {#if disk}
    {#each disk.items as item (item.key)}
      <div class="row">
        <span class="label">{ITEM_KEYS[item.key] ? t(ITEM_KEYS[item.key]) : item.key}</span>
        <span class="control disk-value" data-disk-item={item.key}>
          {formatBytes(item.bytes)}
          <span class="files">({item.files})</span>
        </span>
      </div>
    {/each}
    <div class="row">
      <span class="label">{t('settings.disk.total')}</span>
      <span class="control disk-value" data-disk-total>{formatBytes(disk.totalBytes)}</span>
    </div>
    <div class="row">
      <span class="label">{t('settings.disk.clearHint')}</span>
      <span class="control btn-row">
        <button class="btn" type="button" data-setting="clearLogs" onclick={() => (pending = 'logs')}>
          {scopeLabel('logs')}
        </button>
        <button
          class="btn"
          type="button"
          data-setting="clearWebview"
          onclick={() => (pending = 'webview')}
        >
          {scopeLabel('webview')}
        </button>
        <button
          class="btn"
          type="button"
          data-setting="clearBackups"
          onclick={() => (pending = 'backups')}
        >
          {scopeLabel('backups')}
        </button>
      </span>
    </div>
  {:else}
    <p class="loading">{t('settings.loading')}</p>
  {/if}
</div>

{#if pending}
  <ConfirmDialog
    open={true}
    title={t('settings.disk.clearTitle')}
    message={t('settings.disk.clearMessage', { scope: scopeLabel(pending) })}
    confirmLabel={t('settings.disk.clearConfirm')}
    onCancel={() => (pending = null)}
    onConfirm={confirmClear}
  />
{/if}

<style>
  .disk-value {
    color: var(--ink);
    font-variant-numeric: tabular-nums;
  }

  .files {
    color: var(--muted);
    font-size: 12px;
  }

  .loading {
    color: var(--muted);
    font-size: 13px;
  }
</style>
