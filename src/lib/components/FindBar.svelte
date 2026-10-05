<!--
  FindBar — 编辑态查找/替换条（全词/计数/历史/范围）。
  职责：查询/替换输入、大小写/正则/全词开关、查找范围、查找历史与匹配计数的交互；
        命中搜索、选区落定、高亮与编辑下发由 EditLayer 完成（本组件不触碰 IPC）。
  定位：fixed 相对视口（锚定阅读容器右上角），容器内部滚动不影响位置。
  默认值（大小写/全词/范围）仅在打开时从设置取一次（defaults prop）。
-->
<script lang="ts">
  import type { MessageKey } from '../i18n/zh-CN';
  import { t } from '../i18n/index.svelte';
  import type { FindScope, FindSettings, SearchMode } from '../ipc';

  type RangeTuple = [number, number] | null;

  interface Props {
    /** 打开时是否显示替换行 */
    replaceMode: boolean;
    /** 聚焦信号（每次打开自增；变化时重新聚焦并全选查找输入） */
    focusSignal: number;
    /** 定位基准容器（阅读 .reader；窗口尺寸变化时重算） */
    anchor: () => HTMLElement | null;
    /** 打开时刻的设置默认值（大小写/全词/范围初值） */
    defaults: FindSettings;
    /** 命中文案（EditLayer 计算；空串不显示） */
    countText: string;
    /** 查找历史（最新在前） */
    history: string[];
    /** 查找下一个 */
    onFindNext: (
      query: string,
      caseSensitive: boolean,
      mode: SearchMode,
      wholeWord: boolean,
      scope: FindScope,
      range: RangeTuple,
    ) => void;
    /** 替换一个（从当前选区起点/光标起） */
    onReplace: (
      query: string,
      replacement: string,
      caseSensitive: boolean,
      mode: SearchMode,
      wholeWord: boolean,
      scope: FindScope,
      range: RangeTuple,
    ) => void;
    /** 全部替换（走「预览 → 二次确认（可剔除）」流程；范围限定时直接替换范围内命中） */
    onReplaceAll: (
      query: string,
      replacement: string,
      caseSensitive: boolean,
      mode: SearchMode,
      wholeWord: boolean,
      scope: FindScope,
      range: RangeTuple,
    ) => void;
    /** 查询条件变化（查询/大小写/模式/全词/范围；EditLayer 据此刷新高亮与计数） */
    onQueryChange: (
      query: string,
      caseSensitive: boolean,
      mode: SearchMode,
      wholeWord: boolean,
      scope: FindScope,
      range: RangeTuple,
    ) => void;
    /** 清空查找历史 */
    onClearHistory: () => void;
    /** 关闭查找条 */
    onClose: () => void;
  }
  let {
    replaceMode,
    focusSignal,
    anchor,
    defaults,
    countText,
    history,
    onFindNext,
    onReplace,
    onReplaceAll,
    onQueryChange,
    onClearHistory,
    onClose,
  }: Props = $props();

  let query = $state('');
  let replacement = $state('');
  // svelte-ignore state_referenced_locally
  // （仅取初值：打开/挂载时读取设置默认，之后由用户交互接管）
  let caseSensitive = $state(defaults.caseSensitive);
  // svelte-ignore state_referenced_locally
  // （仅取初值：同上）
  let wholeWord = $state(defaults.wholeWord);
  // svelte-ignore state_referenced_locally
  // （仅取初值：同上）
  let scope = $state<FindScope>(defaults.defaultScope);
  /** 行范围输入（1 基行号，展示口径；EditLayer 转 0 基） */
  let rangeFrom = $state(1);
  let rangeTo = $state(1);
  /** 查找模式：标准字面 / 用户正则（`.*` 按钮切换） */
  let mode = $state<SearchMode>('literal');
  /** 历史下拉开关 */
  let historyOpen = $state(false);
  let findInput = $state<HTMLInputElement | null>(null);
  /** 相对视口的定位（top/right，px） */
  let position = $state<{ top: number; right: number }>({ top: 64, right: 24 });

  /** 范围选择项文案映射（显式表以便类型安全）。 */
  const SCOPE_LABELS: Record<FindScope, MessageKey> = {
    document: 'setting.enum.findScope.document',
    selection: 'setting.enum.findScope.selection',
    rowRange: 'setting.enum.findScope.rowRange',
  };

  /** 当前行范围参数（仅 rowRange 模式传出）。 */
  function rangeTuple(): RangeTuple {
    return scope === 'rowRange' ? [rangeFrom, rangeTo] : null;
  }

  /** 依据锚定容器重算位置（容器盒子在窗口内不随内容滚动移动）。 */
  function updatePosition(): void {
    const element = anchor();
    if (element === null) return;
    const rect = element.getBoundingClientRect();
    position = {
      top: rect.top + 10,
      right: Math.max(8, window.innerWidth - rect.right + 14),
    };
  }

  $effect(() => {
    updatePosition();
    window.addEventListener('resize', updatePosition);
    return () => window.removeEventListener('resize', updatePosition);
  });

  // 打开（或信号变化）时聚焦并全选，便于直接输入或替换上次查询
  $effect(() => {
    void focusSignal;
    findInput?.focus();
    findInput?.select();
  });

  // 查询条件变化上报（高亮/计数的刷新由 EditLayer 统一防抖调度）
  $effect(() => {
    onQueryChange(query, caseSensitive, mode, wholeWord, scope, rangeTuple());
  });

  // 点击组件外部时收起历史下拉（按钮与面板均在 .history-wrap 内）
  function handleWindowClick(event: MouseEvent): void {
    if (!historyOpen) return;
    const target = event.target;
    if (target instanceof Element && target.closest('.history-wrap')) return;
    historyOpen = false;
  }

  /** 选择历史项：回填查询并立即查找下一个。 */
  function pickHistory(item: string): void {
    query = item;
    historyOpen = false;
    onFindNext(query, caseSensitive, mode, wholeWord, scope, rangeTuple());
  }

  /** 查找条内按键：Esc 关闭（优先收历史下拉）；Enter 查找下一个 / 替换一个。 */
  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      if (historyOpen) {
        historyOpen = false;
        return;
      }
      onClose();
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      if (event.target === findInput) {
        onFindNext(query, caseSensitive, mode, wholeWord, scope, rangeTuple());
      } else {
        onReplace(query, replacement, caseSensitive, mode, wholeWord, scope, rangeTuple());
      }
    }
  }
