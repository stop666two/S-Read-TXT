<script lang="ts">
  // 三方合并视图：左侧冲突列表（逐块选择 我方/他方/双方/不用），
  // 右侧为合并输出预览（虚拟列表；冲突行按当前选择展开并着色）。
  import { ipc, toIpcError, type MergeDocs } from '../../lib/ipc';
  import { t } from '../../lib/i18n/index.svelte';
  import { buildOutputModel, countConflicts, type MergeChoice, type OutputRow } from '../../lib/compare/merge-model';

  interface Props {
    merge: MergeDocs;
  }

  let { merge }: Props = $props();

  const ROW_H = 18;
  const OVERSCAN = 12;

  const conflicts = $derived(merge.regions.filter((region) => region.kind === 'conflict'));
  const choices = $state<MergeChoice[]>([]);
  const touched = $state<boolean[]>([]);

  // 初始化/重建冲突选择（沿用旧选择，缺省我方）
  $effect(() => {
    if (choices.length !== conflicts.length) {
      const next: MergeChoice[] = conflicts.map((_, index) => choices[index] ?? 'ours');
      choices.length = 0;
      choices.push(...next);
    }
    if (touched.length !== conflicts.length) {
      const next = conflicts.map((_, index) => touched[index] ?? false);
      touched.length = 0;
      touched.push(...next);
    }
  });

  const model = $derived(buildOutputModel(merge.regions, choices));
  const totalConflicts = countConflicts(merge.regions);

  let outputEl = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(600);
  let cache = $state<Map<string, Map<number, string>>>(new Map());
  let fetching = $state(false);

  const visibleStart = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN));
  const visibleEnd = $derived(
    Math.min(model.rows.length, Math.ceil((scrollTop + viewportH) / ROW_H) + OVERSCAN),
  );

  function textFor(source: OutputRow['source'], row: number): string {
    return cache.get(source)?.get(row) ?? '';
  }

  function collectRanges(): Array<{ source: OutputRow['source']; start: number; count: number }> {
    const ranges: Record<string, Array<[number, number]>> = { base: [], ours: [], theirs: [] };
    for (let i = visibleStart; i < visibleEnd; i += 1) {
      const row = model.rows[i];
      if (!row) continue;
      if (cache.get(row.source)?.has(row.row)) continue;
      ranges[row.source].push([row.row, row.row + 1]);
    }
    const tasks: Array<{ source: OutputRow['source']; start: number; count: number }> = [];
    for (const source of ['base', 'ours', 'theirs'] as const) {
      const list = ranges[source].sort((a, b) => a[0] - b[0]);
      for (const [start, end] of list) tasks.push({ source, start, count: Math.min(end - start, 2048) });
    }
    return tasks;
  }

  $effect(() => {
    void visibleStart;
    void visibleEnd;
    void merge;
    void cache;
    if (!outputEl || fetching) return;
    const tasks = collectRanges();
    if (tasks.length === 0) return;
    fetching = true;
    void (async () => {
      try {
        for (const task of tasks) {
          const rows = await ipc.mergeRows(task.source, task.start, task.count);
          const next = new Map(cache);
          const bucket = new Map(next.get(task.source) ?? []);
          rows.forEach((text, index) => bucket.set(task.start + index, text));
          next.set(task.source, bucket);
          cache = next;
        }
      } catch {
        // 拉取失败：保持空白；滚动后自然重试
      } finally {
        fetching = false;
      }
    })();
  });

  $effect(() => {
    if (!outputEl || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => {
      viewportH = outputEl?.clientHeight ?? 600;
    });
    observer.observe(outputEl);
    viewportH = outputEl.clientHeight;
    return () => observer.disconnect();
  });

  function onScroll(event: Event): void {
    scrollTop = (event.currentTarget as HTMLElement).scrollTop;
  }

  function setChoice(index: number, choice: MergeChoice): void {
    choices[index] = choice;
    touched[index] = true;
  }

  function scrollToConflict(index: number): void {
    const target = model.conflictStarts[index];
    if (target == null || !outputEl) return;
    outputEl.scrollTop = target * ROW_H;
    scrollTop = outputEl.scrollTop;
  }

  // ---- 写回（任务 13）----
  let writeTarget = $state('');
  let makeBackup = $state(true);
  let writeStatus = $state('');
  let writeBusy = $state(false);

  $effect(() => {
    if (!writeTarget && merge.oursPath) writeTarget = merge.oursPath;
  });

  async function writeBack(): Promise<void> {
    if (writeBusy) return;
    writeBusy = true;
    writeStatus = '';
    try {
      const payload = choices
        .map((choice, index) => ({ regionIndex: index, choice }))
        .filter((_, index) => touched[index] === true);
      const result = await ipc.writeMergeOutput(writeTarget, payload, makeBackup);
      writeStatus = result.backup
        ? t('compare.writeOk', { lines: result.lines, backup: result.backup })
        : t('compare.writeOkNoBackup', { lines: result.lines });
    } catch (error) {
      writeStatus = t('compare.writeFailed', { reason: toIpcError(error).message });
    } finally {
      writeBusy = false;
    }
  }

  async function undoWrite(): Promise<void> {
    if (writeBusy) return;
    writeBusy = true;
    try {
      const restored = await ipc.undoMergeWriteback(writeTarget);
      writeStatus = restored ? t('compare.undoOk') : t('compare.undoNone');
    } catch (error) {
      writeStatus = t('compare.writeFailed', { reason: toIpcError(error).message });
    } finally {
      writeBusy = false;
    }
  }
