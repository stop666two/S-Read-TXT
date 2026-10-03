<!--
  EmptyState — 欢迎页（未打开任何文件时的空状态，P0-7 按已确认设计稿实现）。
  组成：品牌区（logo + 名称）· 主按钮（打开文件）· 最近打开（≤3 条，点击直达）·
        快捷键提示行（取生效绑定）· 拖拽/编辑提示。
  注：「新建文件 / 载入示例文本」按钮待 P3「新建文件」功能落地后加入（不展示无效按钮）。
-->
<script lang="ts">
  import { t } from '../i18n/index.svelte';
  import type { MessageKey } from '../i18n/zh-CN';
  import type { HistoryEntry } from '../ipc';

  interface Props {
    /** 触发打开文件（接系统对话框） */
    onOpen: () => void;
    /** 最近打开（欢迎页列表；展示前 3 条） */
    recent?: HistoryEntry[];
    /** 最近打开点击回调 */
    onOpenRecent?: (entry: HistoryEntry) => void;
    /** 生效快捷键（动作 → 组合键；缺失项自动隐藏） */
    bindings?: Record<string, string>;
  }
  let { onOpen, recent = [], onOpenRecent, bindings = {} }: Props = $props();

  /** 快捷键提示行（顺序同设计稿；组合键取生效绑定，缺省回退默认） */
  const hintItems = $derived.by(() => {
    const items: { action: string; fallback: string }[] = [
      { action: 'openFile', fallback: 'Ctrl+O' },
      { action: 'historyPanel', fallback: 'Ctrl+Shift+H' },
      { action: 'find', fallback: 'Ctrl+F' },
      { action: 'fullscreen', fallback: 'F11' },
    ];
    return items.map((item) => ({
      action: item.action,
      combo: bindings[item.action] ?? item.fallback,
      label: t(`shortcut.${item.action}` as MessageKey),
    }));
  });

  /** 取路径上一级目录名（展示用；无则空串） */
  function folderOf(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts.length >= 2 ? parts[parts.length - 2] : '';
  }

  /** 行meta：目录 · 阅读进度（无进度时仅目录） */
  function metaOf(entry: HistoryEntry): string {
    const folder = folderOf(entry.path);
    const percent = entry.lastPercent > 0 ? `${Math.round(entry.lastPercent)}%` : '';
    return folder && percent ? `${folder} · ${percent}` : folder || percent;
  }
</script>

<div class="empty">
  <div class="welcome">
    <div class="brand">
      <div class="logo" aria-hidden="true">S</div>
      <span class="appname">S-Read-TXT</span>
    </div>
    <p class="tagline">{t('empty.tagline')}</p>
    <div class="actions">
      <button class="open-btn" type="button" onclick={onOpen}>{t('empty.open')}</button>
    </div>
    {#if recent.length > 0}
      <div class="recent">
        <h4>{t('empty.recent')}</h4>
        {#each recent.slice(0, 3) as entry (entry.path)}
          <button class="row" type="button" onclick={() => onOpenRecent?.(entry)}>
            <span class="name">{entry.name}</span>
            <span class="meta">{metaOf(entry)}</span>
          </button>
        {/each}
      </div>
    {/if}
    <div class="hints">
      {#each hintItems as item (item.action)}
        <span><b>{item.combo}</b> {item.label}</span>
      {/each}
    </div>
    <p class="note">{t('empty.note')}</p>
  </div>
</div>

<style>
  .empty {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    background: var(--base);
    color: var(--muted);
    user-select: none;
  }

  .welcome {
    display: flex;
    flex-direction: column;
    width: min(520px, calc(100% - 64px));
    text-align: left;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .logo {
    display: flex;
    width: 30px;
    height: 30px;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--accent);
    border-radius: 8px;
    color: var(--accent);
    font-size: 15px;
    font-weight: 600;
  }

  .appname {
    color: var(--ink);
    font-size: 17px;
    font-weight: 600;
  }

  .tagline {
    margin: 10px 0 22px;
    font-size: 12px;
  }

  .actions {
    display: flex;
    gap: 10px;
    margin-bottom: 24px;
  }

  .open-btn {
    padding: 8px 20px;
    border: 1px solid var(--accent);
    border-radius: 6px;
    background: var(--accent);
    color: var(--surface);
    font: inherit;
    font-size: 13px;
    cursor: default;
    transition:
      background-color 90ms ease,
      border-color 90ms ease;
  }

  .open-btn:hover {
    background: color-mix(in srgb, var(--accent) 86%, #000000);
    border-color: color-mix(in srgb, var(--accent) 86%, #000000);
  }

  .open-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .recent {
    border-top: 1px solid var(--line);
    padding-top: 12px;
    margin-bottom: 4px;
  }

  .recent h4 {
    margin: 0 0 6px;
    color: var(--muted);
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.04em;
  }

  .row {
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 5px 8px;
    border: none;
    border-radius: 5px;
    background: transparent;
    font: inherit;
    text-align: left;
    cursor: default;
  }

  .row:hover {
    background: var(--hover);
  }

  .row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .row .name {
    overflow: hidden;
    color: var(--ink);
    font-size: 12.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row .meta {
    flex: none;
    color: var(--muted);
    font-size: 12px;
  }

  .hints {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
    border-top: 1px solid var(--line);
    padding-top: 12px;
    margin-top: 10px;
    font-size: 12px;
  }

  .hints b {
    color: var(--ink);
    font-weight: 500;
  }

  .note {
    margin: 12px 0 0;
    font-size: 12px;
    opacity: 0.85;
  }
</style>
