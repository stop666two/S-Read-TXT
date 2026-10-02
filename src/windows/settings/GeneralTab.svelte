<!--
  GeneralTab — 常规设置：可打开文件大小上限 / 标签数量上限 / 日志级别 / 默认备份。
  修改即存（经共享 store）；数值范围与后端一致（越界由后端再次归一）。
-->
<script lang="ts">
  import { settings } from './store.svelte';

  /** 日志级别选项（与后端 LogLevel 序列化一致） */
  const LOG_LEVELS = [
    { value: 'error', label: '仅错误' },
    { value: 'warn', label: '警告及以上' },
    { value: 'info', label: '信息及以上（默认）' },
    { value: 'debug', label: '调试（最详细）' },
  ] as const;

  /** 数字输入解析（非法值返回 null，不提交） */
  function readNumber(event: Event): number | null {
    const value = Number.parseInt((event.target as HTMLInputElement).value, 10);
    return Number.isFinite(value) ? value : null;
  }

  const app = $derived(settings.snapshot?.app ?? null);
</script>

{#if app}
  <div class="rows">
    <label class="row">
      <span class="label">
        可打开文件大小上限
        <small>超过该大小的文件将拒绝打开（1–2048 MB）</small>
      </span>
      <input
        type="number"
        min="1"
        max="2048"
        value={app.maxFileSizeMB}
        onchange={(event) => {
          const value = readNumber(event);
          if (value !== null) void settings.saveApp({ maxFileSizeMB: value });
        }}
      />
      <span class="unit">MB</span>
    </label>
    <label class="row">
      <span class="label">
        标签数量上限
        <small>同时打开的标签数上限（1–100）</small>
      </span>
      <input
        type="number"
        min="1"
        max="100"
        value={app.maxTabs}
        onchange={(event) => {
          const value = readNumber(event);
          if (value !== null) void settings.saveApp({ maxTabs: value });
        }}
      />
      <span class="unit">个</span>
    </label>
    <label class="row">
      <span class="label">
        日志级别
        <small>写入 data/logs 的详细程度（环境变量 SRT_LOG_LEVEL 优先）</small>
      </span>
      <select
        value={app.logLevel}
        onchange={(event) => void settings.saveApp({ logLevel: (event.target as HTMLSelectElement).value })}
      >
        {#each LOG_LEVELS as item (item.value)}
          <option value={item.value}>{item.label}</option>
        {/each}
      </select>
    </label>
    <label class="row">
      <span class="label">
        首次保存生成 .bak 备份
        <small>对已存在的文件首次保存前，生成同名 .bak 备份</small>
      </span>
      <input
        type="checkbox"
        checked={app.saveBackupEnabled}
        onchange={(event) => void settings.saveApp({ saveBackupEnabled: (event.target as HTMLInputElement).checked })}
      />
    </label>
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
    width: 90px;
    padding: 4px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--base);
    color: var(--ink);
  }

  select {
    padding: 4px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--base);
    color: var(--ink);
  }

  .unit {
    color: var(--muted);
    font-size: 12px;
    width: 22px;
  }

  .loading {
    margin-top: 40px;
    text-align: center;
    color: var(--muted);
    font-size: 13px;
  }
</style>
