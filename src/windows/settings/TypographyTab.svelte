<!--
  TypographyTab — 阅读排版设置：主题 / 字体 / 字号 / 行高 / 限宽 / 边距。
  修改即存并广播主窗口实时应用；主题同时应用到设置窗口自身。
-->
<script lang="ts">
  import { settings } from './store.svelte';

  /** 主题选项（值 = 后端 ThemeChoice；system=跟随系统） */
  const THEMES = [
    { value: 'light', label: '浅色' },
    { value: 'dark', label: '深色' },
    { value: 'eye', label: '护眼' },
    { value: 'system', label: '跟随系统' },
  ] as const;

  /** 常见中文字体（可换成自定义：直接编辑配置文件 data/reader.json） */
  const FONTS = [
    'Microsoft YaHei',
    'SimSun',
    'SimHei',
    'KaiTi',
    'FangSong',
    'PingFang SC',
  ] as const;

  const reader = $derived(settings.snapshot?.reader ?? null);

  /** 字体下拉候选（当前值不在常见列表中时补一项，避免显示空选） */
  const fontOptions = $derived.by(() => {
    const current = reader?.typography.fontFamily;
    if (current && !(FONTS as readonly string[]).includes(current)) {
      return [current, ...FONTS];
    }
    return FONTS;
  });

  function readNumber(event: Event, fallback: number): number {
    const value = Number.parseFloat((event.target as HTMLInputElement).value);
    return Number.isFinite(value) ? value : fallback;
  }

  /** 切换主题：保存 + 立即应用到设置窗口自身 */
  function changeTheme(theme: string): void {
    void settings.saveReader({ theme });
    const resolved =
      theme === 'system'
        ? window.matchMedia('(prefers-color-scheme: dark)').matches
          ? 'dark'
          : 'light'
        : theme;
    document.documentElement.dataset.theme = resolved;
  }
</script>

{#if reader}
  <div class="rows">
    <label class="row">
      <span class="label">
        主题
        <small>阅读区与界面配色；「跟随系统」随系统深浅色切换</small>
      </span>
      <select value={reader.theme} onchange={(event) => changeTheme((event.target as HTMLSelectElement).value)}>
        {#each THEMES as item (item.value)}
          <option value={item.value}>{item.label}</option>
        {/each}
      </select>
    </label>
    <label class="row">
      <span class="label">
        字体
        <small>阅读区正文字体</small>
      </span>
      <select
        value={reader.typography.fontFamily}
        onchange={(event) =>
          void settings.saveReader({
            typography: { ...reader.typography, fontFamily: (event.target as HTMLSelectElement).value },
          })}
      >
        {#each fontOptions as font (font)}
          <option value={font}>{font}</option>
        {/each}
      </select>
    </label>
    <label class="row">
      <span class="label">
        字号
        <small>12–32 px（当前 {reader.typography.fontSize}px）</small>
      </span>
      <input
        type="range"
        min="12"
        max="32"
        step="1"
        value={reader.typography.fontSize}
        onchange={(event) =>
          void settings.saveReader({
            typography: {
              ...reader.typography,
              fontSize: Math.round(readNumber(event, reader.typography.fontSize)),
            },
          })}
      />
    </label>
    <label class="row">
      <span class="label">
        行高
        <small>正文行距倍数（1.2–2.6）</small>
      </span>
      <input
        type="number"
        min="1.2"
        max="2.6"
        step="0.1"
        value={reader.typography.lineHeight}
        onchange={(event) =>
          void settings.saveReader({
            typography: { ...reader.typography, lineHeight: readNumber(event, reader.typography.lineHeight) },
          })}
      />
    </label>
    <label class="row">
      <span class="label">
        正文限宽
        <small>阅读列最大宽度（480–1200 px），过宽影响阅读舒适度</small>
      </span>
      <input
        type="number"
        min="480"
        max="1200"
        step="10"
        value={reader.typography.contentWidth}
        onchange={(event) =>
          void settings.saveReader({
            typography: { ...reader.typography, contentWidth: readNumber(event, reader.typography.contentWidth) },
          })}
      />
    </label>
    <label class="row">
      <span class="label">
        左右边距
        <small>阅读区两侧留白（24–96 px）</small>
      </span>
      <input
        type="number"
        min="24"
        max="96"
        step="4"
        value={reader.typography.pagePadding}
        onchange={(event) =>
          void settings.saveReader({
            typography: { ...reader.typography, pagePadding: readNumber(event, reader.typography.pagePadding) },
          })}
      />
    </label>
  </div>
{:else}
  <p class="loading">正在载入配置…</p>
{/if}

<style>
  .rows {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--line);
    border-radius: 8px;
    overflow: hidden;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    background: var(--surface);
    font-size: 13px;
  }

  .row + .row {
    border-top: 1px solid var(--line);
  }

  .label {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .label small {
    color: var(--muted);
    font-size: 11.5px;
  }

  input[type='number'] {
    width: 90px;
    padding: 4px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--base);
    color: var(--ink);
  }

  input[type='range'] {
    width: 160px;
  }

  select {
    padding: 4px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--base);
    color: var(--ink);
  }

  .loading {
    margin-top: 40px;
    text-align: center;
    color: var(--muted);
    font-size: 13px;
  }
</style>
