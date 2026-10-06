<!--
  CommandPalette — 命令面板（Ctrl+Shift+P）。
  能力：空查询列出全部命令，输入即时过滤；↑↓ 导航、Enter 执行、Esc 关闭；
       按分组展示，右侧显示快捷键提示；不可用命令灰显并给出原因。
  契约：data-command-palette / data-command-input / data-command-item[data-command-id]。
-->
<script lang="ts">
  import { t } from '../i18n/index.svelte';
  import type { MessageKey } from '../i18n/zh-CN';
  import { filterCommands, nextIndex, type PaletteCommand } from '../commands/palette';

  interface Props {
    /** 是否打开 */
    open: boolean;
    /** 命令表（由 App 构建，闭包携带处理器） */
    commands: readonly PaletteCommand[];
    /** 执行命令（按 id；调用方负责关闭面板） */
    onRun: (id: string) => void;
    /** 关闭面板 */
    onClose: () => void;
  }

  let { open, commands, onRun, onClose }: Props = $props();

  let query = $state('');
  let index = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);

  const filtered = $derived(filterCommands(commands, query, t));

  /** 选中下标（随过滤收敛，空结果 -1） */
  const activeIndex = $derived(filtered.length === 0 ? -1 : Math.min(index, filtered.length - 1));

  /** 按分组聚合（保持过滤顺序；flat 为全局下标用于高亮） */
  const groups = $derived.by(() => {
    const out: { key: MessageKey; items: { command: PaletteCommand; flat: number }[] }[] = [];
    filtered.forEach((command, flat) => {
      const last = out[out.length - 1];
      if (last && last.key === command.groupKey) {
        last.items.push({ command, flat });
      } else {
        out.push({ key: command.groupKey, items: [{ command, flat }] });
      }
    });
    return out;
  });

  // 打开时重置查询与选中并聚焦输入框
  $effect(() => {
    if (open) {
      query = '';
      index = 0;
      queueMicrotask(() => inputEl?.focus());
    }
  });

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      index = nextIndex(activeIndex, filtered.length, 1);
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      index = nextIndex(activeIndex, filtered.length, -1);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      const command = filtered[activeIndex];
      if (command?.enabled) onRun(command.id);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
    }
  }
</script>

{#if open}
  <div
    class="palette-overlay"
    data-command-palette
    role="presentation"
    onmousedown={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}
  >
    <div class="palette" role="dialog" aria-modal="true" aria-label={t('palette.aria')}>
      <input
        class="palette-input"
        data-command-input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={onKeydown}
        placeholder={t('palette.placeholder')}
        aria-label={t('palette.placeholder')}
        role="combobox"
        aria-expanded="true"
        aria-controls="command-palette-list"
        autocomplete="off"
        spellcheck="false"
      />
      <div class="palette-list" id="command-palette-list" role="listbox" aria-label={t('palette.aria')}>
        {#if filtered.length === 0}
          <div class="palette-empty">{t('palette.empty')}</div>
        {/if}
        {#each groups as group (group.key)}
          <div class="palette-group">{t(group.key)}</div>
          {#each group.items as item (item.command.id)}
            <button
              type="button"
              class="palette-item"
              class:active={item.flat === activeIndex}
              data-command-item={item.command.id}
              role="option"
              aria-selected={item.flat === activeIndex}
              disabled={!item.command.enabled}
              onclick={() => {
                if (item.command.enabled) onRun(item.command.id);
              }}
              onmousemove={() => (index = item.flat)}
            >
              <span class="palette-title">{t(item.command.titleKey)}</span>
              {#if !item.command.enabled && item.command.disabledReasonKey}
                <span class="palette-reason">{t(item.command.disabledReasonKey)}</span>
              {:else if item.command.shortcutHint}
                <kbd class="palette-kbd">{item.command.shortcutHint}</kbd>
              {/if}
            </button>
          {/each}
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .palette-overlay {
    position: fixed;
    inset: 0;
    z-index: 300;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    background: color-mix(in srgb, var(--ink) 22%, transparent);
  }

  .palette {
    width: 560px;
    max-width: calc(100vw - 48px);
    max-height: 62vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
  }

  .palette-input {
    padding: 11px 14px;
    border: none;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
    color: var(--ink);
    font-size: 13.5px;
    outline: none;
  }

  .palette-input::placeholder {
    color: var(--muted);
  }

  .palette-list {
    overflow-y: auto;
    padding: 6px 0;
  }

  .palette-group {
    padding: 6px 14px 3px;
    color: var(--muted);
    font-size: 11px;
  }

  .palette-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 14px;
    border: none;
    background: transparent;
    color: var(--ink);
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }

  .palette-item.active {
    background: var(--hover);
  }

  .palette-item:disabled {
    color: var(--muted);
    cursor: default;
  }

  .palette-title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .palette-reason {
    color: var(--muted);
    font-size: 11.5px;
  }

  .palette-kbd {
    padding: 1px 6px;
    border: 1px solid var(--line);
    border-radius: 4px;
    color: var(--muted);
    font-size: 11px;
    font-family: inherit;
  }

  .palette-empty {
    padding: 18px 14px;
    color: var(--muted);
    font-size: 12.5px;
    text-align: center;
  }
</style>
