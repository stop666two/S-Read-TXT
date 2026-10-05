<script lang="ts">
  // 双栏比较视图：并排 / 统一两种呈现；虚拟列表按需向后端取行文本。
  import { untrack } from 'svelte';
  import { ipc, type DiffDocs } from '../../lib/ipc';
  import {
    buildSideModel,
    buildUnifiedModel,
    mergeRanges,
    nextHunkRow,
    type AlignRow,
    type UnifiedRow,
  } from '../../lib/compare/line-model';

  interface Props {
    diff: DiffDocs;
    view: 'side' | 'unified';
    /** 导航条注册：暴露「上一处/下一处」动作给宿主工具条。 */
    onNavReady?: (nav: { prev: () => void; next: () => void }) => void;
  }

  let { diff, view, onNavReady }: Props = $props();

  const ROW_H = 18;
  const OVERSCAN = 12;

  const sideModel = $derived(buildSideModel(diff.hunks));
  const unifiedModel = $derived(buildUnifiedModel(diff.hunks));

  let scrollEl = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(600);
  let linesLeft = $state<Map<number, string>>(new Map());
  let linesRight = $state<Map<number, string>>(new Map());
  let fetching = $state(false);

  const totalRows = $derived(
    view === 'side' ? sideModel.rows.length : unifiedModel.rows.length,
  );

  const visibleStart = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN));
  const visibleEnd = $derived(
    Math.min(totalRows, Math.ceil((scrollTop + viewportH) / ROW_H) + OVERSCAN),
  );

  function linesFor(side: 'left' | 'right'): Map<number, string> {
    return side === 'left' ? linesLeft : linesRight;
  }

  /** 收集可见区间需要的一侧行号并合并为连续区间。 */
  function neededRanges(side: 'left' | 'right'): Array<[number, number]> {
    const ranges: Array<[number, number]> = [];
    if (view === 'side') {
      for (let i = visibleStart; i < visibleEnd; i += 1) {
        const row: AlignRow | undefined = sideModel.rows[i];
        const ln = side === 'left' ? row?.left : row?.right;
        if (ln != null && !linesFor(side).has(ln)) ranges.push([ln, ln + 1]);
      }
    } else {
      for (let i = visibleStart; i < visibleEnd; i += 1) {
        const row: UnifiedRow | undefined = unifiedModel.rows[i];
        if (!row) continue;
        const rowSide = row.side;
        const label = row.kind === 'equal' ? 'left' : rowSide;
        if (label !== side) continue;
        if (!linesFor(side).has(row.row)) ranges.push([row.row, row.row + 1]);
      }
    }
    return mergeRanges(ranges);
  }

  $effect(() => {
    // 依赖：可见窗口 / 视图 / 文档 / 缓存版本（读写缓存 Map 触发重跑）
    void visibleStart;
    void visibleEnd;
    void view;
    void diff;
    if (!scrollEl || fetching) return;
    const tasks: Array<{ side: 'left' | 'right'; start: number; count: number }> = [];
    for (const side of ['left', 'right'] as const) {
      for (const [start, end] of neededRanges(side)) {
        tasks.push({ side, start, count: Math.min(end - start, 4096) });
      }
    }
    if (tasks.length === 0) return;
    fetching = true;
    void (async () => {
      try {
        for (const task of tasks) {
          const rows = await ipc.compareRows(task.side, task.start, task.count);
          const map = new Map(linesFor(task.side));
          rows.forEach((text, index) => map.set(task.start + index, text));
          if (task.side === 'left') linesLeft = map;
          else linesRight = map;
        }
      } catch {
        // 拉取失败：保持空白格；滚动或文档变化会自然重试
      } finally {
        fetching = false;
      }
    })();
  });

  function onScroll(event: Event): void {
    const el = event.currentTarget as HTMLElement;
    scrollTop = el.scrollTop;
  }

  function nav(forward: boolean): void {
    const model = view === 'side' ? sideModel : unifiedModel;
    const from = Math.floor(scrollTop / ROW_H);
    const target = nextHunkRow(model, from, forward);
    if (target == null || !scrollEl) return;
    scrollEl.scrollTop = target * ROW_H;
    scrollTop = scrollEl.scrollTop;
  }

  untrack(() => onNavReady)?.({ prev: () => nav(false), next: () => nav(true) });

  $effect(() => {
    if (!scrollEl || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => {
      viewportH = scrollEl?.clientHeight ?? 600;
    });
    observer.observe(scrollEl);
    viewportH = scrollEl.clientHeight;
    return () => observer.disconnect();
  });

  function textFor(side: 'left' | 'right', line: number | null): string {
    if (line == null) return '';
    return linesFor(side).get(line) ?? '';
  }
