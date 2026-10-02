<!--
  App.svelte — 根组件：应用外壳与全局接线。
  阶段 2c：打开文件（对话框/拖拽）、标签栏、空状态、Toast、退出已接线；
  阅读区真实内容（虚拟滚动/编码切换）在 2c-2 完成。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import DropOverlay from './lib/components/DropOverlay.svelte';
  import EmptyState from './lib/components/EmptyState.svelte';
  import MenuBar from './lib/components/MenuBar.svelte';
  import ReaderView from './lib/components/ReaderView.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import TabBar from './lib/components/TabBar.svelte';
  import Toast from './lib/components/Toast.svelte';
  import ToolBar from './lib/components/ToolBar.svelte';
  import { formatBytes } from './lib/format';
  import { describeIpcError, ipc, toIpcError } from './lib/ipc';
  import { tabs } from './lib/state/tabs.svelte';
  import { toasts } from './lib/state/toasts.svelte';
  import type { ResolvedTheme, ThemeChoice } from './lib/types';

  /** 主题选择（默认跟随系统；阶段 8 起由设置加载/保存） */
  let themeChoice = $state<ThemeChoice>('system');
  /** 文件拖拽悬停（控制遮罩显示） */
  let dragging = $state(false);
  /** 应用版本（无文件时状态栏展示） */
  let version = $state('');
  /** 阅读百分比（2c-2 由阅读区回报；先占位） */
  let readPercent = $state(0);

  /** 当前活动标签 */
  const active = $derived(tabs.active);

  /** 主题切换入口 */
  function setTheme(theme: ThemeChoice): void {
    themeChoice = theme;
  }

  /** 打开文件（对话框；菜单/工具栏/空状态共用） */
  function openFile(): void {
    void tabs.openViaDialog();
  }

  /** 退出应用（关闭窗口即退出；未保存拦截在编辑阶段接入） */
  async function quit(): Promise<void> {
    await getCurrentWindow().close();
  }

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

  onMount(() => {
    // 版本号（状态栏无文件时展示；失败不阻塞启动）
    void ipc.getAppInfo().then(
      (info) => {
        version = `v${info.version}`;
      },
      (error: unknown) => toasts.error(describeIpcError(toIpcError(error))),
    );

    // 文件拖拽（Tauri 原生事件：over → 遮罩；drop → 逐个打开）
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === 'over') {
          dragging = true;
        } else if (event.payload.type === 'drop') {
          dragging = false;
          for (const path of event.payload.paths) {
            void tabs.openPath(path);
          }
        } else {
          dragging = false;
        }
      })
      .then((stop) => {
        unlisten = stop;
      });

    return () => {
      unlisten?.();
    };
  });
</script>

<div class="shell">
  <MenuBar {themeChoice} onThemeChange={setTheme} onOpenFile={openFile} onQuit={() => void quit()} />
  <ToolBar
    {themeChoice}
    onThemeChange={setTheme}
    encodingLabel={active?.encoding ?? '—'}
    onOpenFile={openFile}
  />
  <TabBar
    tabs={tabs.tabs}
    activeId={tabs.activeId}
    onSelect={(tabId) => tabs.select(tabId)}
    onClose={(tabId) => void tabs.close(tabId)}
  />
  {#if active}
    <ReaderView />
  {:else}
    <EmptyState onOpen={openFile} />
  {/if}
  <StatusBar
    fileName={active?.name}
    percent={readPercent}
    sizeLabel={active ? formatBytes(active.byteLen) : undefined}
    encodingLabel={active?.encoding}
    {version}
  />
  <Toast />
  <DropOverlay visible={dragging} />
</div>

<style>
  .shell {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
</style>
