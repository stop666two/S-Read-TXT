<!--
  EncodingMenu — 编码切换下拉（工具栏与状态栏共用，避免重复实现）。
  组成：触发按钮（显示传入文案）+ 下拉（自动检测 + 支持的编码，单选标记）。
  行为：点击外部自动收起；选择后回调 onPick（null = 自动检测）。
  openUp：下拉向上展开（状态栏位于窗口底部时使用）。
-->
<script lang="ts">
  interface Props {
    /** 触发按钮显示文案（工具栏：「编码：自动」；状态栏：当前编码） */
    displayLabel: string;
    /** 支持的编码列表（后端提供） */
    encodings: string[];
    /** 当前手动编码（null = 自动检测，用于单选标记） */
    override: string | null;
    /** 选择回调（null = 自动检测） */
    onPick: (label: string | null) => void;
    /** 下拉向上展开（默认向下） */
    openUp?: boolean;
  }
  let { displayLabel, encodings, override, onPick, openUp = false }: Props = $props();

  /** 下拉开合 */
  let open = $state(false);

  /** 选择编码并收起 */
  function pick(label: string | null): void {
    open = false;
    onPick(label);
  }
</script>

<svelte:window
  onclick={(event) => {
    // 点击下拉之外区域时收起（多处实例各自处理，互不影响）
    const target = event.target as HTMLElement | null;
    if (!target?.closest('.encoding-wrap')) open = false;
  }}
/>

<div class="encoding-wrap">
  <button
    class="enc-btn"
    title="切换编码"
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    {displayLabel}
  </button>
  {#if open}
    <div class="dropdown" class:up={openUp} role="menu">
      <button class="item" onclick={() => pick(null)}>
        <span class="radio" class:on={override === null}></span>
        <span>自动检测</span>
      </button>
      {#each encodings as label (label)}
        <button class="item" onclick={() => pick(label)}>
          <span class="radio" class:on={override === label}></span>
          <span>{label}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .encoding-wrap {
    position: relative;
  }

  .enc-btn {
    height: 24px;
    padding: 0 10px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 12.5px;
    cursor: default;
  }

  .enc-btn:hover {
    background: var(--hover);
  }

  .dropdown {
    position: absolute;
    top: calc(100% + 3px);
    left: 0;
    z-index: 30;
    min-width: 160px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  .dropdown.up {
    top: auto;
    bottom: calc(100% + 3px);
    left: auto;
    right: 0;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--ink);
    font: inherit;
    font-size: 12.5px;
    text-align: left;
    cursor: default;
  }

  .item:hover {
    background: var(--hover);
  }

  .radio {
    width: 12px;
    height: 12px;
    border: 1px solid var(--muted);
    border-radius: 50%;
    box-sizing: border-box;
  }

  .radio.on {
    border: 4px solid var(--accent);
  }
</style>
