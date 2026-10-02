<!--
  TitleBar — 自定义标题栏（替代原生窗口装饰，随主题令牌配色）。
  组成：应用图标 + 窗口标题 + 拖拽区 + 最小化/最大化(还原)/关闭。
  行为：拖拽移动窗口（data-tauri-drag-region）；双击拖拽区切换最大化；
        最大化状态实时驱动中间按钮图标；关闭复用主窗口关闭链路
        （App 的 onCloseRequested 拦截负责脏标签三态确认）。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import appIcon from '../../assets/app-icon.png';

  interface Props {
    /** 窗口标题（App 计算：文件名 + 应用名） */
    title: string;
  }
  let { title }: Props = $props();

  const appWindow = getCurrentWindow();
  /** 当前是否最大化（驱动中间按钮图标与提示文案） */
  let maximized = $state(false);

  onMount(() => {
    void appWindow.isMaximized().then((value) => (maximized = value));
    let stop: (() => void) | undefined;
    void appWindow
      .onResized(() => {
        void appWindow.isMaximized().then((value) => (maximized = value));
      })
      .then((unlisten) => {
        stop = unlisten;
      });
    return () => stop?.();
  });

  function minimize(): void {
    void appWindow.minimize();
  }

  function toggleMaximize(): void {
    void appWindow.toggleMaximize();
  }

  function close(): void {
    void appWindow.close();
  }
</script>

<header class="title-bar">
  <img class="app-icon" src={appIcon} alt="" draggable="false" />
  <span class="app-title">{title}</span>
  <!-- 双击拖拽区最大化是桌面窗口惯例（鼠标手势）；键盘用户可用右侧按钮，功能等价 -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="drag-region" data-tauri-drag-region ondblclick={toggleMaximize} role="presentation"
  ></div>
  <div class="controls">
    <button class="ctl" type="button" aria-label="最小化" title="最小化" onclick={minimize}>
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
        <path d="M0 5h10" stroke="currentColor" stroke-width="1" />
      </svg>
    </button>
    <button
      class="ctl"
      type="button"
      aria-label={maximized ? '还原' : '最大化'}
      title={maximized ? '还原' : '最大化'}
      onclick={toggleMaximize}
    >
      {#if maximized}
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M3.5 0.5h6v6" fill="none" stroke="currentColor" stroke-width="1" />
          <rect x="0.5" y="3.5" width="6" height="6" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
      {:else}
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1" />
        </svg>
      {/if}
    </button>
    <button class="ctl close" type="button" aria-label="关闭" title="关闭" onclick={close}>
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
        <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" stroke="currentColor" stroke-width="1" />
      </svg>
    </button>
  </div>
</header>

<style>
  .title-bar {
    display: flex;
    align-items: center;
    height: var(--h-titlebar);
    background: var(--chrome);
    border-bottom: 1px solid var(--line);
    user-select: none;
  }

  .app-icon {
    width: 16px;
    height: 16px;
    margin: 0 8px 0 10px;
  }

  .app-title {
    color: var(--ink);
    font-size: 12.5px;
    white-space: nowrap;
  }

  .drag-region {
    flex: 1;
    align-self: stretch;
  }

  .controls {
    display: flex;
    align-self: stretch;
  }

  .ctl {
    width: 46px;
    border: none;
    background: transparent;
    color: var(--muted);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: default;
  }

  .ctl:hover {
    background: var(--hover);
    color: var(--ink);
  }

  .ctl.close:hover {
    background: #e81123;
    color: #ffffff;
  }
</style>
