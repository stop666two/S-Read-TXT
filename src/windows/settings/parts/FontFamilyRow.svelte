<!--
  FontFamilyRow — 正文字体行（注册表驱动的排版分组内使用）。
  含：字体下拉（系统候选 + 自定义字体）、导入按钮、已导入字体清单（删除）。
  导入后自动切换使用；删除正在使用的字体时回退默认字体。
-->
<script lang="ts">
  import { onMount } from 'svelte';

  import { ask, open as openDialog } from '@tauri-apps/plugin-dialog';

  import { formatBytes } from '../../../lib/format';
  import { t } from '../../../lib/i18n/index.svelte';
  import { describeIpcError, ipc, toIpcError, type FontEntry } from '../../../lib/ipc';
  import { toasts } from '../../../lib/state/toasts.svelte';

  interface Props {
    /** 行标题 */
    label: string;
    /** 行描述 */
    desc: string;
    /** 当前字体（系统名或 `custom:<文件名>`） */
    value: string;
    /** 设置项 id（写入 data-setting） */
    setting: string;
    /** 选择回调（立即落盘） */
    onCommit: (value: string) => void;
    /** 行级「恢复默认」回调 */
    onReset?: () => void;
    /** 恢复按钮的无障碍标签 */
    resetLabel?: string;
  }
  let { label, desc, value, setting, onCommit, onReset, resetLabel }: Props = $props();

  /** 自定义字体值前缀（与 App.svelte 的解析约定一致） */
  const CUSTOM_PREFIX = 'custom:';
  /** 默认字体（删除正在使用的自定义字体时回退） */
  const DEFAULT_FONT = 'Microsoft YaHei';

  /** 常见中文字体候选（当前值不在列表时动态补入，避免选择丢失） */
  const FONT_CANDIDATES = [
    'Microsoft YaHei',
    'SimSun',
    'SimHei',
    'KaiTi',
    'FangSong',
    'Noto Sans CJK SC',
    'Source Han Sans SC',
  ];

  /** 已导入的自定义字体 */
  let fonts = $state<FontEntry[]>([]);

  async function refreshFonts(): Promise<void> {
    try {
      fonts = await ipc.listFonts();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  onMount(() => {
    void refreshFonts();
  });

  /** 字体候选（系统候选 + 自定义；当前值不在列表时动态补入） */
  const options = $derived.by(() => {
    const list: { value: string; label: string }[] = FONT_CANDIDATES.map((name) => ({
      value: name,
      label: name,
    }));
    for (const font of fonts) {
      list.push({
        value: `${CUSTOM_PREFIX}${font.fileName}`,
        label: t('settings.fontManager.customSuffix', { name: font.label }),
      });
    }
    if (value && !list.some((option) => option.value === value)) {
      list.unshift({ value, label: value });
    }
    return list;
  });

  /** 导入字体：选择文件 → 复制入库 → 刷新列表并切换使用 */
  async function importFont(): Promise<void> {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: t('settings.fontManager.title'), extensions: ['ttf', 'otf', 'woff', 'woff2'] }],
    });
    if (typeof picked !== 'string') return;
    try {
      const entry = await ipc.importFont(picked);
      await refreshFonts();
      onCommit(`${CUSTOM_PREFIX}${entry.fileName}`);
      toasts.show(t('settings.fontManager.imported', { name: entry.label }));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 删除自定义字体（确认后；若正在使用则回退默认字体） */
  async function removeFont(font: FontEntry): Promise<void> {
    const confirmed = await ask(t('settings.fontManager.deleteMessage', { name: font.label }), {
      title: t('settings.fontManager.deleteTitle'),
      kind: 'warning',
    });
    if (!confirmed) return;
    try {
      await ipc.removeFont(font.fileName);
      if (value === `${CUSTOM_PREFIX}${font.fileName}`) {
        onCommit(DEFAULT_FONT);
      }
      await refreshFonts();
      toasts.show(t('settings.fontManager.deleted', { name: font.label }));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }
</script>

<div class="row">
  <span class="label" id={`lbl-${setting}`}>{label}<small>{desc}</small></span>
  <span class="control">
    {#if onReset}
      <button
        class="reset-mini"
        type="button"
        data-setting-reset={setting}
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
    <select
      data-setting={setting}
      aria-labelledby={`lbl-${setting}`}
      {value}
      onchange={(event) => onCommit((event.currentTarget as HTMLSelectElement).value)}
    >
      {#each options as item (item.value)}
        <option value={item.value}>{item.label}</option>
      {/each}
    </select>
    <button class="btn" type="button" data-setting="importFont" onclick={() => void importFont()}>
      {t('settings.fontManager.import')}
    </button>
  </span>
</div>

{#each fonts as font (font.fileName)}
  <div class="row">
    <span class="label">{font.label}<small>{font.fileName} · {formatBytes(font.sizeBytes)}</small></span>
    <span class="control">
      <button
        class="btn danger"
        type="button"
        data-setting={`font-${font.fileName}`}
        onclick={() => void removeFont(font)}
      >
        {t('settings.fontManager.delete')}
      </button>
    </span>
  </div>
{/each}

{#if fonts.length === 0}
  <div class="row">
    <span class="label">{t('settings.fontManager.none')}<small>{t('settings.fontManager.noneDesc')}</small></span>
    <span class="control"></span>
  </div>
{/if}
