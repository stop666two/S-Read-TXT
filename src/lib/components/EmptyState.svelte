<script lang="ts">
  // 空状态：未打开任何文件时的占位引导（打开按钮 + 拖拽与快捷键提示）。
  import { t } from '../i18n/index.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    /** 触发打开文件（接系统对话框） */
    onOpen: () => void;
  }
  let { onOpen }: Props = $props();
</script>

<div class="empty">
  <Icon name="open" size={40} />
  <h1>{t('empty.title')}</h1>
  <p>{t('empty.hint')}</p>
  <button class="open-btn" onclick={onOpen}>{t('empty.open')}</button>
  <p class="hint">{t('empty.shortcutHint')}</p>
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
    margin: 10px 0 2px;
    padding: 7px 24px;
    border: 1px solid var(--accent);
    border-radius: 8px;
    background: transparent;
    color: var(--accent);
    font: inherit;
    font-size: 13px;
    cursor: default;
    transition:
      background-color 90ms ease,
      color 90ms ease;
  }

  .open-btn:hover {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }

  .open-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .hint {
    color: var(--muted);
    opacity: 0.85;
  }
</style>
