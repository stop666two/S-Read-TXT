<!--
  RegistryPage — 注册表驱动的设置页（P0-4）。
  能力：按分组卡片渲染（可折叠）、全局搜索、单项/分组恢复默认；控件按 SettingKind 分派。
  契约：沿用 settings.css 的 .rows/.row/.label/.control 类；data-setting = 完整设置项 id；
        行级恢复按钮 data-setting-reset；分组恢复 data-setting-reset-group。
-->
<script lang="ts">
  import ConfirmDialog from '../../lib/components/ConfirmDialog.svelte';
  import { i18n, t, tOptional } from '../../lib/i18n/index.svelte';
  import type { MessageKey } from '../../lib/i18n/zh-CN';
  import type { ResetScope, SettingSpec } from '../../lib/ipc';
  import ChoiceRow from './parts/ChoiceRow.svelte';
  import ThemeRow from './parts/ThemeRow.svelte';
  import FontFamilyRow from './parts/FontFamilyRow.svelte';
  import BackgroundRow from './parts/BackgroundRow.svelte';
  import SliderRow from './parts/SliderRow.svelte';
  import ToggleRow from './parts/ToggleRow.svelte';
  import { buildPatch, getSettingValue } from './registry-util';
  import { settings } from './store.svelte';

  interface Props {
    /** 本页展示的分组 id 列表（搜索时忽略，改为全局搜索） */
    groups: readonly string[];
    /** 搜索关键词（空串 = 常规分组视图） */
    query?: string;
  }
  let { groups, query = '' }: Props = $props();

  const snapshot = $derived(settings.snapshot);
  const registry = $derived(settings.registry);
  const searching = $derived(query.trim().length > 0);

  /** 数值单位（注册表不含单位，按 id 映射；随语言环境变化） */
  const units = $derived.by(() => {
    const days = i18n.locale === 'en' ? 'days' : '天';
    const chars = i18n.locale === 'en' ? 'chars' : '字';
    return {
      'app.maxFileSizeMB': 'MB',
      'app.hardLimitMB': 'MB',
      'app.history.retentionDays': days,
      'reader.typography.fontSize': 'px',
      'reader.typography.lineHeight': '×',
      'reader.typography.contentWidth': 'px',
      'reader.typography.pagePadding': 'px',
      'reader.typography.pagePaddingY': 'px',
      'reader.typography.paragraphSpacing': 'px',
      'reader.typography.firstLineIndent': chars,
    } as Record<string, string>;
  });

  /** 枚举标签键（按设置项 id + 枚举值映射语言包；缺省显示原值） */
  const ENUM_LABEL_KEYS: Record<string, Record<string, MessageKey>> = {
    'app.logLevel': {
      error: 'setting.enum.logLevel.error',
      warn: 'setting.enum.logLevel.warn',
      info: 'setting.enum.logLevel.info',
      debug: 'setting.enum.logLevel.debug',
    },
    'app.locale': { 'zh-CN': 'setting.enum.locale.zh-CN', en: 'setting.enum.locale.en' },
  'app.editor.lines.defaultScope': {
    all: 'setting.enum.lineScope.all',
    currentLine: 'setting.enum.lineScope.currentLine',
    rowRange: 'setting.enum.lineScope.rowRange',
    nonEmpty: 'setting.enum.lineScope.nonEmpty',
    selection: 'setting.enum.lineScope.selection',
  },
  'app.editor.lines.sortMode': {
    lex: 'setting.enum.sortMode.lex',
    natural: 'setting.enum.sortMode.natural',
    length: 'setting.enum.sortMode.length',
  },
  'app.editor.lines.dedupeMode': {
    keepFirst: 'setting.enum.dedupeMode.keepFirst',
    keepLast: 'setting.enum.dedupeMode.keepLast',
  },
    'app.editor.multiCursor.rectModifier': {
      alt: 'setting.enum.rectModifier.alt',
      ctrlAlt: 'setting.enum.rectModifier.ctrlAlt',
    },
    'app.editor.lines.indentStyle': {
    spaces: 'setting.enum.indentStyle.spaces',
    tab: 'setting.enum.indentStyle.tab',
  },
  'app.editor.lines.caseDefault': {
    upper: 'setting.enum.caseMode.upper',
    lower: 'setting.enum.caseMode.lower',
    title: 'setting.enum.caseMode.title',
  },
    'reader.typography.textAlign': {
      left: 'setting.enum.align.left',
      justify: 'setting.enum.align.justify',
    },
  };

  /** 设置项显示名（语言包缺失时回退 id） */
  function specLabel(spec: SettingSpec): string {
    return tOptional(`setting.${spec.id}`) || spec.id;
  }

  /** 设置项描述（语言包缺失时为空串） */
  function specDesc(spec: SettingSpec): string {
    return tOptional(`setting.${spec.id}.desc`);
  }

  /** 搜索匹配：按当前语言的名称/描述/id（快捷键组无通用行，排除） */
  function matches(spec: SettingSpec): boolean {
    if (spec.group === 'shortcuts') return false;
    if (!searching) return true;
    const haystack = `${specLabel(spec)} ${specDesc(spec)} ${spec.id}`.toLowerCase();
    return haystack.includes(query.trim().toLowerCase());
  }

  const visibleGroups = $derived.by(() => {
    const source = searching ? [...new Set(registry.map((spec) => spec.group))] : groups;
    return source
      .map((group) => ({
        id: group,
        specs: registry.filter((spec) => spec.group === group && matches(spec)),
      }))
      .filter((group) => group.specs.length > 0);
  });

  /** 分组折叠状态（本地；默认展开） */
  let collapsed = $state<Record<string, boolean>>({});

  /** 分组恢复默认的确认目标（单项恢复无确认、直接执行） */
  let confirmGroup = $state<string | null>(null);

  function applyScope(scope: ResetScope): void {
    void settings.resetScope(scope);
  }

  /** 提交新值（app 段与 reader 段分别走对应保存通道） */
  function commit(spec: SettingSpec, value: unknown): void {
    if (!snapshot) return;
    if (spec.id.startsWith('app.')) {
      void settings.saveApp(buildPatch(snapshot.app, spec.id, value));
    } else {
      void settings.saveReaderNow(buildPatch(snapshot.reader, spec.id, value));
    }
  }

  /** 拖动实时预览（仅 reader 数值项；节流落盘，主窗口即时生效） */
  function live(spec: SettingSpec, value: unknown): void {
    if (!snapshot || !spec.id.startsWith('reader.')) return;
    settings.saveReaderLive(buildPatch(snapshot.reader, spec.id, value));
  }

  /** 滑块步进：整数项 1；小数项（行高）0.05 */
  function numStep(kind: { min: number; max: number; integer: boolean }): number {
    if (kind.integer) return 1;
    return kind.max - kind.min <= 3 ? 0.05 : 1;
  }

  function numberValue(spec: SettingSpec): number {
    const raw = snapshot ? getSettingValue(snapshot, spec.id) : 0;
    return typeof raw === 'number' ? raw : 0;
  }

  function boolValue(spec: SettingSpec): boolean {
    const raw = snapshot ? getSettingValue(snapshot, spec.id) : false;
    return raw === true;
  }

  function enumOptions(spec: SettingSpec): { value: string; label: string }[] {
    if (spec.kind.type !== 'enum') return [];
    const mapping = ENUM_LABEL_KEYS[spec.id] ?? {};
    return spec.kind.values.map((value) => {
      const key = mapping[value];
      return { value, label: key ? t(key) : value };
    });
  }
