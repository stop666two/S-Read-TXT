<!--
  TypographyTab — 阅读排版：主题与字体（含自定义字体导入/管理）、正文排版参数。
  范围与 `src-tauri/src/settings/defaults.rs` 的常量保持一致（后端会再次归一兜底）。
  滑块拖动实时预览（节流落盘），松开/数字框确认立即落盘；字体导入后自动切换使用。
-->
<script lang="ts">
  import { onMount } from 'svelte';

  import { ask, open as openDialog } from '@tauri-apps/plugin-dialog';

  import { formatBytes } from '../../lib/format';
  import {
    describeIpcError,
    ipc,
    toIpcError,
    type FontEntry,
    type TypographySettings,
  } from '../../lib/ipc';
  import { toasts } from '../../lib/state/toasts.svelte';
  import ChoiceRow from './parts/ChoiceRow.svelte';
  import SliderRow from './parts/SliderRow.svelte';
  import ToggleRow from './parts/ToggleRow.svelte';
  import { settings } from './store.svelte';

  /** 自定义字体值前缀（与 App.svelte 的解析约定一致） */
  const CUSTOM_PREFIX = 'custom:';

  /** 主题选项（system 由窗口监听系统明暗解析） */
  const THEMES = [
    { value: 'system', label: '跟随系统' },
    { value: 'light', label: '浅色' },
    { value: 'dark', label: '深色' },
    { value: 'eye', label: '护眼' },
  ];

  /** 常见中文字体候选（当前值不在列表时动态补入，避免选择丢失） */
  const FONT_CANDIDATES = [
    'Microsoft YaHei',
    'SimSun',
    'SimHei',
    'KaiTi',
    'FangSong',
    'Noto Sans CJK SC',
    'Source Han Sans SC',
  ];

  /** 文字对齐选项（与后端 TextAlign 序列化一致） */
  const ALIGNS = [
    { value: 'left', label: '左对齐' },
    { value: 'justify', label: '两端对齐' },
  ];

  /** 可调数值项（键对应 `TypographySettings` 字段） */
  type NumberKey =
    | 'fontSize'
    | 'lineHeight'
    | 'contentWidth'
    | 'pagePadding'
    | 'pagePaddingY'
    | 'paragraphSpacing'
    | 'firstLineIndent';
  interface SliderDef {
    key: NumberKey;
    label: string;
    desc: string;
    min: number;
    max: number;
    step: number;
    unit: string;
  }

  /** 滑块定义（范围与后端 defaults.rs 同步） */
  const SLIDERS: SliderDef[] = [
    { key: 'fontSize', label: '字号', desc: '正文字号（8–72 px）', min: 8, max: 72, step: 1, unit: 'px' },
    { key: 'lineHeight', label: '行高', desc: '行高倍数（1.0–3.2 ×）', min: 1, max: 3.2, step: 0.05, unit: '×' },
    { key: 'contentWidth', label: '限宽', desc: '正文列宽上限（320–2400 px）', min: 320, max: 2400, step: 20, unit: 'px' },
    { key: 'pagePadding', label: '左右边距', desc: '阅读区左右留白（0–240 px）', min: 0, max: 240, step: 4, unit: 'px' },
    { key: 'pagePaddingY', label: '上下边距', desc: '阅读区上下留白（0–240 px）', min: 0, max: 240, step: 4, unit: 'px' },
    { key: 'paragraphSpacing', label: '段间距', desc: '段落之间的额外留白（0–64 px）', min: 0, max: 64, step: 1, unit: 'px' },
    { key: 'firstLineIndent', label: '首行缩进', desc: '每段首行缩进字符数（0–8 字）', min: 0, max: 8, step: 1, unit: '字' },
  ];

  const reader = $derived(settings.snapshot?.reader ?? null);
  const typo = $derived(reader?.typography ?? null);

  /** 已导入的自定义字体（onMount 拉取；导入/删除后刷新） */
  let fonts = $state<FontEntry[]>([]);

  async function refreshFonts(): Promise<void> {
    try {
      fonts = await ipc.listFonts();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  onMount(() => {
    void refreshFonts();
  });

  /** 字体候选（系统候选 + 自定义；当前值不在列表时动态补入） */
  const fontOptions = $derived.by(() => {
    const options = [
      ...FONT_CANDIDATES.map((name) => ({ value: name, label: name })),
      ...fonts.map((font) => ({
        value: `${CUSTOM_PREFIX}${font.fileName}`,
        label: `${font.label}（自定义）`,
      })),
    ];
    if (typo && !options.some((option) => option.value === typo.fontFamily)) {
      options.unshift({ value: typo.fontFamily, label: typo.fontFamily });
    }
    return options;
  });

  /** 导入字体：选择文件 → 复制入库 → 刷新列表并切换使用 */
  async function importFont(): Promise<void> {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: '字体文件', extensions: ['ttf', 'otf', 'woff', 'woff2'] }],
    });
    if (typeof picked !== 'string' || !typo) return;
    try {
      const entry = await ipc.importFont(picked);
      await refreshFonts();
      patchNow({ fontFamily: `${CUSTOM_PREFIX}${entry.fileName}` });
      toasts.show(`字体「${entry.label}」已导入并应用`);
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 删除自定义字体（确认后；若正在使用则回退默认字体） */
  async function removeFont(font: FontEntry): Promise<void> {
    const confirmed = await ask(`确定删除字体「${font.label}」？删除后无法恢复。`, {
      title: '删除字体',
      kind: 'warning',
    });
    if (!confirmed || !typo) return;
    try {
      await ipc.removeFont(font.fileName);
      if (typo.fontFamily === `${CUSTOM_PREFIX}${font.fileName}`) {
        patchNow({ fontFamily: 'Microsoft YaHei' });
      }
      await refreshFonts();
      toasts.show(`字体「${font.label}」已删除`);
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 构造类型安全的排版补丁（键 → 字段） */
  function typoPatch(key: NumberKey, value: number): Partial<TypographySettings> {
    switch (key) {
      case 'fontSize':
        return { fontSize: value };
      case 'lineHeight':
        return { lineHeight: value };
      case 'contentWidth':
        return { contentWidth: value };
      case 'pagePadding':
        return { pagePadding: value };
      case 'pagePaddingY':
        return { pagePaddingY: value };
      case 'paragraphSpacing':
        return { paragraphSpacing: value };
      case 'firstLineIndent':
        return { firstLineIndent: value };
    }
  }

  /** 拖动中：实时预览（节流落盘，主窗口即时生效） */
  function patchLive(next: Partial<TypographySettings>): void {
    if (!typo) return;
    settings.saveReaderLive({ typography: { ...typo, ...next } });
  }

  /** 确认：立即落盘 */
  function patchNow(next: Partial<TypographySettings>): void {
    if (!typo) return;
    void settings.saveReaderNow({ typography: { ...typo, ...next } });
  }
</script>

{#if reader && typo}
  <p class="section-title">主题与字体</p>
  <div class="rows">
    <ChoiceRow
      label="主题"
      desc="阅读界面配色（跟随系统读取窗口明暗）"
      value={reader.theme}
      options={THEMES}
      setting="theme"
      onCommit={(value) => void settings.saveReader({ theme: value })}
    />
    <ChoiceRow
      label="正文字体"
      desc="系统字体或已导入的自定义字体"
      value={typo.fontFamily}
      options={fontOptions}
      setting="fontFamily"
      onCommit={(value) => patchNow({ fontFamily: value })}
    />
  </div>

  <p class="section-title">自定义字体</p>
  <div class="rows">
    <div class="row">
      <span class="label">导入字体文件<small>支持 ttf / otf / woff / woff2（单文件 ≤64MB；导入后立即切换使用）</small></span>
      <span class="control">
        <button class="btn" type="button" data-setting="importFont" onclick={() => void importFont()}>
          导入字体…
        </button>
      </span>
    </div>
    {#each fonts as font (font.fileName)}
      <div class="row">
        <span class="label">{font.label}<small>{font.fileName} · {formatBytes(font.sizeBytes)}</small></span>
        <span class="control">
          <button
            class="btn danger"
            type="button"
            data-setting={`font-${font.fileName}`}
            onclick={() => void removeFont(font)}
          >
            删除
          </button>
        </span>
      </div>
    {/each}
    {#if fonts.length === 0}
      <div class="row">
        <span class="label">尚未导入自定义字体<small>导入后会出现在上方「正文字体」列表中</small></span>
        <span class="control"></span>
      </div>
    {/if}
  </div>

  <p class="section-title">正文排版</p>
  <div class="rows">
    {#each SLIDERS as item (item.key)}
      <SliderRow
        label={item.label}
        desc={item.desc}
        value={typo[item.key]}
        min={item.min}
        max={item.max}
        step={item.step}
        unit={item.unit}
        setting={item.key}
        onLive={(value) => patchLive(typoPatch(item.key, value))}
        onCommit={(value) => patchNow(typoPatch(item.key, value))}
      />
    {/each}
    <ChoiceRow
      label="文字对齐"
      desc="两端对齐可能拉伸字间距（中文小说通常选左对齐）"
      value={typo.textAlign}
      options={ALIGNS}
      setting="textAlign"
      onCommit={(value) => patchNow({ textAlign: value as TypographySettings['textAlign'] })}
    />
    <ToggleRow
      label="翻页平滑滚动"
      desc="PgUp / PgDn 翻页时使用平滑动画（跳转首尾仍为瞬时）"
      checked={typo.smoothScroll}
      setting="smoothScroll"
      onCommit={(checked) => patchNow({ smoothScroll: checked })}
    />
  </div>
{:else}
  <p class="loading">正在载入配置…</p>
{/if}

<style>
  .btn {
    padding: 5px 14px;
    border: 1px solid var(--line);
    border-radius: 7px;
    background: var(--surface);
    color: var(--ink);
    font-size: 12px;
    cursor: pointer;
    transition:
      border-color 90ms ease,
      color 90ms ease;
  }

  .btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .btn.danger {
    color: #b0413e;
  }

  .btn.danger:hover {
    border-color: #b0413e;
    color: #b0413e;
  }
</style>
