<script lang="ts">
  // 标签栏：紧凑矩形标签（阶段 2b 为静态示例；拖拽/右键/中键在阶段 5 接线）。
  // 视觉规格（设计 D28）：高 34px；活动标签顶部 2px 强调条 + 内容区底色；
  // 非活动标签透明、悬停浅底色；宽度 90–180px、文本省略；关闭按钮悬停显现。
  import Icon from './Icon.svelte';

  /** 标签展示数据（静态示例） */
  interface TabItem {
    id: number;
    name: string;
    active: boolean;
  }

  let tabs = $state<TabItem[]>([
    { id: 1, name: '示例文本.txt', active: true },
    { id: 2, name: '长文件名示例-第二卷.txt', active: false },
  ]);
</script>

<div class="tab-bar" role="tablist" aria-label="打开的文件">
  {#each tabs as tab (tab.id)}
    <div class="tab" class:active={tab.active} role="tab" aria-selected={tab.active}>
      <span class="name" title={tab.name}>{tab.name}</span>
      <button class="close" title="关闭标签（Ctrl+W）" aria-label={`关闭 ${tab.name}`}>
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
    overflow: hidden;
  }

  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 90px;
    max-width: 180px;
    padding: 0 8px 0 12px;
    border-right: 1px solid var(--line);
    color: var(--muted);
    font-size: 12.5px;
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
