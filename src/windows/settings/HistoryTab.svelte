<!--
  HistoryTab — 历史记录设置：保留条数 / 保留天数 / 清空历史。
  说明：历史面板（浏览与单条删除）在阶段 7 提供；本页签提供保留策略与清空。
-->
<script lang="ts">
  import { ask } from '@tauri-apps/plugin-dialog';

  import { describeIpcError, ipc, toIpcError } from '../../lib/ipc';
  import { toasts } from '../../lib/state/toasts.svelte';
  import { settings } from './store.svelte';

  const app = $derived(settings.snapshot?.app ?? null);

  /** 数字输入解析（非法值返回 null，不提交） */
  function readNumber(event: Event): number | null {
    const value = Number.parseInt((event.target as HTMLInputElement).value, 10);
    return Number.isFinite(value) ? value : null;
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
  <div class="rows">
    <label class="row">
      <span class="label">
        保留条数
        <small>历史记录最大条数（1–100000，超出按最旧优先清理）</small>
      </span>
      <input
        type="number"
        min="1"
        max="100000"
        step="100"
        value={app.history.maxEntries}
        onchange={(event) => {
          const value = readNumber(event);
          if (value !== null) {
            void settings.saveApp({ history: { ...app.history, maxEntries: value } });
          }
        }}
      />
    </label>
    <label class="row">
      <span class="label">
        保留天数
        <small>超过天数的记录自动清理（1–3650 天）</small>
      </span>
      <input
        type="number"
        min="1"
        max="3650"
        value={app.history.retentionDays}
        onchange={(event) => {
          const value = readNumber(event);
          if (value !== null) {
            void settings.saveApp({ history: { ...app.history, retentionDays: value } });
          }
        }}
      />
    </label>
    <div class="row">
      <span class="label">
        清空历史记录
        <small>删除全部历史条目（二次确认）</small>
      </span>
      <button class="danger" type="button" onclick={() => void clearHistory()}>清空…</button>
    </div>
  </div>
{:else}
  <p class="loading">正在载入配置…</p>
{/if}

<style>
  .rows {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line);
    border-radius: 8px;
    overflow: hidden;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    background: var(--surface);
    font-size: 13px;
  }

  .row + .row {
    border-top: 1px solid var(--line);
  }

  .label {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .label small {
    color: var(--muted);
    font-size: 11.5px;
  }

  input[type='number'] {
    width: 110px;
    padding: 4px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--base);
    color: var(--ink);
  }

  .danger {
    padding: 5px 12px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--surface);
    color: #b0413e;
    cursor: pointer;
    font-size: 12px;
  }

  .danger:hover {
    border-color: #b0413e;
  }

  .loading {
    margin-top: 40px;
    text-align: center;
    color: var(--muted);
    font-size: 13px;
  }
</style>
