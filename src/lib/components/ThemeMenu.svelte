<!--
  ThemeMenu — 主题切换下拉：工具栏入口。
  组成：调色盘按钮 + 下拉（跟随系统 + 主题清单，单选标记）+ 导入/导出动作。
  行为：点击外部收起；选择后回调 onPick(id)；条目文案由 App 按当前语言构建
        （system 用语言包，其余取主题清单 name/nameEn）。
-->
<script lang="ts">
  import { t } from '../i18n/index.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    /** 当前主题 id（system 或具体主题，用于单选标记） */
    themeId: string;
    /** 主题条目（App 构建：跟随系统在最前） */
    entries: { id: string; label: string }[];
    /** 当前主题显示名（按钮提示文案） */
    currentLabel: string;
    /** 选择主题 */
    onPick: (id: string) => void;
    /** 导入主题（原生对话框由 App 处理） */
    onImport: () => void;
    /** 导出当前主题 */
    onExport: () => void;
  }
  let { themeId, entries, currentLabel, onPick, onImport, onExport }: Props = $props();

  /** 下拉开合 */
  let open = $state(false);

  /** 选择主题并收起 */
  function pick(id: string): void {
    open = false;
    onPick(id);
  }
</script>

<svelte:window
  onclick={(event) => {
    // 点击下拉之外区域时收起（与编码下拉同理，实例间互不影响）
    const target = event.target as HTMLElement | null;
    if (!target?.closest('.theme-wrap')) open = false;
  }}
/>

<div class="theme-wrap">
  <button
    class="icon-btn"
    title={t('toolbar.theme', { name: currentLabel })}
    aria-label={t('toolbar.themeMenu')}
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    <Icon name="palette" />
  </button>
  {#if open}
    <div class="dropdown" role="menu">
      {#each entries as item (item.id)}
        <button class="item" onclick={() => pick(item.id)}>
          <span class="radio" class:on={themeId === item.id}></span>
          <span>{item.label}</span>
        </button>
      {/each}
      <div class="separator"></div>
      <button
        class="item"
        onclick={() => {
          open = false;
          onImport();
        }}
      >
        <span>{t('theme.import')}</span>
      </button>
      <button
        class="item"
        onclick={() => {
          open = false;
          onExport();
        }}
      >
        <span>{t('theme.export')}</span>
      </button>
    </div>
  {/if}
</div>

<style>
  .theme-wrap {
    position: relative;
    display: inline-flex;
  }

  .icon-btn {
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: default;
    transition:
      background-color 80ms ease,
      color 80ms ease;
  }

  .icon-btn:hover {
    background: var(--hover);
    color: var(--ink);
  }

  .dropdown {
    position: absolute;
    top: calc(100% + 3px);
    left: 0;
    z-index: 40;
    min-width: 180px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--ink);
    font: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: default;
  }

  .item:hover {
    background: var(--hover);
  }

  .radio {
    width: 12px;
    height: 12px;
    border: 1px solid var(--muted);
    border-radius: 50%;
    box-sizing: border-box;
  }

  .radio.on {
    border: 4px solid var(--accent);
  }

  .separator {
    height: 1px;
    margin: 4px 2px;
    background: var(--line);
  }
</style>
