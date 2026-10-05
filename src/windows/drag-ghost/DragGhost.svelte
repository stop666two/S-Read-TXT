<!--
  DragGhost —— 标签拖拽拖影：跟随光标的标签片（名称 + 颜色点）。
  数据来自 Rust 定向事件 `srt://tab-drag-ghost`；窗口由 Rust 置顶并点击穿透。
-->
<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';

  /** 当前拖拽标签信息（null = 尚未收到） */
  let name = $state('');
  let color = $state<string | null>(null);
  let dark = $state(false);

  /** 色点取色（与主窗调色板浅色档一致；拖影窗口不继承主题变量，故内联） */
  const PALETTE: Record<string, string> = {
    red: '#c94f4f',
    orange: '#d97a1f',
    yellow: '#bfa012',
    green: '#3f8f4f',
    cyan: '#2f8f9f',
    blue: '#4a7dbd',
    purple: '#7a5bbd',
    gray: '#8a8a8a',
  };

  onMount(() => {
    let stop: (() => void) | undefined;
    void listen<{ name: string; color: string | null; dark: boolean }>(
      'srt://tab-drag-ghost',
      (event) => {
        name = event.payload.name;
        color = event.payload.color;
        dark = event.payload.dark;
      },
    ).then((unlisten) => {
      stop = unlisten;
    });
    return () => stop?.();
  });
</script>

<div class="ghost" class:dark>
  {#if color}
    <span class="dot" style="background: {PALETTE[color] ?? 'transparent'}"></span>
  {/if}
  <span class="name">{name}</span>
</div>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    background: transparent;
    overflow: hidden;
  }

  .ghost {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    margin: 2px;
    border-radius: 5px;
    background: #faf9f7;
    color: #2b2b2b;
    border: 1px solid #d8d4cc;
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.22);
    font-family: system-ui, 'Microsoft YaHei', sans-serif;
    font-size: 12.5px;
    white-space: nowrap;
    overflow: hidden;
  }

  .ghost.dark {
    background: #2a2a2a;
    color: #ececec;
    border-color: #4a4a4a;
  }

  .dot {
    flex: 0 0 auto;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
