<!--
  ThemeRow — 主题行（P0-5）：注册表 `reader.theme` 的专属控件。
  组成：主题下拉（跟随系统 + 主题清单）+ 导入/导出动作 + 用户主题删除（二次确认）。
  契约：select 带 data-setting={setting}（值 = 主题 id）；名称按当前语言取自清单。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { open, save } from '@tauri-apps/plugin-dialog';

  import ConfirmDialog from '../../../lib/components/ConfirmDialog.svelte';
  import { i18n, t } from '../../../lib/i18n/index.svelte';
  import { describeIpcError, ipc, toIpcError, type ThemeSummary } from '../../../lib/ipc';
  import { toasts } from '../../../lib/state/toasts.svelte';
  import { settings } from '../store.svelte';

  interface Props {
    /** 设置项名称（行标题） */
    label: string;
    /** 补充说明（行副标题） */
    desc: string;
    /** 设置标识（写入 data-setting = 设置项完整 id） */
    setting: string;
    /** 行级「恢复默认」回调（提供时显示按钮） */
    onReset?: () => void;
    /** 恢复按钮的无障碍标签 */
    resetLabel?: string;
  }
  let { label, desc, setting, onReset, resetLabel }: Props = $props();

  /** 待删除的用户主题（非空时显示确认框） */
  let removeTarget = $state<ThemeSummary | null>(null);

  const current = $derived(settings.snapshot?.reader.theme ?? 'system');
  const currentTheme = $derived(settings.themes.find((theme) => theme.id === current) ?? null);

  /** 主题本地化名（当前语言） */
  function nameOf(theme: ThemeSummary): string {
    return i18n.locale === 'en' ? theme.nameEn : theme.name;
  }

  onMount(() => {
    void settings.loadThemes();
  });

  async function onPick(event: Event): Promise<void> {
    await settings.pickTheme((event.currentTarget as HTMLSelectElement).value);
  }

  async function onImport(): Promise<void> {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t('theme.jsonFilter'), extensions: ['json'] }],
    });
    if (typeof path === 'string') await settings.importThemeFrom(path);
  }

  async function onExport(): Promise<void> {
    try {
      const resolved = await ipc.getTheme(null);
      const path = await save({
        defaultPath: `${resolved.id}.json`,
        filters: [{ name: t('theme.jsonFilter'), extensions: ['json'] }],
      });
      if (typeof path === 'string') await settings.exportThemeTo(path, resolved.id);
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
      value={current}
      onchange={onPick}
    >
      <option value="system">{t('theme.system')}</option>
      {#each settings.themes as theme (theme.id)}
        <option value={theme.id}>{nameOf(theme)}</option>
      {/each}
    </select>
    <button class="mini" type="button" onclick={() => void onImport()}>{t('theme.import')}</button>
    <button class="mini" type="button" onclick={() => void onExport()}>{t('theme.export')}</button>
    {#if currentTheme && !currentTheme.builtin}
      <button class="mini danger" type="button" onclick={() => (removeTarget = currentTheme)}>
        {t('theme.remove')}
      </button>
    {/if}
  </span>
</div>

{#if removeTarget}
  <ConfirmDialog
    open
    title={t('theme.removeTitle')}
    message={t('theme.removeMessage', { name: nameOf(removeTarget) })}
    confirmLabel={t('theme.remove')}
    onConfirm={() => {
      const target = removeTarget;
      removeTarget = null;
      if (target) void settings.removeTheme(target.id);
    }}
    onCancel={() => (removeTarget = null)}
  />
{/if}

<style>
  /* .row/.label/.control/.reset-mini 由全局 settings.css 提供；此处仅本行控件 */
  .mini {
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: 5px;
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: 12px;
    cursor: default;
  }

  .mini:hover {
    background: var(--hover);
    color: var(--ink);
  }

  .mini.danger {
    color: var(--danger, #c0392b);
  }
</style>
