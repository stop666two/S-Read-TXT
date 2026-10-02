<!--
  App.svelte — 根组件：应用外壳（菜单栏 / 工具栏 / 标签栏 / 阅读区 / 状态栏）。
  阶段 2b：完成布局骨架与主题系统；阅读数据与真实交互在 2c 接线。
-->
<script lang="ts">
  import MenuBar from './lib/components/MenuBar.svelte';
  import ToolBar from './lib/components/ToolBar.svelte';
  import TabBar from './lib/components/TabBar.svelte';
  import ReaderView from './lib/components/ReaderView.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import type { ResolvedTheme, ThemeChoice } from './lib/types';

  /** 主题选择（默认跟随系统；阶段 8 起由设置加载/保存） */
  let themeChoice = $state<ThemeChoice>('system');

  // 解析主题：「跟随系统」依据 prefers-color-scheme，其余直接采用；
  // 结果写入 <html data-theme>，全部令牌随之切换。
  $effect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = (): void => {
      const resolved: ResolvedTheme =
        themeChoice === 'system' ? (media.matches ? 'dark' : 'light') : themeChoice;
      document.documentElement.dataset.theme = resolved;
    };
    apply();
    media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
  });
</script>

<div class="shell">
  <MenuBar {themeChoice} onThemeChange={(theme) => (themeChoice = theme)} />
  <ToolBar {themeChoice} onThemeChange={(theme) => (themeChoice = theme)} />
  <TabBar />
  <ReaderView />
  <StatusBar />
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
</style>