</script>

<div class="merge" data-merge-view>
  <div class="writebar">
    <label class="field">
      <span>{t('compare.writeTarget')}</span>
      <input type="text" bind:value={writeTarget} spellcheck="false" data-merge-target />
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={makeBackup} data-merge-backup />
      <span>{t('compare.makeBackup')}</span>
    </label>
    <button type="button" class="primary" disabled={writeBusy || !writeTarget} data-merge-write onclick={() => void writeBack()}>
      {t('compare.writeBack')}
    </button>
    <button type="button" disabled={writeBusy} data-merge-undo onclick={() => void undoWrite()}>
      {t('compare.undoWrite')}
    </button>
    {#if writeStatus}
      <span class="status" data-merge-write-status>{writeStatus}</span>
    {/if}
  </div>
  <div class="split">
    <aside class="conflicts" aria-label={t('compare.conflictList')}>
    <div class="head">
      {#if totalConflicts === 0}
        <span class="ok" data-merge-none>{t('compare.noConflict')}</span>
      {:else}
        <span data-merge-count>{t('compare.conflicts', { count: totalConflicts })}</span>
      {/if}
    </div>
    {#each conflicts as conflict, index (index)}
      <div class="card" data-merge-conflict-item data-conflict-index={index}>
        <button
          type="button"
          class="jump"
          data-merge-jump={index}
          onclick={() => scrollToConflict(index)}
        >
          #{index + 1} · {t('compare.baseRange', { from: conflict.baseRange[0] + 1, to: conflict.baseRange[1] })}
        </button>
        <div class="choices" role="radiogroup" aria-label={t('compare.conflictChoice')}>
          {#each [
            { id: 'ours', label: t('compare.mine') },
            { id: 'theirs', label: t('compare.theirs') },
            { id: 'both', label: t('compare.both') },
            { id: 'none', label: t('compare.none') },
          ] as option (option.id)}
            <button
              type="button"
              role="radio"
              aria-checked={choices[index] === option.id}
              class:active={choices[index] === option.id}
              data-merge-choice={option.id}
              data-conflict-index={index}
              onclick={() => setChoice(index, option.id as MergeChoice)}
            >
              {option.label}
            </button>
          {/each}
        </div>
        {#if touched[index] === false}
          <div class="hint">{t('compare.defaultMine')}</div>
        {/if}
      </div>
    {/each}
  </aside>
  <div class="output" bind:this={outputEl} onscroll={onScroll} data-merge-output>
    <div class="spacer" style="height: {model.rows.length * ROW_H}px">
      {#each Array.from({ length: Math.max(0, visibleEnd - visibleStart) }, (_, k) => visibleStart + k) as index (index)}
        {@const row = model.rows[index]}
        {#if row}
          <div
            class="row"
            class:conflict={row.conflict != null}
            class:first={row.conflict != null && model.conflictStarts[row.conflict] === index}
            style="top: {index * ROW_H}px"
            data-merge-out-row
            data-source={row.source}
          >
            <span class="ln">{row.row + 1}</span>
            <span class="src" aria-hidden="true">{row.source === 'base' ? 'B' : row.source === 'ours' ? '我' : '他'}</span>
            <span class="mark">{row.conflict != null && model.conflictStarts[row.conflict] === index ? '⚠' : ''}</span>
            <span class="code">{textFor(row.source, row.row)}</span>
          </div>
        {/if}
      {/each}
    </div>
  </div>
  </div>
</div>

<style>
  .merge {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg, #faf9f7);
    color: var(--fg, #1f2328);
  }
  .writebar {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border, #d8d5d0);
    font-size: 12.5px;
    flex-wrap: wrap;
  }
  .writebar .field {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 220px;
  }
  .writebar input[type='text'] {
    flex: 1;
    min-width: 160px;
    padding: 3px 6px;
    border: 1px solid var(--border, #d8d5d0);
    border-radius: 6px;
    background: transparent;
    color: inherit;
    font-size: 12.5px;
  }
  .writebar .check {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
  .writebar button {
    border: 1px solid var(--border, #d8d5d0);
    background: transparent;
    color: inherit;
    border-radius: 6px;
    padding: 3px 10px;
    cursor: pointer;
    font-size: 12.5px;
  }
  .writebar button.primary {
    border-color: var(--accent, #0969da);
    color: var(--accent, #0969da);
  }
  .writebar button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .status {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    opacity: 0.85;
  }
  .split {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .conflicts {
    flex: 0 0 250px;
    border-right: 1px solid var(--border, #d8d5d0);
    overflow: auto;
    padding: 8px;
    font-size: 12.5px;
  }
  .head {
    margin-bottom: 6px;
    opacity: 0.85;
  }
  .ok {
    color: #2ea043;
  }
  .card {
    border: 1px solid var(--border, #d8d5d0);
    border-radius: 8px;
    padding: 6px 8px;
    margin-bottom: 8px;
  }
  .jump {
    background: transparent;
    border: none;
    color: inherit;
    padding: 0;
    cursor: pointer;
    font-size: 12px;
    opacity: 0.8;
  }
  .choices {
    display: flex;
    gap: 4px;
    margin-top: 6px;
    flex-wrap: wrap;
  }
  .choices button {
    border: 1px solid var(--border, #d8d5d0);
    background: transparent;
    color: inherit;
    border-radius: 6px;
    padding: 2px 8px;
    font-size: 12px;
    cursor: pointer;
  }
  .choices button.active {
    border-color: var(--accent, #0969da);
    color: var(--accent, #0969da);
  }
  .hint {
    margin-top: 4px;
    font-size: 11px;
    opacity: 0.6;
  }
  .output {
    position: relative;
    flex: 1;
    min-width: 0;
    overflow: auto;
    font-family: var(--font-mono, monospace);
    font-size: 12.5px;
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
  .row.conflict {
    background: color-mix(in srgb, #d29922 16%, transparent);
  }
  .row.first {
    border-top: 1px solid color-mix(in srgb, #d29922 60%, transparent);
  }
  .ln {
    flex: 0 0 52px;
    text-align: right;
    padding-right: 8px;
    color: var(--fg-faint, #8a8f98);
    user-select: none;
    font-variant-numeric: tabular-nums;
  }
  .src {
    flex: 0 0 20px;
    text-align: center;
    opacity: 0.7;
    user-select: none;
    font-size: 11px;
  }
  .mark {
    flex: 0 0 16px;
    text-align: center;
    user-select: none;
    color: #d29922;
  }
  .code {
    flex: 1;
    white-space: pre;
    overflow: hidden;
    text-overflow: ellipsis;
    padding-right: 8px;
  }
</style>
