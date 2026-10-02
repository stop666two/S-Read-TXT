<script lang="ts">
  // 状态栏：有文件时「左 文件名+阅读百分比 / 右 大小+编码」；无文件时显示应用名与版本。
  interface Props {
    /** 文件名（无打开文件时不传） */
    fileName?: string;
    /** 阅读百分比（0–100） */
    percent?: number;
    /** 文件大小展示文本（已格式化） */
    sizeLabel?: string;
    /** 当前编码标签 */
    encodingLabel?: string;
    /** 应用版本（无文件时右侧展示，如 v0.0.1-beta） */
    version?: string;
  }
  let { fileName, percent, sizeLabel, encodingLabel, version = '' }: Props = $props();
</script>

<footer class="status-bar">
  {#if fileName !== undefined}
    <span class="left">{fileName} · 阅读 {Math.round(percent ?? 0)}%</span>
    <span class="right">
      {#if sizeLabel}
        <span>{sizeLabel}</span>
        <span class="dot">·</span>
      {/if}
      <span class="encoding" title="切换编码">{encodingLabel ?? '—'}</span>
    </span>
  {:else}
    <span class="left">S-Read-TXT {version}</span>
    <span class="right">未打开文件</span>
  {/if}
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: var(--h-status);
    padding: 0 10px;
    background: var(--chrome);
    border-top: 1px solid var(--line);
    color: var(--muted);
    font-size: 12px;
    user-select: none;
  }

  .right {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .dot {
    color: var(--line);
  }

  .encoding {
    color: var(--muted);
  }
</style>
