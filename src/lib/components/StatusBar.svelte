<script lang="ts">
  // 状态栏：有文件时「左 文件名+阅读百分比 / 右 大小+编码（点击切换）」；无文件时显示应用名与版本。
  // 元素显隐由设置控制（阅读排版 → 状态栏元素开关；缺省全显示）。
  import { t } from '../i18n/index.svelte';
  import EncodingMenu from './EncodingMenu.svelte';

  interface Props {
    /** 文件名（无打开文件时不传） */
    fileName?: string;
    /** 阅读百分比（0–100） */
    percent?: number;
    /** 文件大小展示文本（已格式化） */
    sizeLabel?: string;
    /** 当前编码标签（有效编码展示：自动检测结果或手动覆盖） */
    encodingLabel?: string;
    /** 支持的编码列表（传入时编码可点击切换，向后兼容缺省为纯展示） */
    encodings?: string[];
    /** 当前手动编码（null = 自动检测；单选标记） */
    encodingOverride?: string | null;
    /** 编码切换回调（null = 自动检测） */
    onEncodingChange?: (label: string | null) => void;
    /** 应用版本（无文件时右侧展示，如 v0.0.1-beta） */
    version?: string;
    /** 显示文件名与进度（设置项） */
    showFileName?: boolean;
    /** 显示阅读百分比（设置项） */
    showPercent?: boolean;
    /** 显示文件大小（设置项） */
    showSize?: boolean;
    /** 显示编码切换（设置项） */
    showEncoding?: boolean;
    /** 是否只读（文件超过只读阈值：状态栏展示「只读」标记） */
    readOnly?: boolean;
  }
  let {
    fileName,
    percent,
    sizeLabel,
    encodingLabel,
    encodings = [],
    encodingOverride = null,
    onEncodingChange,
    version = '',
    showFileName = true,
    showPercent = true,
    showSize = true,
    showEncoding = true,
    readOnly = false,
  }: Props = $props();
</script>

<footer class="status-bar">
  {#if fileName !== undefined}
    <span class="left">
      {#if showFileName}{fileName}{/if}
      {#if showFileName && showPercent}<span class="dot"> · </span>{/if}
      {#if showPercent}{t('status.reading', { percent: Math.round(percent ?? 0) })}{/if}
      {#if readOnly}
        <span class="readonly" title={t('status.readOnlyHint')}>{t('status.readOnly')}</span>
      {/if}
    </span>
    <span class="right">
      {#if showSize && sizeLabel}
        <span>{sizeLabel}</span>
        {#if showEncoding}<span class="dot">·</span>{/if}
      {/if}
      {#if showEncoding}
        {#if onEncodingChange}
          <EncodingMenu
            displayLabel={encodingLabel ?? '—'}
            {encodings}
            override={encodingOverride}
            onPick={onEncodingChange}
            openUp
          />
        {:else}
          <span class="encoding" title={t('status.encodingHint')}>{encodingLabel ?? '—'}</span>
        {/if}
      {/if}
    </span>
  {:else}
    <span class="left">S-Read-TXT {version}</span>
    <span class="right">{t('status.noFile')}</span>
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

  .readonly {
    margin-left: 8px;
    padding: 0 5px;
    border: 1px solid var(--line);
    border-radius: 3px;
    font-size: 11px;
    color: var(--warn, #8a5200);
  }
</style>
