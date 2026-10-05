<script lang="ts">
  // 分屏布局渲染：叶子渲染栏位视图（由父级 snippet 提供内容），分支渲染行/列容器与分隔条。
  // 分隔条拖动按容器像素换算百分比权重（相邻子项各保留 MIN_SIZE 下限）。
  import type { Snippet } from 'svelte';

  import { MIN_SIZE, type PaneLayout } from '../layout/pane-tree';
  import PaneTree from './PaneTree.svelte';

  interface Props {
    /** 布局子树（根调用传入完整树） */
    layout: PaneLayout;
    /** 当前激活栏位键（叶子标记 data-pane-active） */
    activePane: string;
    /** 分隔条拖动后的新权重（path 指向所属分支） */
    onSizes: (path: number[], sizes: number[]) => void;
    /** 栏位获得焦点（指针按下） */
    onActivate: (pane: string) => void;
    /** 拖放预览（拖拽任务使用；null = 无） */
    hover?: { pane: string; zone: string } | null;
    /** 栏位内容渲染（参数：栏位键、该栏的拖放预览区） */
    view: Snippet<[string, string | null]>;
    /** 当前节点从根出发的孩子下标路径 */
    path?: number[];
  }
  let { layout, activePane, onSizes, onActivate, hover = null, view, path = [] }: Props =
    $props();

  /** 分隔条拖拽会话 */
  let splitDrag = $state<{
    index: number;
    startX: number;
    startY: number;
    sizes: number[];
    box: { width: number; height: number };
  } | null>(null);
  /** 分支容器（拖拽换算用） */
  let splitBoxEl = $state<HTMLElement | null>(null);

  function startSplit(event: PointerEvent, index: number): void {
    if (layout.type !== 'split' || !splitBoxEl) return;
    event.preventDefault();
    const rect = splitBoxEl.getBoundingClientRect();
    splitDrag = {
      index,
      startX: event.clientX,
      startY: event.clientY,
      sizes: [...layout.sizes],
      box: { width: rect.width, height: rect.height },
    };
    (event.currentTarget as Element).setPointerCapture(event.pointerId);
  }

  function moveSplit(event: PointerEvent): void {
    const current = splitDrag;
    if (!current || layout.type !== 'split') return;
    const horizontal = layout.dir === 'row';
    const extent = horizontal ? current.box.width : current.box.height;
    if (extent <= 0) return;
    const deltaPercent =
      ((horizontal ? event.clientX - current.startX : event.clientY - current.startY) / extent) *
      100;
    const sizes = [...current.sizes];
    const share = sizes[current.index - 1] + sizes[current.index];
    const first = Math.min(
      Math.max(sizes[current.index - 1] + deltaPercent, MIN_SIZE),
      share - MIN_SIZE,
    );
    sizes[current.index - 1] = first;
    sizes[current.index] = share - first;
    onSizes(path, sizes);
  }

  function endSplit(): void {
    splitDrag = null;
  }
</script>

{#if layout.type === 'leaf'}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="pane-cell"
    data-pane={layout.pane}
    data-pane-active={layout.pane === activePane ? 'true' : undefined}
    onpointerdown={() => onActivate(layout.pane)}
  >
    {@render view(layout.pane, hover?.pane === layout.pane ? hover.zone : null)}
    {#if hover?.pane === layout.pane && hover.zone.startsWith('edge-')}
      <div class="drop-half {hover.zone}" aria-hidden="true"></div>
    {/if}
  </div>
{:else}
  <div
    class="pane-split"
    class:row={layout.dir === 'row'}
    class:column={layout.dir === 'column'}
    bind:this={splitBoxEl}
  >
    {#each layout.children as child, index (index)}
      {#if index > 0}
        <div
          class="splitter"
          class:horizontal={layout.dir === 'row'}
          role="separator"
          aria-orientation={layout.dir === 'row' ? 'vertical' : 'horizontal'}
          onpointerdown={(event) => startSplit(event, index)}
          onpointermove={moveSplit}
          onpointerup={endSplit}
          onpointercancel={endSplit}
        ></div>
      {/if}
      <div class="split-cell" style="flex-grow: {layout.sizes[index] ?? 0}">
        <PaneTree
          layout={child}
          {activePane}
          {onSizes}
          {onActivate}
          {hover}
          {view}
          path={[...path, index]}
        />
      </div>
    {/each}
  </div>
{/if}

<style>
  .pane-cell {
    position: relative;
    display: flex;
    flex-direction: column;
    flex: 1 1 0;
    min-width: 0;
    min-height: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .pane-split {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }

  .pane-split.row {
    flex-direction: row;
  }

  .pane-split.column {
    flex-direction: column;
  }

  .split-cell {
    display: flex;
    flex: 1 1 0;
    min-width: 0;
    min-height: 0;
  }

  .splitter {
    flex: 0 0 5px;
    background: var(--line);
    z-index: 2;
    cursor: row-resize;
  }

  .splitter.horizontal {
    cursor: col-resize;
  }

  .splitter:hover {
    background: var(--accent);
  }

  /* 拖放边缘预览：占据目标栏的一半 */
  .drop-half {
    position: absolute;
    z-index: 5;
    pointer-events: none;
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    border: 1px solid var(--accent);
  }

  .drop-half.edge-left {
    left: 0;
    top: 0;
    bottom: 0;
    width: 50%;
  }

  .drop-half.edge-right {
    right: 0;
    top: 0;
    bottom: 0;
    width: 50%;
  }

  .drop-half.edge-top {
    left: 0;
    right: 0;
    top: 0;
    height: 50%;
  }

  .drop-half.edge-bottom {
    left: 0;
    right: 0;
    bottom: 0;
    height: 50%;
  }
</style>
