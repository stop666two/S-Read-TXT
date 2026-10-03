<!--
  SettingsApp — 设置窗口外壳（P0-4：注册表驱动 + 全局搜索 + 左导航）。
  契约（自动化与外部依赖）：导航容器保留 .tabs、项为 .tab[role="tab"]；
  内容沿用 settings.css 的 .rows / .row / .label / .control 类；
  控件 data-setting = 设置项完整 id（如 reader.typography.fontSize）。
-->
<script lang="ts">
  import { onMount } from 'svelte';

  import Toast from '../../lib/components/Toast.svelte';
  import TitleBar from '../../lib/components/TitleBar.svelte';
  import { setLocale, t } from '../../lib/i18n/index.svelte';
  import { ipc } from '../../lib/ipc';
  import AboutTab from './AboutTab.svelte';
  import BackupSection from './BackupSection.svelte';
  import DiskSection from './DiskSection.svelte';
  import HistoryTab from './HistoryTab.svelte';
  import RegistryPage from './RegistryPage.svelte';
  import ShortcutsTab from './ShortcutsTab.svelte';
  import { settings } from './store.svelte';

  // 设置页共享样式（类名 .rows/.row/.label/.control 与冒烟脚本约定一致）
  import './settings.css';

  /** 页签定义（顺序即导航顺序；图标为 16 分辨率线性 SVG path） */
  const TABS = [
    {
      id: 'general',
      labelKey: 'settings.category.general',
      icon: 'M3 5h10M3 11h10M6 3.4v3.2M11 9.4v3.2',
    },
    { id: 'typography', labelKey: 'settings.category.reader', icon: 'M4 4h8M8 4v8M6.2 12h3.6' },
    {
      id: 'shortcuts',
      labelKey: 'settings.category.shortcuts',
      icon: 'M2.8 5.2h10.4v5.6H2.8zM5 7.4h.01M7.2 7.4h.01M9.4 7.4h.01M5 9.4h6',
    },
    {
      id: 'history',
      labelKey: 'settings.category.history',
      icon: 'M8 2.8a5.2 5.2 0 1 0 5.2 5.2M8 5.4V8l2 1.4',
    },
    {
      id: 'about',
      labelKey: 'settings.category.about',
      icon: 'M8 2.8a5.2 5.2 0 1 0 0 10.4A5.2 5.2 0 0 0 8 2.8zM8 7.4v4M8 5.2h.01',
    },
  ] as const;

  type TabId = (typeof TABS)[number]['id'];

  /** 当前页签（默认「常规」；菜单打开时经 take_settings_tab 定位） */
  let tab = $state<TabId>('general');

  /** 搜索关键词（非空时注册表页切换为全局搜索视图） */
  let query = $state('');

  /** 是否处于搜索态（搜索态隐藏常规页的独立区块） */
  const searchActive = $derived(query.trim().length > 0);

  // 界面语言跟随配置：载入与修改时同步本窗口语言包（即时切换）
  $effect(() => {
    setLocale(settings.snapshot?.app.locale ?? 'zh-CN');
  });

  onMount(() => {
    // 菜单请求的初始页签（如 帮助→快捷键/关于；读取即清空）
    void ipc.takeSettingsTab().then((pending) => {
      if (pending && TABS.some((item) => item.id === pending)) {
        tab = pending as TabId;
      }
    });
    // 应用已保存主题：设置窗口与主窗口同一套令牌
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    let choice = 'system';
    const apply = (): void => {
      const resolved = choice === 'system' ? (media.matches ? 'dark' : 'light') : choice;
      document.documentElement.dataset.theme = resolved;
    };
    void settings.load().then(() => {
      choice = settings.snapshot?.reader.theme ?? 'system';
      apply();
      media.addEventListener('change', apply);
    });
    void settings.loadRegistry();
    void settings.loadDisk();
    return () => media.removeEventListener('change', apply);
  });
</script>

<div class="shell">
  <TitleBar title={t('settings.title')} showMaximize={false} />
  <div class="body">
    <!-- 使用 div 而非 nav：nav 不允许承载 tablist 交互角色（a11y）；类名与角色契约保持不变 -->
    <div class="tabs" role="tablist" aria-label={t('settings.title')}>
      {#each TABS as item (item.id)}
        <button
          class="tab"
          class:active={tab === item.id}
          type="button"
          role="tab"
          aria-selected={tab === item.id}
          onclick={() => (tab = item.id)}
        >
          <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
            <path
              d={item.icon}
              fill="none"
              stroke="currentColor"
              stroke-width="1.3"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
          <span>{t(item.labelKey)}</span>
        </button>
      {/each}
    </div>
    <main class="content">
      <div class="searchbar">
        <input
          class="search"
          type="search"
          placeholder={t('settings.search.placeholder')}
          aria-label={t('settings.search.placeholder')}
          bind:value={query}
        />
      </div>
      {#if tab === 'general'}
        <RegistryPage groups={['app.basic', 'app.startup']} {query} />
        {#if !searchActive}
          <BackupSection />
          <DiskSection />
        {/if}
      {:else if tab === 'typography'}
        <RegistryPage groups={['reader.basic', 'reader.typography', 'reader.statusBar']} {query} />
      {:else if tab === 'shortcuts'}
        <ShortcutsTab />
      {:else if tab === 'history'}
        <HistoryTab {query} />
      {:else}
        <AboutTab />
      {/if}
    </main>
  </div>
  <Toast />
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--base);
    color: var(--ink);
  }

  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }

  .tabs {
    width: 184px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 14px 10px;
    background: var(--chrome);
  }

  .tab {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--muted);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    transition:
      background-color 90ms ease,
      color 90ms ease;
  }

  .tab:hover {
    background: var(--hover);
    color: var(--ink);
  }

  .tab.active {
    background: var(--hover);
    color: var(--ink);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .tab:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .content {
    flex: 1;
    overflow: auto;
    padding: 24px 28px;
  }
</style>