</script>

<div class="diff" bind:this={scrollEl} onscroll={onScroll} data-diff-scroll>
  <div class="spacer" style="height: {totalRows * ROW_H}px">
    {#if view === 'side'}
      {#each Array.from({ length: Math.max(0, visibleEnd - visibleStart) }, (_, k) => visibleStart + k) as index (index)}
        {@const row = sideModel.rows[index]}
        {#if row}
          <div class="row side" style="top: {index * ROW_H}px" data-diff-row data-kind={row.kind}>
            <div class="cell left" data-side="left" data-line={row.left ?? ''}>
              <span class="ln">{row.left == null ? '' : row.left + 1}</span>
              <span class="code">{textFor('left', row.left)}</span>
            </div>
            <div class="cell right" data-side="right" data-line={row.right ?? ''}>
              <span class="ln">{row.right == null ? '' : row.right + 1}</span>
              <span class="code">{textFor('right', row.right)}</span>
            </div>
          </div>
        {/if}
      {/each}
    {:else}
      {#each Array.from({ length: Math.max(0, visibleEnd - visibleStart) }, (_, k) => visibleStart + k) as index (index)}
        {@const row = unifiedModel.rows[index]}
        {#if row}
          <div class="row unified" style="top: {index * ROW_H}px" data-diff-row data-kind={row.kind}>
            <div class="cell wide" data-side={row.side} data-line={row.row}>
              <span class="ln">{row.row + 1}</span>
              <span class="sign">{row.kind === 'delete' ? '-' : row.kind === 'insert' ? '+' : ' '}</span>
              <span class="code">{textFor(row.side, row.row)}</span>
            </div>
          </div>
        {/if}
      {/each}
    {/if}
  </div>
</div>

<style>
  .diff {
    position: relative;
    height: 100%;
    overflow: auto;
    font-family: var(--font-mono, monospace);
    font-size: 12.5px;
    background: var(--bg, #faf9f7);
    color: var(--fg, #1f2328);
  }
  .spacer {
    position: relative;
    min-width: 100%;
  }
  .row {
    position: absolute;
    left: 0;
    right: 0;
    height: 18px;
    display: flex;
    line-height: 18px;
  }
  .row[data-kind='change'],
  .row[data-kind='delete'] .cell.left,
  .row.unified[data-kind='delete'] {
    background: color-mix(in srgb, #f85149 16%, transparent);
  }
  .row[data-kind='change'] .cell.right,
  .row.unified[data-kind='insert'] {
    background: color-mix(in srgb, #2ea043 18%, transparent);
  }
  .row[data-kind='insert'] .cell.right {
    background: color-mix(in srgb, #2ea043 18%, transparent);
  }
  .row[data-kind='change'] .cell.left {
    background: color-mix(in srgb, #f85149 16%, transparent);
  }
  .cell {
    display: flex;
    min-width: 0;
    overflow: hidden;
  }
  .cell.left {
    width: 50%;
    border-right: 1px solid var(--border, #d8d5d0);
  }
  .cell.right {
    width: 50%;
  }
  .cell.wide {
    width: 100%;
  }
  .ln {
    flex: 0 0 52px;
    text-align: right;
    padding-right: 8px;
    color: var(--fg-faint, #8a8f98);
    user-select: none;
    font-variant-numeric: tabular-nums;
  }
  .sign {
    flex: 0 0 14px;
    text-align: center;
    user-select: none;
  }
  .code {
    flex: 1;
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
    padding-right: 8px;
  }
</style>
