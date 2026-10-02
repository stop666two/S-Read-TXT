<script lang="ts">
  // 工具栏：打开 / 历史 / 编码 / 主题 / 设置（图标按钮 + 悬停提示）。
  // 阶段 2b：仅主题按钮接线（循环切换）；其余按钮在阶段 2c/6/8 接线。
  // 编码按钮显示当前编码文本（阶段 2c 改为下拉切换）。
  import Icon from './Icon.svelte';
  import type { ThemeChoice } from '../types';

  interface Props {
    /** 当前主题选择 */
    themeChoice: ThemeChoice;
    /** 主题切换回调 */
    onThemeChange: (theme: ThemeChoice) => void;
    /** 当前编码显示文本（阶段 2c 接入真实标签） */
    encodingLabel?: string;
    /** 打开文件回调（阶段 2c 接线） */
    onOpenFile?: () => void;
    /** 历史面板回调（阶段 6 接线） */
    onHistory?: () => void;
    /** 设置窗口回调（阶段 8 接线） */
    onSettings?: () => void;
  }
  let {
    themeChoice,
    onThemeChange,
    encodingLabel = 'UTF-8',
    onOpenFile,
    onHistory,
    onSettings,
  }: Props = $props();

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
</script>

<div class="toolbar">
  <button class="icon-btn" title="打开文件（Ctrl+O）" aria-label="打开文件" onclick={() => onOpenFile?.()}>
    <Icon name="open" />
  </button>
  <button class="icon-btn" title="历史记录" aria-label="历史记录" onclick={() => onHistory?.()}>
    <Icon name="history" />
  </button>
  <div class="sep"></div>
  <button class="text-btn" title="切换文件编码">编码：{encodingLabel}</button>
  <div class="sep"></div>
  <button class="icon-btn" title={`主题：${themeNames[themeChoice]}（点击循环切换）`} aria-label="切换主题" onclick={cycleTheme}>
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
</style>