</script>

{#if !snapshot || registry.length === 0}
  <p class="loading">{t('settings.loading')}</p>
{:else if searching && visibleGroups.length === 0}
  <p class="empty">{t('settings.search.noResults')}</p>
{:else}
  {#if searching}
    <p class="search-hint">{t('settings.search.globalHint')}</p>
  {/if}
  {#each visibleGroups as group (group.id)}
    <section class="group" data-setting-group={group.id}>
      {#if !searching}
        <header class="group-head">
          <button
            class="collapse"
            type="button"
            aria-expanded={!collapsed[group.id]}
            aria-label={collapsed[group.id] ? t('settings.expand') : t('settings.collapse')}
            onclick={() => (collapsed[group.id] = !collapsed[group.id])}
          >
            <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true">
              <path
                d="M4 6l4 4 4-4"
                fill="none"
                stroke="currentColor"
                stroke-width="1.4"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>
          <h3>{tOptional(`settingGroup.${group.id}`) || group.id}</h3>
          <button
            class="ghost"
            type="button"
            data-setting-reset-group={group.id}
            onclick={() => (confirmGroup = group.id)}
          >
            {t('settings.resetGroup')}
          </button>
        </header>
      {/if}
      {#if searching || !collapsed[group.id]}
        <div class="rows">
          {#each group.specs as spec (spec.id)}
            {@const label = specLabel(spec)}
            {@const desc = specDesc(spec)}
            {#if spec.id === 'reader.theme'}
              <ThemeRow
                {label}
                {desc}
                setting={spec.id}
                onReset={() => applyScope({ kind: 'field', id: spec.id })}
                resetLabel={t('settings.resetField')}
              />
            {:else if spec.id === 'reader.typography.fontFamily'}
              <FontFamilyRow
                {label}
                {desc}
                value={String(getSettingValue(snapshot, spec.id) ?? '')}
                setting={spec.id}
                onCommit={(value) => commit(spec, value)}
                onReset={() => applyScope({ kind: 'field', id: spec.id })}
                resetLabel={t('settings.resetField')}
              />
            {:else if spec.id === 'reader.background.file'}
              <BackgroundRow
                {label}
                {desc}
                value={snapshot.reader.background}
                onReset={() => applyScope({ kind: 'field', id: spec.id })}
                resetLabel={t('settings.resetField')}
              />
            {:else if spec.kind.type === 'number'}
              <SliderRow
                {label}
                {desc}
                value={numberValue(spec)}
                min={spec.kind.min}
                max={spec.kind.max}
                step={numStep(spec.kind)}
                unit={units[spec.id] ?? ''}
                setting={spec.id}
                onLive={(value) => live(spec, value)}
                onCommit={(value) => commit(spec, value)}
                onReset={() => applyScope({ kind: 'field', id: spec.id })}
                resetLabel={t('settings.resetField')}
              />
            {:else if spec.kind.type === 'bool'}
              <ToggleRow
                {label}
                {desc}
                checked={boolValue(spec)}
                setting={spec.id}
                onCommit={(value) => commit(spec, value)}
                onReset={() => applyScope({ kind: 'field', id: spec.id })}
                resetLabel={t('settings.resetField')}
              />
            {:else if spec.kind.type === 'enum'}
              <ChoiceRow
                {label}
                {desc}
                value={String(getSettingValue(snapshot, spec.id) ?? '')}
                options={enumOptions(spec)}
                setting={spec.id}
                onCommit={(value) => commit(spec, value)}
                onReset={() => applyScope({ kind: 'field', id: spec.id })}
                resetLabel={t('settings.resetField')}
              />
            {/if}
          {/each}
        </div>
      {/if}
    </section>
  {/each}
{/if}

{#if confirmGroup}
  <ConfirmDialog
    open={true}
    title={t('settings.resetGroupTitle')}
    message={t('settings.resetGroupMessage', {
      group: tOptional(`settingGroup.${confirmGroup}`) || confirmGroup,
    })}
    confirmLabel={t('settings.resetGroupConfirm')}
    onCancel={() => (confirmGroup = null)}
    onConfirm={() => {
      const target = confirmGroup;
      confirmGroup = null;
      if (target) applyScope({ kind: 'group', name: target });
    }}
  />
{/if}
