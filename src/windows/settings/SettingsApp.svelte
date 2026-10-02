<!--
  SettingsApp — 设置窗口外壳（阶段 5：快捷键页签可用；其余页签待后续阶段填充）。
  布局：自定义标题栏 + 顶部页签 + 内容区；主题跟随 reader.json（与主窗口观感一致）。
-->
<script lang="ts">
  import { onMount } from 'svelte';

  import Toast from '../../lib/components/Toast.svelte';
  import TitleBar from '../../lib/components/TitleBar.svelte';
  import { ipc } from '../../lib/ipc';
  import AboutTab from './AboutTab.svelte';
  import GeneralTab from './GeneralTab.svelte';
  import HistoryTab from './HistoryTab.svelte';
  import ShortcutsTab from './ShortcutsTab.svelte';
  import TypographyTab from './TypographyTab.svelte';
  import { settings } from './store.svelte';

  /** 页签定义（顺序即展示顺序） */
  const TABS = [
    { id: 'general', label: '常规' },
    { id: 'typography', label: '阅读排版' },
    { id: 'shortcuts', label: '快捷键' },
    { id: 'history', label: '历史记录' },
    { id: 'about', label: '关于' },
  ] as const;

  type TabId = (typeof TABS)[number]['id'];

  /** 当前页签（默认「常规」；菜单打开时经 take_settings_tab 定位） */
  let tab = $state<TabId>('general');

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
    return () => media.removeEventListener('change', apply);
  });
</script>

<div class="shell">
  <TitleBar title="设置" showMaximize={false} />
  <div class="tabs" role="tablist" aria-label="设置页签">
    {#each TABS as item (item.id)}
      <button
        class="tab"
        class:active={tab === item.id}
        type="button"
        role="tab"
        aria-selected={tab === item.id}
        onclick={() => (tab = item.id)}
      >
        {item.label}
      </button>
    {/each}
  </div>
  <main class="content">
    {#if tab === 'general'}
      <GeneralTab />
    {:else if tab === 'typography'}
      <TypographyTab />
    {:else if tab === 'shortcuts'}
      <ShortcutsTab />
    {:else if tab === 'history'}
      <HistoryTab />
    {:else}
      <AboutTab />
    {/if}
  </main>
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

  .tabs {
    display: flex;
    gap: 2px;
    padding: 8px 12px 0;
    background: var(--chrome);
    border-bottom: 1px solid var(--line);
  }

  .tab {
    border: none;
    background: transparent;
    color: var(--muted);
    padding: 6px 12px;
    border-radius: 6px 6px 0 0;
    cursor: pointer;
    font-size: 13px;
  }

  .tab:hover {
    background: var(--hover);
  }

  .tab.active {
    color: var(--ink);
    background: var(--base);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .content {
    flex: 1;
    overflow: auto;
    padding: 16px;
  }
</style>