</script>

<svelte:window onclick={handleWindowClick} />

<div
  class="find-bar"
  style="top: {position.top}px; right: {position.right}px"
  role="search"
  aria-label={t('find.aria')}
>
  <div class="row">
    <input
      bind:this={findInput}
      bind:value={query}
      class="input"
      placeholder={t('find.query')}
      aria-label={t('find.query')}
      spellcheck="false"
      onkeydown={handleKeydown}
    />
    <div class="history-wrap">
      <button
        class="icon"
        class:on={historyOpen}
        type="button"
        data-find-history
        aria-haspopup="listbox"
        aria-expanded={historyOpen}
        title={t('find.history')}
        aria-label={t('find.history')}
        onclick={() => (historyOpen = !historyOpen)}
      >
        ▾
      </button>
      {#if historyOpen}
        <div class="history-panel" role="listbox" aria-label={t('find.history')}>
          {#if history.length === 0}
            <div class="history-empty">{t('find.historyEmpty')}</div>
          {:else}
            {#each history as item (item)}
              <button
                class="history-item"
                type="button"
                role="option"
                aria-selected="false"
                data-find-history-item
                onclick={() => pickHistory(item)}
              >
                {item}
              </button>
            {/each}
            <button
              class="history-clear"
              type="button"
              data-find-history-clear
              onclick={onClearHistory}
            >
              {t('find.historyClear')}
            </button>
          {/if}
        </div>
      {/if}
    </div>
    <button
      class="icon"
      class:on={mode === 'regex'}
      type="button"
      aria-pressed={mode === 'regex'}
      title={t('find.regexHint')}
      aria-label={t('find.regex')}
      onclick={() => (mode = mode === 'regex' ? 'literal' : 'regex')}
    >
      .*
    </button>
    <button
      class="icon"
      class:on={caseSensitive}
      type="button"
      aria-pressed={caseSensitive}
      title={t('find.caseSensitive')}
      aria-label={t('find.caseSensitive')}
      onclick={() => (caseSensitive = !caseSensitive)}
    >
      Aa
    </button>
    <button
      class="icon"
      class:on={wholeWord}
      type="button"
      data-find-whole-word
      aria-pressed={wholeWord}
      title={t('find.wholeWordHint')}
      aria-label={t('find.wholeWord')}
      onclick={() => (wholeWord = !wholeWord)}
    >
      W
    </button>
    <select
      class="scope"
      bind:value={scope}
      data-find-scope
      aria-label={t('find.scope')}
      title={t('find.scope')}
    >
      {#each Object.entries(SCOPE_LABELS) as [value, labelKey] (value)}
        <option {value}>{t(labelKey as MessageKey)}</option>
      {/each}
    </select>
    {#if scope === 'rowRange'}
      <input
        class="range"
        type="number"
        min="1"
        bind:value={rangeFrom}
        data-find-range-from
        placeholder={t('find.rangeFrom')}
        aria-label={t('find.rangeFrom')}
      />
      <input
        class="range"
        type="number"
        min="1"
        bind:value={rangeTo}
        data-find-range-to
        placeholder={t('find.rangeTo')}
        aria-label={t('find.rangeTo')}
      />
    {/if}
    {#if countText}
      <span class="count" data-find-count>{countText}</span>
    {/if}
    <button
      class="action"
      disabled={query.length === 0}
      onclick={() => onFindNext(query, caseSensitive, mode, wholeWord, scope, rangeTuple())}
    >
      {t('find.next')}
    </button>
    <button class="icon" type="button" title={t('find.closeHint')} aria-label={t('find.close')} onclick={onClose}>×</button>
  </div>
  {#if replaceMode}
    <div class="row">
      <input
        bind:value={replacement}
        class="input"
        placeholder={t('find.replacement')}
        aria-label={t('find.replacement')}
        spellcheck="false"
        onkeydown={handleKeydown}
      />
      <button
        class="action"
        disabled={query.length === 0}
        onclick={() => onReplace(query, replacement, caseSensitive, mode, wholeWord, scope, rangeTuple())}
      >
        {t('find.replace')}
      </button>
      <button
        class="action"
        disabled={query.length === 0}
        onclick={() => onReplaceAll(query, replacement, caseSensitive, mode, wholeWord, scope, rangeTuple())}
      >
        {t('find.replaceAll')}
      </button>
    </div>
  {/if}
</div>

<style>
  .find-bar {
    position: fixed;
    z-index: 40;
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 6px;
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.12);
    font-size: 12.5px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .input {
    width: 200px;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--base);
    color: var(--ink);
    font: inherit;
    outline: none;
  }

  .input:focus {
    border-color: var(--accent);
  }

  .icon {
    min-width: 26px;
    height: 26px;
    padding: 0 6px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: transparent;
    color: var(--ink);
    font: inherit;
    cursor: default;
  }

  .icon.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .action {
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: transparent;
    color: var(--ink);
    font: inherit;
    cursor: default;
  }

  .action:hover:not(:disabled),
  .icon:hover {
    background: var(--hover);
  }

  .action:disabled {
    color: var(--muted);
  }

  .scope {
    height: 26px;
    max-width: 120px;
    padding: 0 4px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--base);
    color: var(--ink);
    font: inherit;
  }

  .range {
    width: 64px;
    height: 26px;
    padding: 0 6px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--base);
    color: var(--ink);
    font: inherit;
    outline: none;
  }

  .range:focus {
    border-color: var(--accent);
  }

  .count {
    color: var(--muted);
    white-space: nowrap;
  }

  .history-wrap {
    position: relative;
  }

  .history-panel {
    position: absolute;
    top: 30px;
    left: 0;
    z-index: 50;
    display: flex;
    flex-direction: column;
    min-width: 200px;
    max-height: 240px;
    overflow-y: auto;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 6px;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.16);
  }

  .history-item {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
    padding: 5px 8px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--ink);
    font: inherit;
    cursor: default;
  }

  .history-item:hover {
    background: var(--hover);
  }

  .history-clear {
    margin-top: 2px;
    padding: 5px 8px;
    border: 0;
    border-top: 1px solid var(--line);
    border-radius: 0 0 4px 4px;
    background: transparent;
    color: var(--muted);
    font: inherit;
    text-align: left;
    cursor: default;
  }

  .history-clear:hover {
    background: var(--hover);
    color: var(--ink);
  }

  .history-empty {
    padding: 6px 8px;
    color: var(--muted);
  }
</style>
