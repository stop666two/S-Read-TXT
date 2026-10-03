<!--
  TabContextMenu —— 标签右键菜单（关闭 / 关闭其他 / 关闭全部）。
  定位：fixed 于光标位置；点击空白、选择动作或 Esc 关闭。
  可访问性：role=menu + menuitem；打开时聚焦第一项，支持方向键/Enter/Esc。
-->
<script lang="ts">
  import { t } from '../i18n/index.svelte';

  interface Props {
    /** 光标位置（视口坐标） */
    x: number;
    y: number;
    /** 关闭当前标签 */
    onClose: () => void;
    /** 关闭其他标签（保留当前） */
    onCloseOthers: () => void;
    /** 关闭全部标签 */
    onCloseAll: () => void;
    /** 请求关闭菜单（点击外部 / Esc / 执行动作后） */
    onDismiss: () => void;
  }

  let { x, y, onClose, onCloseOthers, onCloseAll, onDismiss }: Props = $props();

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
      ...(root?.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]') ?? []),
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
</style>
