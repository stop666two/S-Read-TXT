<script lang="ts">
  // 空状态：未打开任何文件时的占位引导（打开按钮 + 拖拽与快捷键提示）。
  import Icon from './Icon.svelte';

  interface Props {
    /** 触发打开文件（接系统对话框） */
    onOpen: () => void;
  }
  let { onOpen }: Props = $props();
</script>

<div class="empty">
  <Icon name="open" size={40} />
  <h1>未打开任何文件</h1>
  <p>拖拽 TXT 文件到窗口，或点击下方按钮打开</p>
  <button class="open-btn" onclick={onOpen}>打开文件</button>
  <p class="hint">快捷键 Ctrl+O · 支持同时打开多个文件</p>
</div>

<style>
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    background: var(--base);
    color: var(--muted);
    user-select: none;
  }

  .empty h1 {
    margin: 6px 0 0;
    color: var(--ink);
    font-size: 16px;
    font-weight: 500;
  }

  .empty p {
    margin: 0;
    font-size: 12.5px;
  }

  .open-btn {
    margin: 8px 0 2px;
    padding: 6px 22px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: transparent;
    color: var(--accent);
    font: inherit;
    font-size: 13px;
    cursor: default;
  }

  .open-btn:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .hint {
    color: var(--muted);
    opacity: 0.85;
  }
</style>
