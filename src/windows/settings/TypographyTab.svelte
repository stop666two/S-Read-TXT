<!--
  TypographyTab — 阅读排版：主题与字体、正文排版参数（全部滑块）。
  范围与 `src-tauri/src/settings/defaults.rs` 的常量保持一致（后端会再次归一兜底）。
  滑块拖动实时预览（节流落盘），松开/数字框确认立即落盘。
-->
<script lang="ts">
  import type { TypographySettings } from '../../lib/ipc';
  import ChoiceRow from './parts/ChoiceRow.svelte';
  import SliderRow from './parts/SliderRow.svelte';
  import { settings } from './store.svelte';

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

  /** 可调数值项（键对应 `TypographySettings` 字段） */
  type NumberKey = 'fontSize' | 'lineHeight' | 'contentWidth' | 'pagePadding' | 'pagePaddingY';
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
  ];

  const reader = $derived(settings.snapshot?.reader ?? null);
  const typo = $derived(reader?.typography ?? null);

  /** 字体候选（含当前值，去重） */
  const fontOptions = $derived(
    typo && !FONT_CANDIDATES.includes(typo.fontFamily)
      ? [
          { value: typo.fontFamily, label: typo.fontFamily },
          ...FONT_CANDIDATES.map((name) => ({ value: name, label: name })),
        ]
      : FONT_CANDIDATES.map((name) => ({ value: name, label: name })),
  );

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
      desc="系统已安装的中文字体"
      value={typo.fontFamily}
      options={fontOptions}
      setting="fontFamily"
      onCommit={(value) => patchNow({ fontFamily: value })}
    />
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
  </div>
{:else}
  <p class="loading">正在载入配置…</p>
{/if}
