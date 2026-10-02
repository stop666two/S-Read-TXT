<script lang="ts">
  // 状态栏：左侧文件名 + 阅读百分比；右侧文件大小 + 编码（阶段 2b 静态示例）。
  // 真实数据在阶段 2c 由标签状态驱动；编码文本可点击切换（阶段 2c 接下拉）。
  interface Props {
    /** 文件名（无打开文件时不传） */
    fileName?: string;
    /** 阅读百分比（0–100，取整展示） */
    percent?: number;
    /** 文件大小展示文本 */
    sizeLabel?: string;
    /** 当前编码标签 */
    encodingLabel?: string;
  }
  let { fileName = '示例文本.txt', percent = 42, sizeLabel = '12.4 KB', encodingLabel = 'UTF-8' }: Props =
    $props();
</script>

<footer class="status-bar">
  <span class="left">{fileName} · 阅读 {Math.round(percent)}%</span>
  <span class="right">
    <span>{sizeLabel}</span>
    <span class="dot">·</span>
    <span class="encoding" title="切换编码">{encodingLabel}</span>
  </span>
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
