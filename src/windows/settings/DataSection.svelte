<script lang="ts">
  // 数据位置（P0-10）：展示当前数据目录与迁移入口。
  // 流程：选择目标目录 → 二次确认 → 复制校验（后端）→ 结果展示 → 立即重启/稍后。
  // 说明：迁移成功后使用新位置需重启（指针在启动时读取）；旧目录被占用时延迟清理。
  import { open } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';

  import ConfirmDialog from '../../lib/components/ConfirmDialog.svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import type { DataDirStatus, MigrationReport } from '../../lib/ipc';
  import { ipc, toIpcError } from '../../lib/ipc';

  let status = $state<DataDirStatus | null>(null);
  let busy = $state(false);
  let pendingTarget = $state<string | null>(null);
  let report = $state<MigrationReport | null>(null);
  let message = $state('');

  onMount(() => {
    void ipc
      .dataDirStatus()
      .then((value) => (status = value))
      .catch(() => undefined);
  });

  /** 选择迁移目标目录（原生目录选择器；取消静默返回）。 */
  async function pickTarget(): Promise<void> {
    if (busy) return;
    message = '';
    const selected = await open({
      directory: true,
      multiple: false,
      title: t('settings.data.pickTitle'),
    });
    if (typeof selected !== 'string') return;
    pendingTarget = selected;
  }

  /** 确认执行迁移（后端完成复制/校验/指针/清理；失败保留原目录）。 */
  async function confirmMigrate(): Promise<void> {
    const target = pendingTarget;
    pendingTarget = null;
    if (!target) return;
    busy = true;
    try {
      report = await ipc.migrateDataDir(target);
    } catch (error) {
      report = null;
      message = toIpcError(error).message;
    } finally {
      busy = false;
    }
  }

  /** 立即重启（新进程接管；当前进程随之退出，Promise 通常不会返回）。 */
  function restartNow(): void {
    void ipc.restartApp().catch((error) => {
      message = toIpcError(error).message;
    });
  }
</script>

<section class="card">
  <h3>{t('settings.data.title')}</h3>
  <p class="desc">{t('settings.data.desc')}</p>
  <div class="row">
    <span class="label">{t('settings.data.current')}</span>
    <code class="path" data-data-dir>{status?.dir ?? '…'}</code>
  </div>
  {#if status?.persisted}
    <div class="row">
      <span class="label">{t('settings.data.persisted')}</span>
      <code class="path">{status.persisted}</code>
    </div>
  {/if}
  <div class="actions">
    <button class="btn" type="button" data-setting="migrateData" disabled={busy} onclick={pickTarget}>
      {t('settings.data.migrate')}
    </button>
  </div>
  {#if message}
    <p class="msg err" data-data-msg>{message}</p>
  {/if}
  {#if report}
    <p class="msg ok" data-data-report>
      {t('settings.data.done', { files: report.copiedFiles, bytes: report.copiedBytes })}
      {report.skipped > 0 ? t('settings.data.skipped', { count: report.skipped }) : ''}
    </p>
    <div class="actions">
      <button class="btn primary" type="button" data-setting="restartNow" onclick={restartNow}>
        {t('settings.data.restartNow')}
      </button>
      <button class="btn" type="button" data-setting="restartLater" onclick={() => (report = null)}>
        {t('settings.data.restartLater')}
      </button>
    </div>
  {/if}
</section>

{#if pendingTarget}
  <ConfirmDialog
    open={pendingTarget !== null}
    title={t('settings.data.confirmTitle')}
    message={t('settings.data.confirmMessage', { target: pendingTarget })}
    confirmLabel={t('settings.data.confirm')}
    onConfirm={confirmMigrate}
    onCancel={() => (pendingTarget = null)}
  />
{/if}

<style>
  .card {
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--surface);
  }

  h3 {
    margin: 0 0 6px;
    font-size: 14px;
    color: var(--ink);
  }

  .desc {
    margin: 0 0 10px;
    font-size: 12.5px;
    color: var(--muted);
  }

  .row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 6px;
    font-size: 12.5px;
  }

  .label {
    flex: none;
    color: var(--muted);
  }

  .path {
    overflow: hidden;
    color: var(--ink);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }

  .btn {
    padding: 5px 12px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--surface);
    color: var(--ink);
    font: inherit;
    font-size: 12.5px;
    cursor: default;
  }

  .btn:hover {
    background: var(--hover);
  }

  .btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .btn.primary {
    border-color: var(--accent);
    color: var(--accent);
  }

  .msg {
    margin: 8px 0 0;
    font-size: 12.5px;
  }

  .msg.ok {
    color: var(--accent);
  }

  .msg.err {
    color: var(--warn, #8a5200);
  }
</style>
