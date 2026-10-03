<!--
  BackgroundRow — 背景图文件行（注册表驱动，`reader.background.file`）。
  含：选择图片（原生对话框 → 复制入库并启用）、当前文件展示、移除。
  数值类参数（不透明度/填充/模糊/亮度）由 RegistryPage 的通用行组件渲染。
-->
<script lang="ts">
  import { open as openDialog } from '@tauri-apps/plugin-dialog';

  import { t } from '../../../lib/i18n/index.svelte';
  import { settings } from '../store.svelte';

  interface Props {
    /** 行标题 */
    label: string;
    /** 行描述 */
    desc: string;
    /** 当前背景图设置（快照） */
    value: { file?: string | null };
    /** 行级「恢复默认」回调 */
    onReset?: () => void;
    /** 恢复按钮的无障碍标签 */
    resetLabel?: string;
  }
  let { label, desc, value, onReset, resetLabel }: Props = $props();
  let busy = $state(false);

  /** 选择图片：原生对话框 → 复制入库并启用 */
  async function pick(): Promise<void> {
    if (busy) return;
    const picked = await openDialog({
      multiple: false,
      title: t('background.chooseTitle'),
      filters: [{ name: t('background.filter'), extensions: ['png', 'jpg', 'jpeg', 'webp'] }],
    });
    if (typeof picked !== 'string') return;
    busy = true;
    try {
      await settings.pickBackgroundFile(picked);
    } finally {
      busy = false;
    }
  }

  /** 移除背景图（文件 + 设置回退） */
  async function clear(): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      await settings.clearBackground();
    } finally {
      busy = false;
    }
  }
</script>

<div class="row">
  <span class="label">{label}<small>{desc}</small></span>
  <span class="control">
    {#if onReset}
      <button
        class="reset-mini"
        type="button"
        data-setting-reset="reader.background.file"
        title={resetLabel}
        aria-label={resetLabel}
        onclick={onReset}
      >
        <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true">
          <path
            d="M3.5 6.5a5 5 0 1 1 .6 4.4M3.5 3v3.5H7"
            fill="none"
            stroke="currentColor"
            stroke-width="1.2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
    {/if}
    <span class="file" title={value.file ?? undefined}>{value.file ?? t('background.none')}</span>
    <button
      class="mini"
      type="button"
      data-setting="reader.background.file"
      disabled={busy}
      onclick={() => void pick()}
    >
      {t('background.pick')}
    </button>
    {#if value.file}
      <button
        class="mini danger"
        type="button"
        data-setting="reader.background.clear"
        disabled={busy}
        onclick={() => void clear()}
      >
        {t('background.clear')}
      </button>
    {/if}
  </span>
</div>

<style>
  /* .row/.label/.control/.reset-mini 由全局 settings.css 提供；此处仅本行控件 */
  .file {
    max-width: 200px;
    overflow: hidden;
    color: var(--muted);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mini {
    padding: 3px 10px;
    border: 1px solid var(--line);
    border-radius: 5px;
    background: transparent;
    color: var(--ink);
    font: inherit;
    font-size: 12px;
    cursor: default;
  }

  .mini:hover:not(:disabled) {
    background: var(--hover);
  }

  .mini:disabled {
    opacity: 0.5;
  }

  .mini.danger {
    border-color: color-mix(in srgb, #c62828 45%, var(--line));
    color: #c62828;
  }
</style>
