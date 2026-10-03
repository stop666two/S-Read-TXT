<!--
  HistoryTab — 历史记录设置（注册表驱动「历史保留」分组 + 清空历史）。
  范围与后端 defaults.rs 保持一致；清空不影响已打开的标签与文件。
-->
<script lang="ts">
  import { ask } from '@tauri-apps/plugin-dialog';

  import { t } from '../../lib/i18n/index.svelte';
  import { describeIpcError, ipc, toIpcError } from '../../lib/ipc';
  import { toasts } from '../../lib/state/toasts.svelte';
  import RegistryPage from './RegistryPage.svelte';

  interface Props {
    /** 搜索关键词（透传给注册表页；非空时切换为全局搜索视图） */
    query?: string;
  }
  let { query = '' }: Props = $props();

  /** 清空历史记录（二次确认后执行；不影响已打开的标签与文件） */
  async function clearHistory(): Promise<void> {
    let confirmed = false;
    try {
      confirmed = await ask(t('history.clearMessage'), {
        title: t('history.clearTitle'),
        kind: 'warning',
      });
    } catch (error) {
      if (import.meta.env.DEV) console.error('[settings] confirm dialog failed', error);
      return;
    }
    if (!confirmed) return;
    try {
      await ipc.clearHistory();
      toasts.show(t('history.clearDone'));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }
</script>

<RegistryPage groups={['app.history']} {query} />

{#if !query.trim()}
  <div class="rows">
    <div class="row">
      <span class="label">{t('settings.historyClearRow')}<small>{t('settings.historyClearRowDesc')}</small></span>
      <span class="control">
        <button class="btn danger" type="button" data-setting="clearHistory" onclick={() => void clearHistory()}>
          {t('history.clear')}
        </button>
      </span>
    </div>
  </div>
{/if}
