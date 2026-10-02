<script lang="ts">
  // 标签栏：由标签存储驱动（选择/关闭已接线；拖拽排序与右键菜单在阶段 5 接线）。
  // 视觉规格（设计 D28）：高 34px；活动标签顶部 2px 强调条 + 阅读区底色；
  // 宽度 90–180px、文本省略；关闭按钮悬停/活动时显现。
  import Icon from './Icon.svelte';
  import type { TabInfo } from '../ipc';

  interface Props {
    /** 标签列表（后端顺序） */
    tabs: TabInfo[];
    /** 活动标签 id */
    activeId: number | null;
    /** 选择标签 */
    onSelect: (tabId: number) => void;
    /** 关闭标签 */
    onClose: (tabId: number) => void;
  }
  let { tabs, activeId, onSelect, onClose }: Props = $props();
</script>

<div class="tab-bar" role="tablist" aria-label="打开的文件">
  {#each tabs as tab (tab.tabId)}
    <div
      class="tab"
      class:active={tab.tabId === activeId}
      role="tab"
      aria-selected={tab.tabId === activeId}
      tabindex={0}
      title={tab.path}
      onclick={() => onSelect(tab.tabId)}
      onkeydown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect(tab.tabId);
        }
      }}
    >
      <span class="name">{tab.name}</span>
      <button
        class="close"
        title="关闭标签（Ctrl+W）"
        aria-label={`关闭 ${tab.name}`}
        onclick={(event) => {
          event.stopPropagation();
          onClose(tab.tabId);
        }}
      >
        <Icon name="close" size={12} />
      </button>
    </div>
  {/each}
</div>

<style>
  .tab-bar {
    display: flex;
    align-items: stretch;
    height: var(--h-tabbar);
    background: var(--chrome);
    border-bottom: 1px solid var(--line);
    user-select: none;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
  }

  .tab-bar::-webkit-scrollbar {
    display: none;
  }

  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 0 0 auto;
    min-width: 90px;
    max-width: 180px;
    padding: 0 8px 0 12px;
    border-right: 1px solid var(--line);
    color: var(--muted);
    font-size: 12.5px;
    cursor: default;
  }

  .tab:hover {
    background: var(--hover);
    color: var(--ink);
  }

  .tab.active {
    background: var(--tab-active);
    color: var(--ink);
  }

  .tab.active::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--accent);
  }

  .name {
    flex: 1;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: inherit;
    opacity: 0;
    cursor: default;
  }

  .tab:hover .close,
  .tab.active .close {
    opacity: 1;
  }

  .close:hover {
    background: var(--hover);
  }
</style>
