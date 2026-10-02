<!--
  FindBar — 编辑态查找/替换条（阶段 4c）。
  职责：查询/替换输入、大小写开关、下一个/替换/全部替换与关闭的交互；
        命中搜索、选区落定与编辑下发由 EditLayer 完成（本组件不触碰 IPC）。
  定位：fixed 相对视口（锚定阅读容器右上角），容器内部滚动不影响位置。
-->
<script lang="ts">
  import type { SearchMode } from '../ipc';

  interface Props {
    /** 打开时是否显示替换行 */
    replaceMode: boolean;
    /** 聚焦信号（每次打开自增；变化时重新聚焦并全选查找输入） */
    focusSignal: number;
    /** 定位基准容器（阅读 .reader；窗口尺寸变化时重算） */
    anchor: () => HTMLElement | null;
    /** 查找下一个 */
    onFindNext: (query: string, caseSensitive: boolean, mode: SearchMode) => void;
    /** 替换一个（从当前选区起点/光标起） */
    onReplace: (query: string, replacement: string, caseSensitive: boolean, mode: SearchMode) => void;
    /** 全部替换（走「预览 → 二次确认（可剔除）→ 执行」流程） */
    onReplaceAll: (
      query: string,
      replacement: string,
      caseSensitive: boolean,
      mode: SearchMode,
    ) => void;
    /** 查询条件变化（查询/大小写/模式；EditLayer 据此刷新文档高亮） */
    onQueryChange: (query: string, caseSensitive: boolean, mode: SearchMode) => void;
    /** 关闭查找条 */
    onClose: () => void;
  }
  let {
    replaceMode,
    focusSignal,
    anchor,
    onFindNext,
    onReplace,
    onReplaceAll,
    onQueryChange,
    onClose,
  }: Props = $props();

  let query = $state('');
  let replacement = $state('');
  let caseSensitive = $state(false);
  /** 查找模式：标准字面 / 用户正则（`.*` 按钮切换） */
  let mode = $state<SearchMode>('literal');
  let findInput = $state<HTMLInputElement | null>(null);
  /** 相对视口的定位（top/right，px） */
  let position = $state<{ top: number; right: number }>({ top: 64, right: 24 });

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

  // 查询条件变化上报（文档高亮的刷新由 EditLayer 统一防抖调度）
  $effect(() => {
    onQueryChange(query, caseSensitive, mode);
  });

  /** 查找条内按键：Esc 关闭；Enter 查找下一个 / 替换一个。 */
  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      onClose();
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      if (event.target === findInput) onFindNext(query, caseSensitive, mode);
      else onReplace(query, replacement, caseSensitive, mode);
    }
  }
</script>

<div
  class="find-bar"
  style="top: {position.top}px; right: {position.right}px"
  role="search"
  aria-label="查找与替换"
>
  <div class="row">
    <input
      bind:this={findInput}
      bind:value={query}
      class="input"
      placeholder="查找内容"
      aria-label="查找内容"
      spellcheck="false"
      onkeydown={handleKeydown}
    />
    <button
      class="icon"
      class:on={mode === 'regex'}
      aria-pressed={mode === 'regex'}
      title="正则表达式（Rust regex 语法；替换支持 $1 捕获展开）"
      aria-label="正则表达式"
      onclick={() => (mode = mode === 'regex' ? 'literal' : 'regex')}
    >
      .*
    </button>
    <button
      class="icon"
      class:on={caseSensitive}
      aria-pressed={caseSensitive}
      title="区分大小写"
      aria-label="区分大小写"
      onclick={() => (caseSensitive = !caseSensitive)}
    >
      Aa
    </button>
    <button
      class="action"
      disabled={query.length === 0}
      onclick={() => onFindNext(query, caseSensitive, mode)}
    >
      下一个
    </button>
    <button class="icon" title="关闭（Esc）" aria-label="关闭查找" onclick={onClose}>×</button>
  </div>
  {#if replaceMode}
    <div class="row">
      <input
        bind:value={replacement}
        class="input"
        placeholder="替换为"
        aria-label="替换为"
        spellcheck="false"
        onkeydown={handleKeydown}
      />
      <button
        class="action"
        disabled={query.length === 0}
        onclick={() => onReplace(query, replacement, caseSensitive, mode)}
      >
        替换
      </button>
      <button
        class="action"
        disabled={query.length === 0}
        onclick={() => onReplaceAll(query, replacement, caseSensitive, mode)}
      >
        全部替换
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
</style>
