<!--
  HistoryTab — 历史记录设置：保留条数 / 保留天数（滑块）、清空历史（二次确认）。
  范围与后端 defaults.rs 保持一致；清空不影响已打开的标签与文件。
-->
<script lang="ts">
  import { ask } from '@tauri-apps/plugin-dialog';

  import { describeIpcError, ipc, toIpcError } from '../../lib/ipc';
  import { toasts } from '../../lib/state/toasts.svelte';
  import ActionRow from './parts/ActionRow.svelte';
  import SliderRow from './parts/SliderRow.svelte';
  import { settings } from './store.svelte';

  const app = $derived(settings.snapshot?.app ?? null);

  /** 保存历史保留策略补丁 */
  function patchHistory(next: { maxEntries?: number; retentionDays?: number }): void {
    if (!app) return;
    void settings.saveApp({ history: { ...app.history, ...next } });
  }

  /** 清空历史记录（二次确认后执行） */
  async function clearHistory(): Promise<void> {
    let confirmed = false;
    try {
      confirmed = await ask('将删除全部历史记录（不影响已打开的标签与文件）。此操作不可撤销。', {
        title: '清空历史记录',
        kind: 'warning',
      });
    } catch (error) {
      if (import.meta.env.DEV) console.error('[settings] 确认对话框失败', error);
      return;
    }
    if (!confirmed) return;
    try {
      await ipc.clearHistory();
      toasts.show('历史记录已清空');
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }
</script>

{#if app}
  <p class="section-title">保留策略</p>
  <div class="rows">
    <SliderRow
      label="保留条数"
      desc="历史记录最大条数（100–1,000,000，超出按最旧优先清理）"
      value={app.history.maxEntries}
      min={100}
      max={1000000}
      step={100}
      unit="条"
      setting="maxEntries"
      onCommit={(value) => patchHistory({ maxEntries: value })}
    />
    <SliderRow
      label="保留天数"
      desc="超过天数的记录自动清理（1–36500 天）"
      value={app.history.retentionDays}
      min={1}
      max={36500}
      step={1}
      unit="天"
      setting="retentionDays"
      onCommit={(value) => patchHistory({ retentionDays: value })}
    />
  </div>

  <p class="section-title">维护</p>
  <div class="rows">
    <ActionRow
      label="清空历史记录"
      desc="删除全部历史条目（二次确认）"
      buttonLabel="清空…"
      setting="clearHistory"
      onAction={() => void clearHistory()}
    />
  </div>
{:else}
  <p class="loading">正在载入配置…</p>
{/if}
