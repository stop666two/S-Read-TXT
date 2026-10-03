<script lang="ts">
// 工具栏：打开 / 历史（阶段 6 起可用）/ 编码（共享下拉）/ 主题（循环）/ 设置。
  import { t } from '../i18n/index.svelte';
  import type { MessageKey } from '../i18n/zh-CN';
  import EncodingMenu from './EncodingMenu.svelte';
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
    /** 设置窗口回调（打开独立设置窗口） */
    onSettings?: () => void;
    /** 是否处于编辑模式（编辑按钮激活态） */
    editing: boolean;
    /** 是否只读（文件超过只读阈值：编辑按钮禁用并给出提示） */
    readOnly?: boolean;
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
    readOnly = false,
    canSave,
    onToggleEdit,
    onSave,
  }: Props = $props();

  /** 主题循环顺序（含跟随系统） */
  const themeCycle: ThemeChoice[] = ['light', 'dark', 'eye', 'system'];
  /** 主题名 → 语言包键（提示文案由 t() 渲染） */
  const themeLabelKeys: Record<ThemeChoice, MessageKey> = {
    light: 'theme.light',
    dark: 'theme.dark',
    eye: 'theme.eye',
    system: 'theme.system',
  };

  /** 循环切换主题 */
  function cycleTheme(): void {
    const index = themeCycle.indexOf(themeChoice);
    onThemeChange(themeCycle[(index + 1) % themeCycle.length]);
  }
</script>

<div class="toolbar">
  <button class="icon-btn" title={t('toolbar.openHint')} aria-label={t('toolbar.open')} onclick={() => onOpenFile?.()}>
    <Icon name="open" />
  </button>
  <button class="icon-btn" title={t('toolbar.historyHint')} aria-label={t('toolbar.history')} onclick={() => onHistory?.()}>
    <Icon name="history" />
  </button>
  <div class="sep"></div>
  <button
    class="icon-btn"
    class:active={editing}
    title={readOnly
      ? t('toolbar.editReadOnlyHint')
      : editing
        ? t('toolbar.editDisableHint')
        : t('toolbar.editEnableHint')}
    aria-label={t('toolbar.toggleEdit')}
    aria-pressed={editing}
    disabled={readOnly}
    onclick={() => onToggleEdit?.()}
  >
    <Icon name="edit" />
  </button>
  <button
    class="icon-btn"
    title={t('toolbar.saveHint')}
    aria-label={t('toolbar.save')}
    disabled={!canSave}
    onclick={() => onSave?.()}
  >
    <Icon name="save" />
  </button>
  <div class="sep"></div>
  <EncodingMenu
    displayLabel={t('toolbar.encoding', { value: encodingOverride ?? t('toolbar.encodingAuto') })}
    {encodings}
    override={encodingOverride}
    onPick={onEncodingChange}
  />
  <div class="sep"></div>
  <button
    class="icon-btn"
    title={t('toolbar.theme', { name: t(themeLabelKeys[themeChoice]) })}
    aria-label={t('toolbar.themeCycle')}
    onclick={cycleTheme}
  >
    <Icon name="palette" />
  </button>
  <button class="icon-btn with-text" title={t('toolbar.settings')} aria-label={t('toolbar.settings')} onclick={() => onSettings?.()}>
    <Icon name="settings" />
    <span>{t('toolbar.settings')}</span>
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

  /* 带文字的按钮（如「设置」）：图标 + 文本，宽度自适应 */
  .icon-btn.with-text {
    width: auto;
    gap: 5px;
    padding: 0 8px;
    font-size: 12.5px;
  }

  .icon-btn.active {
    background: var(--hover);
    color: var(--accent);
  }

  .icon-btn:disabled {
    color: var(--muted);
    opacity: 0.55;
  }

  .sep {
    width: 1px;
    height: 18px;
    margin: 0 4px;
    background: var(--line);
  }
</style>
