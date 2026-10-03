<!--
  GeneralTab — 常规设置：文件与标签 / 日志与备份 / 界面元素 / 启动行为。
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
  const reader = $derived(settings.snapshot?.reader ?? null);

  /** 状态栏元素开关（reader.statusBar 补丁保存） */
  function patchStatusBar(patch: Partial<NonNullable<typeof reader>['statusBar']>): void {
    if (!reader) return;
    void settings.saveReader({ statusBar: { ...reader.statusBar, ...patch } });
  }

  /** 启动行为开关（app.startup 补丁保存） */
  function patchStartup(patch: Partial<NonNullable<typeof app>['startup']>): void {
    if (!app) return;
    void settings.saveApp({ startup: { ...app.startup, ...patch } });
  }
</script>

{#if app && reader}
  <p class="section-title">文件与标签</p>
  <div class="rows">
    <SliderRow
      label="只读阈值"
      desc="超过该大小的文件以只读模式打开（可浏览、不可编辑；1–2048 MB）"
      value={app.maxFileSizeMB}
      min={1}
      max={2048}
      step={1}
      unit="MB"
      setting="maxFileSizeMB"
      onCommit={(value) => void settings.saveApp({ maxFileSizeMB: value })}
    />
    <SliderRow
      label="硬上限"
      desc="超过该大小的文件直接拒绝打开（绝对上限；100–16384 MB）"
      value={app.hardLimitMB}
      min={100}
      max={16384}
      step={1}
      unit="MB"
      setting="hardLimitMB"
      onCommit={(value) => void settings.saveApp({ hardLimitMB: value })}
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

  <p class="section-title">界面元素</p>
  <div class="rows">
    <ToggleRow
      label="状态栏：文件名与进度"
      desc="显示当前文件名与阅读百分比"
      checked={reader.statusBar.showFileName}
      setting="statusBar.showFileName"
      onCommit={(checked) => patchStatusBar({ showFileName: checked })}
    />
    <ToggleRow
      label="状态栏：阅读百分比"
      desc="单独控制百分比文本（文件名可独立开关）"
      checked={reader.statusBar.showPercent}
      setting="statusBar.showPercent"
      onCommit={(checked) => patchStatusBar({ showPercent: checked })}
    />
    <ToggleRow
      label="状态栏：文件大小"
      desc="显示当前文件的字节大小"
      checked={reader.statusBar.showSize}
      setting="statusBar.showSize"
      onCommit={(checked) => patchStatusBar({ showSize: checked })}
    />
    <ToggleRow
      label="状态栏：编码切换"
      desc="显示编码按钮（点击可自动检测 / 手动覆盖）"
      checked={reader.statusBar.showEncoding}
      setting="statusBar.showEncoding"
      onCommit={(checked) => patchStatusBar({ showEncoding: checked })}
    />
  </div>

  <p class="section-title">启动行为</p>
  <div class="rows">
    <ToggleRow
      label="启动时恢复上次会话"
      desc="重新打开上次的标签与阅读位置（惰性索引，不拖慢启动）"
      checked={app.startup.restoreSession}
      setting="startup.restoreSession"
      onCommit={(checked) => patchStartup({ restoreSession: checked })}
    />
    <ToggleRow
      label="启动时恢复窗口位置与大小"
      desc="关闭后每次以默认尺寸居中启动"
      checked={app.startup.restoreWindow}
      setting="startup.restoreWindow"
      onCommit={(checked) => patchStartup({ restoreWindow: checked })}
    />
  </div>
{:else}
  <p class="loading">正在载入配置…</p>
{/if}
