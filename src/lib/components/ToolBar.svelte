<script lang="ts">
  // 工具栏：打开 / 历史 / 编码（下拉切换）/ 主题（循环）/ 设置。
  // 编码下拉选项：自动检测 + 后端提供的编码列表；选择后由上层调用 IPC 并更新标签。
  import Icon from './Icon.svelte';
  import type { ThemeChoice } from '../types';

  interface Props {
    /** 当前主题选择 */
    themeChoice: ThemeChoice;
    /** 主题切换回调 */
    onThemeChange: (theme: ThemeChoice) => void;
    /** 支持的编码列表（后端提供；空数组时下拉只显示自动检测） */
    encodings: string[];
    /** 当前手动编码（null = 自动检测） */
    encodingOverride: string | null;
    /** 编码切换（null = 自动检测） */
    onEncodingChange: (label: string | null) => void;
    /** 打开文件回调 */
    onOpenFile?: () => void;
    /** 历史面板回调（阶段 6 接线） */
    onHistory?: () => void;
    /** 设置窗口回调（阶段 8 接线） */
    onSettings?: () => void;
    /** 是否处于编辑模式（编辑按钮激活态） */
    editing: boolean;
    /** 是否有未保存修改（保存按钮可用性） */
    canSave: boolean;
    /** 编辑模式切换回调 */
    onToggleEdit?: () => void;
    /** 保存回调 */
    onSave?: () => void;
  }
  let {
    themeChoice,
    onThemeChange,
    encodings,
    encodingOverride,
    onEncodingChange,
    onOpenFile,
    onHistory,
    onSettings,
    editing,
    canSave,
    onToggleEdit,
    onSave,
  }: Props = $props();

  /** 编码下拉开合 */
  let encodingOpen = $state(false);

  /** 主题循环顺序（含跟随系统） */
  const themeCycle: ThemeChoice[] = ['light', 'dark', 'eye', 'system'];
  /** 主题按钮提示文案 */
  const themeNames: Record<ThemeChoice, string> = {
    light: '浅色',
    dark: '深色',
    eye: '护眼',
    system: '跟随系统',
  };

  /** 循环切换主题 */
  function cycleTheme(): void {
    const index = themeCycle.indexOf(themeChoice);
    onThemeChange(themeCycle[(index + 1) % themeCycle.length]);
  }

  /** 选择编码并收起下拉 */
  function pickEncoding(label: string | null): void {
    encodingOpen = false;
    onEncodingChange(label);
  }
</script>

<svelte:window
  onclick={(event) => {
    // 点击编码下拉之外区域时收起
    const target = event.target as HTMLElement | null;
    if (!target?.closest('.encoding-wrap')) encodingOpen = false;
  }}
/>

<div class="toolbar">
  <button class="icon-btn" title="打开文件（Ctrl+O）" aria-label="打开文件" onclick={() => onOpenFile?.()}>
    <Icon name="open" />
  </button>
  <button class="icon-btn" title="历史记录" aria-label="历史记录" onclick={() => onHistory?.()}>
    <Icon name="history" />
  </button>
  <div class="sep"></div>
  <button
    class="icon-btn"
    class:active={editing}
    title={editing ? '退出编辑模式（Ctrl+E）' : '启用编辑模式（Ctrl+E）'}
    aria-label="切换编辑模式"
    aria-pressed={editing}
    onclick={() => onToggleEdit?.()}
  >
    <Icon name="edit" />
  </button>
  <button
    class="icon-btn"
    title="保存（Ctrl+S）"
    aria-label="保存"
    disabled={!canSave}
    onclick={() => onSave?.()}
  >
    <Icon name="save" />
  </button>
  <div class="sep"></div>
  <div class="encoding-wrap">
    <button
      class="text-btn"
      title="切换文件编码"
      aria-haspopup="menu"
      aria-expanded={encodingOpen}
      onclick={() => (encodingOpen = !encodingOpen)}
    >
      编码：{encodingOverride ?? '自动'}
    </button>
    {#if encodingOpen}
      <div class="dropdown" role="menu">
        <button class="item" onclick={() => pickEncoding(null)}>
          <span class="radio" class:on={encodingOverride === null}></span>
          <span>自动检测</span>
        </button>
        {#each encodings as label (label)}
          <button class="item" onclick={() => pickEncoding(label)}>
            <span class="radio" class:on={encodingOverride === label}></span>
            <span>{label}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
  <div class="sep"></div>
  <button
    class="icon-btn"
    title={`主题：${themeNames[themeChoice]}（点击循环切换）`}
    aria-label="切换主题"
    onclick={cycleTheme}
  >
    <Icon name="palette" />
  </button>
  <button class="icon-btn" title="设置" aria-label="设置" onclick={() => onSettings?.()}>
    <Icon name="settings" />
  </button>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    height: var(--h-toolbar);
    padding: 0 8px;
    background: var(--chrome);
    border-bottom: 1px solid var(--line);
    user-select: none;
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--ink);
    cursor: default;
  }

  .icon-btn:hover {
    background: var(--hover);
  }

  .icon-btn.active {
    background: var(--hover);
    color: var(--accent);
  }

  .icon-btn:disabled {
    color: var(--muted);
    opacity: 0.55;
  }

  .text-btn {
    height: 24px;
    padding: 0 10px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--ink);
    font: inherit;
    font-size: 12.5px;
    cursor: default;
  }

  .text-btn:hover {
    background: var(--hover);
  }

  .sep {
    width: 1px;
    height: 18px;
    margin: 0 4px;
    background: var(--line);
  }

  .encoding-wrap {
    position: relative;
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
