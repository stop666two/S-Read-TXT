<!--
  BackupSection — 备份与恢复（导出 / 导入 / 全部恢复默认）。
  导出：选择保存路径 → 后端写 JSON 包；导入：选择文件 → 后端强校验后应用（导入前自动备份）。
  全部恢复默认：二次确认后调用后端重置（不可撤销）。
-->
<script lang="ts">
  import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';

  import ConfirmDialog from '../../lib/components/ConfirmDialog.svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { settings } from './store.svelte';

  /** 「全部恢复默认」确认框可见性 */
  let confirmAll = $state(false);

  /** 导出：默认文件名 + JSON 过滤；取消时不做任何事 */
  async function exportSettings(): Promise<void> {
    const path = await saveDialog({
      defaultPath: 's-read-txt-settings.json',
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (typeof path !== 'string') return;
    await settings.exportTo(path);
  }

  /** 导入：选择导出文件（强校验失败会以 Toast 显示具体原因） */
  async function importSettings(): Promise<void> {
    const path = await openDialog({
      multiple: false,
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (typeof path !== 'string') return;
    await settings.importFrom(path);
  }
</script>

<p class="section-title">{t('settings.backupTitle')}</p>
<div class="rows">
  <div class="row">
    <span class="label">{t('settings.backupDesc')}</span>
    <span class="control btn-row">
      <button class="btn" type="button" data-setting="exportSettings" onclick={() => void exportSettings()}>
        {t('settings.export')}
      </button>
      <button class="btn" type="button" data-setting="importSettings" onclick={() => void importSettings()}>
        {t('settings.import')}
      </button>
      <button
        class="btn danger"
        type="button"
        data-setting="resetAllSettings"
        onclick={() => (confirmAll = true)}
      >
        {t('settings.resetAll')}
      </button>
    </span>
  </div>
</div>

{#if confirmAll}
  <ConfirmDialog
    open={true}
    title={t('settings.resetAllTitle')}
    message={t('settings.resetAllMessage')}
    confirmLabel={t('settings.resetAllConfirm')}
    onCancel={() => (confirmAll = false)}
    onConfirm={() => {
      confirmAll = false;
      void settings.resetScope({ kind: 'all' });
    }}
  />
{/if}
