<!--
  GeneralTab — 常规设置：文件上限 / 标签上限（滑块）、日志级别、默认备份。
  修改即存（经共享 store）；数值范围与后端 defaults.rs 保持一致（越界由后端再归一）。
-->
<script lang="ts">
  import ChoiceRow from './parts/ChoiceRow.svelte';
  import SliderRow from './parts/SliderRow.svelte';
  import ToggleRow from './parts/ToggleRow.svelte';
  import { settings } from './store.svelte';

  /** 日志级别选项（与后端 LogLevel 序列化一致） */
  const LOG_LEVELS = [
    { value: 'error', label: '仅错误' },
    { value: 'warn', label: '警告及以上' },
    { value: 'info', label: '信息及以上（默认）' },
    { value: 'debug', label: '调试（最详细）' },
  ];

  const app = $derived(settings.snapshot?.app ?? null);
</script>

{#if app}
  <p class="section-title">文件与标签</p>
  <div class="rows">
    <SliderRow
      label="可打开文件大小上限"
      desc="超过该大小的文件将拒绝打开（1–2048 MB）"
      value={app.maxFileSizeMB}
      min={1}
      max={2048}
      step={1}
      unit="MB"
      setting="maxFileSizeMB"
      onCommit={(value) => void settings.saveApp({ maxFileSizeMB: value })}
    />
    <SliderRow
      label="标签数量上限"
      desc="同时打开的标签数上限（1–200）"
      value={app.maxTabs}
      min={1}
      max={200}
      step={1}
      unit="个"
      setting="maxTabs"
      onCommit={(value) => void settings.saveApp({ maxTabs: value })}
    />
  </div>

  <p class="section-title">日志与备份</p>
  <div class="rows">
    <ChoiceRow
      label="日志级别"
      desc="写入 data/logs 的详细程度（环境变量 SRT_LOG_LEVEL 优先）"
      value={app.logLevel}
      options={LOG_LEVELS}
      setting="logLevel"
      onCommit={(value) => void settings.saveApp({ logLevel: value })}
    />
    <ToggleRow
      label="首次保存生成 .bak 备份"
      desc="对已存在的文件首次保存前，生成同名 .bak 备份"
      checked={app.saveBackupEnabled}
      setting="saveBackupEnabled"
      onCommit={(checked) => void settings.saveApp({ saveBackupEnabled: checked })}
    />
  </div>
{:else}
  <p class="loading">正在载入配置…</p>
{/if}
