<script lang="ts">
  // 比较/合并窗口应用（compare.html 入口）：
  // 读取待处理请求（启动或事件通知），加载差异/合并数据并渲染工具条与视图。
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';

  import TitleBar from '../../lib/components/TitleBar.svelte';
  import { ipc, toIpcError, type CompareRequest, type DiffDocs, type MergeDocs } from '../../lib/ipc';
  import { setLocale, t } from '../../lib/i18n/index.svelte';
  import { applyThemeTokens } from '../../lib/theme';
  import DiffView from './DiffView.svelte';
  import MergeView from './MergeView.svelte';

  let request = $state<CompareRequest | null>(null);
  let diff = $state<DiffDocs | null>(null);
  let merge = $state<MergeDocs | null>(null);
  let view = $state<'side' | 'unified'>('side');
  let error = $state('');
  let loading = $state(false);
  let nav = $state<{ prev: () => void; next: () => void } | null>(null);

  const title = $derived(
    request?.mode === 'merge'
      ? t('compare.mergeTitle')
      : t('compare.title'),
  );

  async function load(req: CompareRequest): Promise<void> {
    request = req;
    diff = null;
    merge = null;
    error = '';
    loading = true;
    try {
      if (req.mode === 'diff') {
        diff = await ipc.diffDocs(req.left, req.right);
      } else {
        merge = await ipc.merge3Docs(req.left, req.right, req.extra ?? '');
      }
    } catch (err) {
      error = t('compare.loadFailed', { reason: toIpcError(err).message });
    } finally {
      loading = false;
    }
  }

  async function pull(): Promise<void> {
    const req = await ipc.takeCompareRequest();
    if (req) await load(req);
  }

  onMount(() => {
    // 主题（默认解析当前主题；跟随系统时监听明暗切换）
    const applyTheme = async (): Promise<void> => {
      try {
        const resolved = await ipc.getTheme(null);
        applyThemeTokens(resolved, { enabled: true, durationMs: 0 });
      } catch {
        // 主题解析失败：保持 base.css 默认浅色兜底
      }
    };
    void applyTheme();
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const onMedia = (): void => void applyTheme();
    media.addEventListener('change', onMedia);

    // 语言包（与主窗口一致：读当前设置语言）
    void ipc
      .getSettings()
      .then((settings) => setLocale(settings.app.locale ?? 'zh-CN'))
      .catch(() => setLocale('zh-CN'));

    void pull();
    let stop: (() => void) | undefined;
    void listen('srt://compare-request', () => void pull()).then((unlisten) => {
      stop = unlisten;
    });
    return () => {
      media.removeEventListener('change', onMedia);
      stop?.();
    };
  });
</script>

<div class="shell">
  <TitleBar {title} showMaximize={true} />
  <div class="toolbar">
    <div class="modes" role="tablist" aria-label={t('compare.view')}>
      <button
        type="button"
        class="mode"
        class:active={view === 'side'}
        role="tab"
        aria-selected={view === 'side'}
        data-compare-view="side"
        onclick={() => (view = 'side')}
      >
        {t('compare.sideBySide')}
      </button>
      <button
        type="button"
        class="mode"
        class:active={view === 'unified'}
        role="tab"
        aria-selected={view === 'unified'}
        data-compare-view="unified"
        onclick={() => (view = 'unified')}
      >
        {t('compare.unified')}
      </button>
    </div>
    {#if diff}
      <span class="stats" data-diff-stats>
        {t('compare.stats', { added: diff.added, removed: diff.removed })}
      </span>
      <span class="names" data-diff-names>
        {diff.leftName} ↔ {diff.rightName}
      </span>
      <div class="nav">
        <button type="button" data-diff-prev onclick={() => nav?.prev()}>
          {t('compare.prev')}
        </button>
        <button type="button" data-diff-next onclick={() => nav?.next()}>
          {t('compare.next')}
        </button>
      </div>
    {/if}
    {#if merge}
      <span class="stats" data-merge-stats>
        {t('compare.conflicts', { count: merge.conflicts })}
      </span>
    {/if}
    {#if loading}
      <span class="loading">{t('compare.loading')}</span>
    {/if}
  </div>
  {#if error}
    <div class="error" data-compare-error>{error}</div>
  {/if}
  <div class="content">
    {#if diff}
      {#if diff.hunks.length === 0}
        <div class="same" data-diff-same>{t('compare.identical')}</div>
      {:else}
        <DiffView {diff} {view} onNavReady={(value) => (nav = value)} />
      {/if}
    {:else if merge}
      <MergeView {merge} />
    {/if}
  </div>
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg, #faf9f7);
    color: var(--fg, #1f2328);
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border, #d8d5d0);
    flex: 0 0 auto;
  }
  .modes {
    display: flex;
    gap: 4px;
  }
  .mode {
    border: 1px solid transparent;
    background: transparent;
    color: inherit;
    padding: 3px 10px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 12.5px;
  }
  .mode.active {
    border-color: var(--accent, #0969da);
    color: var(--accent, #0969da);
  }
  .stats,
  .names,
  .loading {
    font-size: 12.5px;
    opacity: 0.85;
    white-space: nowrap;
  }
  .names {
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .nav {
    display: flex;
    gap: 4px;
  }
  .nav button {
    border: 1px solid var(--border, #d8d5d0);
    background: transparent;
    color: inherit;
    border-radius: 6px;
    padding: 3px 10px;
    cursor: pointer;
    font-size: 12.5px;
  }
  .error {
    padding: 10px;
    color: #d1242f;
    font-size: 13px;
  }
  .content {
    flex: 1;
    min-height: 0;
  }
  .same {
    padding: 24px;
    text-align: center;
    opacity: 0.7;
  }
</style>
