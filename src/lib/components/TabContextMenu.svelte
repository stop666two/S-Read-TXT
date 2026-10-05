<!--
  TabContextMenu —— 标签右键菜单（关闭 / 关闭其他 / 关闭全部 / 颜色）。
  定位：fixed 于光标位置；点击空白、选择动作或 Esc 关闭。
  可访问性：role=menu + menuitem；打开时聚焦第一项，支持方向键/Enter/Esc。
-->
<script lang="ts">
  import { t } from '../i18n/index.svelte';

  interface Props {
    /** 光标位置（视口坐标） */
    x: number;
    y: number;
    /** 当前标签颜色（调色板 id；null = 未设置） */
    color: string | null;
    /** 关闭当前标签 */
    onClose: () => void;
    /** 关闭其他标签（保留当前） */
    onCloseOthers: () => void;
    /** 关闭全部标签 */
    onCloseAll: () => void;
    /** 设置标签颜色（null = 清除） */
    onSetColor: (color: string | null) => void;
    /** 请求关闭菜单（点击外部 / Esc / 执行动作后） */
    onDismiss: () => void;
  }

  let { x, y, color, onClose, onCloseOthers, onCloseAll, onSetColor, onDismiss }: Props = $props();

  /** 调色板 id（与 Rust app_state::TAB_COLORS 同步） */
  const TAB_COLORS = ['red', 'orange', 'yellow', 'green', 'cyan', 'blue', 'purple', 'gray'] as const;

  /** 菜单根（用于外部点击判定与初始聚焦） */
  let root = $state<HTMLElement | null>(null);

  /** 执行动作后收起菜单 */
  function run(action: () => void): void {
    action();
    onDismiss();
  }

  /** 键盘导航：方向键移动焦点，Esc 关闭 */
  function onKeydown(event: KeyboardEvent): void {
    const buttons = [
      ...(root?.querySelectorAll<HTMLButtonElement>(
        'button[role="menuitem"], button[role="menuitemradio"]',
      ) ?? []),
    ];
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === 'Escape') {
      event.preventDefault();
      onDismiss();
      return;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const delta = event.key === 'ArrowDown' ? 1 : -1;
      const next = buttons[(index + delta + buttons.length) % buttons.length];
      next?.focus();
      return;
    }
    if (event.key === 'Enter' || event.key === ' ') {
      if (index >= 0) {
        event.preventDefault();
        buttons[index].click();
      }
    }
  }

  /** action：挂载后聚焦第一项（键盘可达）。 */
  function focusFirst(node: HTMLElement): { destroy(): void } {
    requestAnimationFrame(() => {
      node.querySelector<HTMLButtonElement>('button[role="menuitem"]')?.focus();
    });
    return { destroy() {} };
  }
</script>

<svelte:window
  onmousedown={(event) => {
    if (root && !root.contains(event.target as Node)) onDismiss();
  }}
  onresize={() => onDismiss()}
  onblur={() => onDismiss()}
/>

<div
  class="tab-menu"
  role="menu"
  aria-label={t('tabMenu.aria')}
  tabindex="-1"
  bind:this={root}
  style="left: {x}px; top: {y}px;"
  onkeydown={onKeydown}
  use:focusFirst
>
  <button class="item" role="menuitem" onclick={() => run(onClose)}>{t('tabMenu.close')}</button>
  <button class="item" role="menuitem" onclick={() => run(onCloseOthers)}>{t('tabMenu.closeOthers')}</button>
  <button class="item" role="menuitem" onclick={() => run(onCloseAll)}>{t('tabMenu.closeAll')}</button>
  <div class="divider" role="separator"></div>
  <div class="group" role="group" aria-label={t('tabMenu.color')}>
    <div class="group-label">{t('tabMenu.color')}</div>
    <div class="swatches">
      {#each TAB_COLORS as value (value)}
        <button
          class="swatch"
          class:current={color === value}
          data-tab-color={value}
          role="menuitemradio"
          aria-label={t(`tabMenu.color.${value}`)}
          aria-checked={color === value}
          onclick={() => run(() => onSetColor(value))}
        >
          <span class="dot" style="background: var(--tab-color-{value})"></span>
        </button>
      {/each}
    </div>
    <button
      class="item clear"
      role="menuitem"
      data-tab-color-clear
      onclick={() => run(() => onSetColor(null))}
    >
      {t('tabMenu.colorClear')}
    </button>
  </div>
</div>

<style>
  .tab-menu {
    position: fixed;
    z-index: 50;
    min-width: 140px;
    padding: 4px;
    background: var(--surface);
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: 6px;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.16);
    display: flex;
    flex-direction: column;
  }

  .item {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: inherit;
    font-size: 13px;
    text-align: left;
    cursor: default;
  }

  .item:hover,
  .item:focus-visible {
    background: var(--hover);
    outline: none;
  }

  .divider {
    height: 1px;
    margin: 4px 6px;
    background: var(--line);
  }

  .group {
    display: flex;
    flex-direction: column;
  }

  .group-label {
    padding: 4px 10px 2px;
    color: var(--muted);
    font-size: 12px;
  }

  .swatches {
    display: flex;
    gap: 4px;
    padding: 2px 8px 4px;
  }

  .swatch {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    border-radius: 4px;
    background: transparent;
    cursor: default;
    padding: 0;
  }

  .swatch:hover,
  .swatch:focus-visible {
    background: var(--hover);
    outline: none;
  }

  .swatch.current {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }

  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }

  .item.clear {
    font-size: 12.5px;
    color: var(--muted);
  }
</style>
