<script lang="ts">
  // 状态栏：按设置 `app.status.items` 驱动显示项与顺序。
  // 显示项候选：lineCol（行:列 / 第 N 行）/ counts（字数）/ words（词数）/
  //             progress（阅读进度）/ size（大小）/ encoding（编码菜单）/
  //             eol（换行符 + 编辑态转换菜单）/ modified（修改标记）。
  // 无打开文件时：左显示应用名与版本，右显示状态文案。
  import type { MessageKey } from '../i18n/zh-CN';
  import { t } from '../i18n/index.svelte';
  import type { StatusSettings, TextStats } from '../ipc';
  import EncodingMenu from './EncodingMenu.svelte';
  import { isScreenReaderEnhanced, onA11yChange } from '../a11y';

  /** 屏幕阅读器增强开关（经订阅桥接为响应式） */
  let srEnabled = $state(isScreenReaderEnhanced());
  $effect(() => onA11yChange(() => (srEnabled = isScreenReaderEnhanced())));

  /** 状态栏显示项（设置缺省时的兜底顺序；与 Rust DEFAULT_STATUS_ITEMS 对齐）。 */
  const DEFAULT_ITEMS = ['lineCol', 'counts', 'progress', 'size', 'encoding', 'eol', 'modified'];

  interface Props {
    /** 文件名（无打开文件时不传） */
    fileName?: string;
    /** 应用版本（无文件时展示，如 v0.0.1-beta） */
    version?: string;
    /** 是否只读（文件超过只读阈值：展示「只读」标记） */
    readOnly?: boolean;
    /** 状态栏显示设置（未就绪为 null 时用兜底顺序与默认值） */
    statusSettings?: StatusSettings | null;
    /** 是否处于编辑模式（决定行列/选区统计/换行转换可用性） */
    editing?: boolean;
    /** 阅读百分比（0–100） */
    percent?: number;
    /** 文件大小展示文本（已格式化） */
    sizeLabel?: string;
    /** 当前编码标签（有效编码展示：自动检测结果或手动覆盖） */
    encodingLabel?: string;
    /** 支持的编码列表（传入时编码可点击切换，向后兼容缺省为纯展示） */
    encodings?: string[];
    /** 当前手动编码（null = 自动检测；单选标记） */
    encodingOverride?: string | null;
    /** 编码切换回调（null = 自动检测） */
    onEncodingChange?: (label: string | null) => void;
    /** 换行符风格（lf/crlf/cr/mixed/unknown；null = 未知） */
    eol?: string | null;
    /** 换行符转换回调（仅编辑态由父组件传入） */
    onConvertEol?: (target: 'lf' | 'crlf' | 'cr') => void;
    /** 文档统计（全文件；null = 未就绪） */
    docStats?: TextStats | null;
    /** 选区统计（编辑态；null = 无选区） */
    selectionStats?: TextStats | null;
    /** 光标行列（编辑态；1 基；null = 未知） */
    lineCol?: { row: number; column: number } | null;
    /** 顶部可视行（阅读态；1 基；null = 未知） */
    topRow?: number | null;
    /** 是否有未保存修改 */
    dirty?: boolean;
    /** 今日阅读秒数（null = 未就绪） */
    readingSeconds?: number | null;
    /** 跳转到行回调（点击行列时触发；1 基行号） */
    onGotoLine?: (row1: number) => void;
  }
  let {
    fileName,
    version = '',
    readOnly = false,
    statusSettings = null,
    editing = false,
    percent,
    sizeLabel,
    encodingLabel,
    encodings = [],
    encodingOverride = null,
    onEncodingChange,
    eol = null,
    onConvertEol,
    docStats = null,
    selectionStats = null,
    lineCol = null,
    topRow = null,
    dirty = false,
    readingSeconds = null,
    onGotoLine,
  }: Props = $props();

  const items = $derived(statusSettings?.items ?? DEFAULT_ITEMS);
  const countMode = $derived(statusSettings?.countMode ?? 'grapheme');
  const emptySelectionText = $derived(statusSettings?.emptySelectionText ?? '');
  const clickableGoto = $derived(statusSettings?.clickableGoto ?? true);
  const clickableEol = $derived(statusSettings?.clickableEol ?? true);
  /** 换行转换菜单可用性（编辑态 + 设置允许 + 回调就绪）。 */
  const canConvertEol = $derived(editing && clickableEol && !!onConvertEol);

  /** 计数单位文案键（随 countMode 切换）。 */
  const UNIT_KEYS: Record<string, MessageKey> = {
    grapheme: 'status.unitGrapheme',
    codepoint: 'status.unitCodepoint',
    byte: 'status.unitByte',
  };

  /** 按计数模式取值（grapheme/codepoint/byte）。 */
  function modeValue(stats: TextStats): number {
    if (countMode === 'codepoint') return stats.codepoints;
    if (countMode === 'byte') return stats.bytes;
    return stats.graphemes;
  }

  /** 计数文本（capped 加「≥」前缀；千分位随语言）。 */
  function countText(stats: TextStats): string {
    const value = `${stats.capped ? '≥' : ''}${modeValue(stats).toLocaleString()}`;
    return t('status.counts', {
      n: value,
      unit: t(UNIT_KEYS[countMode] ?? 'status.unitGrapheme'),
    });
  }

  /** 当前换行符显示文案（未知值兜底为 —）。 */
  const eolLabel = $derived(
    eol && ['lf', 'crlf', 'cr', 'mixed', 'unknown'].includes(eol)
      ? t(`status.eol.${eol}` as MessageKey)
      : t('status.eol.unknown'),
  );

  /** 换行符转换目标（实时求值：语言切换立即生效）。 */
  const eolTargets = $derived([
    { id: 'lf' as const, label: t('status.eol.lf') },
    { id: 'crlf' as const, label: t('status.eol.crlf') },
    { id: 'cr' as const, label: t('status.eol.cr') },
  ]);

  // —— 行跳转（点击行列 → 内联输入） ——
  let gotoOpen = $state(false);
  let gotoValue = $state('');

  function openGoto(): void {
    if (!clickableGoto || !onGotoLine) return;
    gotoValue = String(lineCol?.row ?? topRow ?? 1);
    gotoOpen = true;
  }

  function submitGoto(): void {
    const n = Number.parseInt(gotoValue, 10);
    if (Number.isFinite(n) && n >= 1) onGotoLine?.(n);
    gotoOpen = false;
  }

  /** 输入框挂载即聚焦并全选（供行跳转内联输入使用）。 */
  function focusNow(node: HTMLInputElement): void {
    node.focus();
    node.select();
  }

  // —— 换行符转换菜单 ——
  let eolOpen = $state(false);

  function pickEol(target: 'lf' | 'crlf' | 'cr'): void {
    eolOpen = false;
    onConvertEol?.(target);
  }
