<!--
  TitleBar — 自定义标题栏（替代原生窗口装饰，随主题令牌配色）。
  组成：应用图标 + 窗口标题 + 拖拽区 + 最小化/最大化(还原)/关闭。
  行为：拖拽移动窗口（data-tauri-drag-region）；双击拖拽区切换最大化；
        最大化状态实时驱动中间按钮图标；关闭复用主窗口关闭链路
        （App 的 onCloseRequested 拦截负责脏标签三态确认）。
  视觉（样板重绘）：38px 高、16 分辨率线性图标、悬停/按下态、关闭键危险红、
        焦点环（键盘可达性）；不设底部分隔线，与工具栏无缝融合。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import appIcon from '../../assets/app-icon.png';
  import Icon from './Icon.svelte';

  interface Props {
    /** 窗口标题（App 计算：文件名 + 应用名） */
    title: string;
    /** 是否显示最大化/还原按钮（设置窗口等不可最大化窗口传 false） */
    showMaximize?: boolean;
    /** 是否显示设置按钮（主窗口显示；设置窗口自身不显示） */
    showSettings?: boolean;
    /** 设置按钮回调（打开独立设置窗口） */
    onSettings?: () => void;
  }
  let { title, showMaximize = true, showSettings = false, onSettings }: Props = $props();

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
  <div class="drag-region" data-tauri-drag-region ondblclick={() => showMaximize && toggleMaximize()} role="presentation"
  ></div>
  <div class="controls">
    {#if showSettings}
      <button class="ctl" type="button" aria-label="打开设置" title="打开设置" onclick={() => onSettings?.()}>
        <Icon name="settings" />
      </button>
    {/if}
    <button class="ctl" type="button" aria-label="最小化" title="最小化" onclick={minimize}>
      <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
        <path d="M4 8h8" fill="none" stroke="currentColor" stroke-width="1.1" stroke-linecap="round" />
      </svg>
    </button>
    {#if showMaximize}
      <button
        class="ctl"
        type="button"
        aria-label={maximized ? '还原' : '最大化'}
        title={maximized ? '还原' : '最大化'}
        onclick={toggleMaximize}
      >
        {#if maximized}
          <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
            <path
              d="M6 3.5h6.5V10M3.5 6H10v6.5H3.5z"
              fill="none"
              stroke="currentColor"
              stroke-width="1.1"
              stroke-linejoin="round"
            />
          </svg>
        {:else}
          <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
            <rect
              x="3.5"
              y="3.5"
              width="9"
              height="9"
              rx="1"
              fill="none"
              stroke="currentColor"
              stroke-width="1.1"
            />
          </svg>
        {/if}
      </button>
    {/if}
    <button class="ctl close" type="button" aria-label="关闭" title="关闭" onclick={close}>
      <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4.5 4.5l7 7M11.5 4.5l-7 7"
          fill="none"
          stroke="currentColor"
          stroke-width="1.1"
          stroke-linecap="round"
        />
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
    user-select: none;
  }

  .app-icon {
    width: 18px;
    height: 18px;
    margin: 0 9px 0 12px;
  }

  .app-title {
    color: var(--ink);
    font-size: 13px;
    font-weight: 500;
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
    transition:
      background-color 80ms ease,
      color 80ms ease;
  }

  .ctl:hover {
    background: var(--hover);
    color: var(--ink);
  }

  .ctl:active {
    background: color-mix(in srgb, var(--hover) 72%, var(--line));
  }

  .ctl:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .ctl.close:hover {
    background: #e81123;
    color: #ffffff;
  }

  .ctl.close:active {
    background: #c50f1f;
  }
</style>