</script>

<footer
  class="status-bar"
  role={srEnabled ? 'status' : undefined}
  aria-live={srEnabled ? 'polite' : 'off'}
>
  {#if fileName !== undefined}
    <span class="left">
      <span class="name">{fileName}</span>
      {#if readOnly}
        <span class="readonly" title={t('status.readOnlyHint')}>{t('status.readOnly')}</span>
      {/if}
      {#each items as item, i (item)}
        {#if i > 0}<span class="dot">·</span>{/if}
        {#if item === 'lineCol'}
          {#if gotoOpen}
            <input
              class="goto-input"
              type="text"
              bind:value={gotoValue}
              use:focusNow
              onkeydown={(event) => {
                if (event.key === 'Enter') submitGoto();
                if (event.key === 'Escape') gotoOpen = false;
                event.stopPropagation();
              }}
              onblur={() => (gotoOpen = false)}
            />
          {:else if editing && lineCol}
            <button class="item click" title={t('status.goto')} onclick={openGoto}>
              {t('status.lineCol', { row: lineCol.row, col: lineCol.column })}
            </button>
          {:else if topRow}
            <button class="item click" title={t('status.goto')} onclick={openGoto}>
              {t('status.line', { row: topRow })}
            </button>
          {/if}
        {:else if item === 'counts'}
          {#if editing}
            {#if selectionStats}
              <span class="item">{t('status.selection', { text: countText(selectionStats) })}</span>
            {:else}
              <span class="item empty-selection">{emptySelectionText}</span>
            {/if}
          {:else if docStats}
            <span class="item">{countText(docStats)}</span>
          {/if}
        {:else if item === 'words'}
          {#if editing}
            {#if selectionStats}
              <span class="item">{t('status.words', { n: selectionStats.words.toLocaleString() })}</span>
            {:else}
              <span class="item empty-selection">{emptySelectionText}</span>
            {/if}
          {:else if docStats}
            <span class="item">{t('status.words', { n: docStats.words.toLocaleString() })}</span>
          {/if}
        {:else if item === 'progress'}
          {#if percent !== undefined}
            <span class="item">{t('status.reading', { percent: Math.round(percent) })}</span>
          {/if}
        {:else if item === 'size'}
          {#if sizeLabel}<span class="item">{sizeLabel}</span>{/if}
        {:else if item === 'encoding'}
          {#if encodingLabel !== undefined}
            {#if onEncodingChange}
              <EncodingMenu
                displayLabel={encodingLabel ?? '—'}
                {encodings}
                override={encodingOverride}
                onPick={onEncodingChange}
                openUp
              />
            {:else}
              <span class="encoding" title={t('status.encodingHint')}>{encodingLabel ?? '—'}</span>
            {/if}
          {/if}
        {:else if item === 'eol'}
          {#if eol}
            {#if canConvertEol}
              <span class="eol-wrap">
                <button
                  class="item click"
                  title={t('status.eolMenu')}
                  onclick={() => (eolOpen = !eolOpen)}
                >
                  {eolLabel}
                </button>
                {#if eolOpen}
                  <span class="eol-menu" role="menu">
                    {#each eolTargets as target (target.id)}
                      <button class="eol-item" role="menuitem" onclick={() => pickEol(target.id)}>
                        {target.label}
                      </button>
                    {/each}
                  </span>
                {/if}
              </span>
            {:else}
              <span class="item" title={editing ? undefined : t('status.eolOnlyEdit')}>
                {eolLabel}
              </span>
            {/if}
          {/if}
        {:else if item === 'modified'}
          {#if dirty}
            <span class="modified" title={t('status.modified')}>●</span>
          {/if}
        {:else if item === 'readTime'}
          {#if readingSeconds !== null}
            <span class="item"
              >{t('status.readTime', {
                h: Math.floor(readingSeconds / 3600),
                m: Math.floor((readingSeconds % 3600) / 60),
              })}</span
            >
          {/if}
        {/if}
      {/each}
    </span>
    <span class="right"></span>
  {:else}
    <span class="left">S-Read-TXT {version}</span>
    <span class="right">{t('status.noFile')}</span>
  {/if}
</footer>

<style>
  .status-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: var(--h-status);
    padding: 0 10px;
    background: var(--chrome);
    border-top: 1px solid var(--line);
    color: var(--muted);
    font-size: 12px;
    user-select: none;
  }

  .left,
  .right {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .name {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 32vw;
  }

  .dot {
    color: var(--line);
  }

  .item {
    white-space: nowrap;
  }

  .empty-selection {
    color: var(--line);
  }

  .click {
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }

  .click:hover {
    color: var(--ink);
  }

  .goto-input {
    width: 64px;
    padding: 0 4px;
    border: 1px solid var(--line);
    border-radius: 3px;
    background: var(--surface);
    color: var(--ink);
    font: inherit;
  }

  .encoding {
    color: var(--muted);
  }

  .readonly {
    padding: 0 5px;
    border: 1px solid var(--line);
    border-radius: 3px;
    font-size: 11px;
    color: var(--warn, #8a5200);
  }

  .modified {
    color: var(--accent);
  }

  .eol-wrap {
    position: relative;
    display: inline-flex;
  }

  .eol-menu {
    position: absolute;
    bottom: calc(100% + 6px);
    left: 0;
    display: flex;
    flex-direction: column;
    min-width: 96px;
    padding: 4px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--surface);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.12);
    z-index: 30;
  }

  .eol-item {
    padding: 4px 8px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--ink);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .eol-item:hover {
    background: var(--hover);
  }
</style>
