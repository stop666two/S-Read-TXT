<script lang="ts">
  // 菜单栏（应用内自绘）：文件 / 编辑 / 查看 / 帮助。
  // 阶段 2b：完成结构、下拉交互与主题动作；其余动作在后续切片接线（回调缺省即无操作）。
  // 交互约定：点击标题开合；已有菜单打开时悬停切换；点击菜单项执行；Esc / 点击外部关闭。
  import { t } from '../i18n/index.svelte';
  import type { EditActionType } from '../edit/actions';
  import type { HistoryEntry } from '../ipc';

  interface Props {
    /** 当前主题 id（用于「查看」菜单的单选标记） */
    themeId: string;
    /** 主题菜单条目（App 按当前语言构建） */
    themeEntries: { id: string; label: string }[];
    /** 主题切换回调 */
    onThemeChange: (id: string) => void;
    /** 打开文件回调（阶段 2c 接线） */
    onOpenFile?: () => void;
    /** 退出回调（阶段 2c 接线） */
    onQuit?: () => void;
    /** 工作区（多文件）查找与替换回调（P1-8） */
    onWorkspaceFind?: () => void;
    /** 多文件搜索是否开启（false 时菜单项禁用） */
    workspaceFindEnabled?: boolean;
    /** 是否处于编辑模式（编辑菜单文案与可用性） */
    editing: boolean;
    /** 是否只读（文件超过只读阈值：编辑模式项禁用并给出提示） */
    readOnly?: boolean;
    /** 是否有未保存修改（保存项可用性） */
    dirty: boolean;
    /** 编辑模式切换回调 */
    onToggleEdit?: () => void;
    /** 保存回调 */
    onSave?: () => void;
    /** 另存为回调（仅编辑态可用） */
    onSaveAs?: () => void;
    /** 重新加载回调（文件菜单） */
    onReload?: () => void;
    /** 编辑动作分发（撤销/重做/剪贴板/全选/查找/替换） */
    onEditorAction?: (type: EditActionType) => void;
    /** 全屏切换回调（查看菜单） */
    onToggleFullscreen?: () => void;
  /** 自动滚动开关状态（查看菜单，勾选标记） */
  autoScroll?: boolean;
  /** 切换自动滚动 */
  onToggleAutoScroll?: () => void;
  /** 专注模式状态（查看菜单） */
  focusMode?: boolean;
  /** 切换专注模式 */
  onToggleFocusMode?: () => void;
  /** 打字机模式状态（查看菜单） */
  typewriter?: boolean;
  /** 切换打字机模式 */
  onToggleTypewriter?: () => void;
  /** 番茄钟进行中（影响菜单项文案） */
  pomodoroOn?: boolean;
  /** 开始/停止番茄钟 */
  onTogglePomodoro?: () => void;
    /** 字号增大（查看菜单；步进由 App 归一后保存） */
    onFontIncrease?: () => void;
    /** 字号减小（查看菜单） */
    onFontDecrease?: () => void;
    /** 重置字号（查看菜单；回默认 16px） */
    onFontReset?: () => void;
    /** 打开设置窗口并定位「快捷键」页签（帮助菜单） */
    onOpenShortcuts?: () => void;
    /** 打开设置窗口并定位「关于」页签（帮助菜单） */
    onOpenAbout?: () => void;
    /** 最近打开条目（≤10；空数组时「最近打开」禁用） */
    recent?: HistoryEntry[];
    /** 打开最近条目（含进度恢复） */
    onOpenRecent?: (entry: HistoryEntry) => void;
    /** 打开历史记录面板 */
    onOpenHistory?: () => void;
  onToggleBookmark?: () => void;
  onAnnotationsPanel?: () => void;
  onClearAnnotations?: () => void;
    /** 打开设置窗口（文件菜单） */
    onSettings?: () => void;
    /** 是否存在活动标签（重新加载可用性） */
    hasTab: boolean;
  }
  let {
    themeId,
    themeEntries,
    onThemeChange,
    onOpenFile,
    onQuit,
    editing,
    readOnly = false,
    dirty,
    onToggleEdit,
    onSave,
    onSaveAs,
    onReload,
    onEditorAction,
    onToggleFullscreen,
  autoScroll,
  onToggleAutoScroll,
  focusMode,
  onToggleFocusMode,
  typewriter,
  onToggleTypewriter,
  pomodoroOn,
  onTogglePomodoro,
    onFontIncrease,
    onFontDecrease,
    onFontReset,
    onOpenShortcuts,
    onOpenAbout,
    recent = [],
    onOpenRecent,
    onOpenHistory,
  onToggleBookmark,
  onAnnotationsPanel,
  onClearAnnotations,
    onWorkspaceFind,
    workspaceFindEnabled = true,
    onSettings,
    hasTab,
  }: Props = $props();

  /** 菜单名联合类型 */
  type MenuName = 'file' | 'edit' | 'view' | 'help';
  /** 当前展开的菜单（null = 全部收起） */
  let openMenu = $state<MenuName | null>(null);
  /** 「最近打开」子菜单展开（悬停/点击切换） */
  let recentOpen = $state(false);
  /** 「复制为」子菜单展开（悬停控制；P1-5） */
  let copyAsOpen = $state(false);
  /** 清理子菜单展开态（P1-7）。 */
  let cleanupOpen = $state(false);
  let annotOpen = $state(false);

  /** 切换菜单开合 */
  function toggle(name: MenuName): void {
    openMenu = openMenu === name ? null : name;
  }

  /** 悬停切换（仅在已有菜单展开时，符合桌面菜单习惯） */
  function hoverSwitch(name: MenuName): void {
    if (openMenu !== null) openMenu = name;
  }

  /** 关闭全部菜单 */
  function close(): void {
    openMenu = null;
  }

  /** 执行动作并关闭菜单 */
  function run(action: (() => void) | undefined): void {
    close();
    action?.();
  }
</script>

<svelte:window
  onclick={(event) => {
    // 点击菜单栏区域外才关闭（替代逐元素 stopPropagation，避免 a11y 告警）
    const target = event.target as HTMLElement | null;
    if (!target?.closest('.menu-bar')) close();
  }}
  onkeydown={(event) => {
    if (event.key === 'Escape') close();
  }}
/>

<nav class="menu-bar" aria-label={t('menu.aria')}>
  <button class="title" class:open={openMenu === 'file'} onclick={(e) => { e.stopPropagation(); toggle('file'); }} onmouseenter={() => hoverSwitch('file')}>{t('menu.file')}</button>
  <button class="title" class:open={openMenu === 'edit'} onclick={(e) => { e.stopPropagation(); toggle('edit'); }} onmouseenter={() => hoverSwitch('edit')}>{t('menu.edit')}</button>
  <button class="title" class:open={openMenu === 'view'} onclick={(e) => { e.stopPropagation(); toggle('view'); }} onmouseenter={() => hoverSwitch('view')}>{t('menu.view')}</button>
  <button class="title" class:open={openMenu === 'help'} onclick={(e) => { e.stopPropagation(); toggle('help'); }} onmouseenter={() => hoverSwitch('help')}>{t('menu.help')}</button>

  {#if openMenu === 'file'}
    <div class="dropdown" role="menu" style="left: 4px">
      <button class="item" onclick={() => run(onOpenFile)}><span>{t('menu.file.open')}</span><span class="hint">Ctrl+O</span></button>
      <button class="item" disabled={!hasTab} onclick={() => run(onReload)}><span>{t('menu.file.reload')}</span></button>
      <div class="separator"></div>
      <div
        class="submenu-wrap"
        role="presentation"
        onmouseenter={() => (recentOpen = true)}
        onmouseleave={() => (recentOpen = false)}
      >
        <button
          class="item"
          disabled={recent.length === 0}
          aria-haspopup="menu"
          aria-expanded={recentOpen}
          onclick={() => (recentOpen = !recentOpen)}
        >
          <span>{t('menu.file.recent')}</span>
          <span class="arrow">▸</span>
        </button>
        {#if recentOpen && recent.length > 0}
          <div class="flyout" role="menu" aria-label={t('menu.file.recentAria')}>
            {#each recent as entry (entry.path)}
              <button class="item" title={entry.path} onclick={() => run(() => onOpenRecent?.(entry))}>
                <span class="recent-name">{entry.name}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
      <button class="item" onclick={() => run(() => onOpenHistory?.())}>
        <span>{t('menu.file.history')}</span>
        <span class="hint">Ctrl+Shift+H</span>
      </button>
      <button class="item" onclick={() => run(onSettings)}><span>{t('menu.file.settings')}</span></button>
      <div class="separator"></div>
      <button class="item" onclick={() => run(onQuit)}><span>{t('menu.file.quit')}</span></button>
    </div>
  {:else if openMenu === 'edit'}
    <div class="dropdown" role="menu" style="left: 46px">
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('undo'))}>
        <span>{t('menu.edit.undo')}</span>
        <span class="hint">Ctrl+Z</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('redo'))}>
        <span>{t('menu.edit.redo')}</span>
        <span class="hint">Ctrl+Y</span>
      </button>
      <div class="separator"></div>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('cut'))}>
        <span>{t('menu.edit.cut')}</span>
        <span class="hint">Ctrl+X</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('copy'))}>
        <span>{t('menu.edit.copy')}</span>
        <span class="hint">Ctrl+C</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('paste'))}>
        <span>{t('menu.edit.paste')}</span>
        <span class="hint">Ctrl+V</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('selectAll'))}>
        <span>{t('menu.edit.selectAll')}</span>
        <span class="hint">Ctrl+A</span>
      </button>
      <div class="separator"></div>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('find'))}>
        <span>{t('menu.edit.find')}</span>
        <span class="hint">Ctrl+F</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('replace'))}>
        <span>{t('menu.edit.replace')}</span>
        <span class="hint">Ctrl+H</span>
      </button>
      <button
        class="item"
        disabled={workspaceFindEnabled === false}
        onclick={() => run(() => onWorkspaceFind?.())}
      >
        <span>{t('menu.edit.workspace')}</span>
      </button>
      <button
        class="item"
        disabled={!editing}
        onclick={() => run(() => onEditorAction?.('batchNumbering'))}
      >
        <span>{t('menu.edit.batchNumbering')}</span>
      </button>
      <button
        class="item"
        disabled={!editing}
        onclick={() => run(() => onEditorAction?.('lineOps'))}
      >
        <span>{t('menu.edit.lineOps')}</span>
      </button>
      <div
        class="submenu-wrap"
        role="presentation"
        onmouseenter={() => (annotOpen = true)}
        onmouseleave={() => (annotOpen = false)}
      >
        <button
          class="item"
          aria-haspopup="menu"
          aria-expanded={annotOpen}
          onclick={() => (annotOpen = !annotOpen)}
        >
          <span>{t('menu.edit.annotations')}</span>
          <span class="arrow">▸</span>
        </button>
        {#if annotOpen}
          <div class="flyout" role="menu" aria-label={t('menu.edit.annotations')}>
            <button class="item" onclick={() => run(() => onToggleBookmark?.())}>
              <span>{t('menu.edit.toggleBookmark')}</span>
            </button>
            <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('highlightSelection'))}>
              <span>{t('menu.edit.highlightSelection')}</span>
            </button>
            <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('addNote'))}>
              <span>{t('menu.edit.addNote')}</span>
            </button>
            <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('addTodo'))}>
              <span>{t('menu.edit.addTodo')}</span>
            </button>
            <div class="sep" role="separator"></div>
            <button class="item" onclick={() => run(() => onAnnotationsPanel?.())}>
              <span>{t('menu.edit.annotationsPanel')}</span>
            </button>
            <button class="item" onclick={() => run(() => onClearAnnotations?.())}>
              <span>{t('menu.edit.clearAnnotations')}</span>
            </button>
          </div>
        {/if}
      </div>
      <button
        class="item"
        disabled={!editing}
        onclick={() => run(() => onEditorAction?.('insertTimestamp'))}
      >
        <span>{t('menu.edit.insertTimestamp')}</span>
      </button>
      <div
        class="submenu-wrap"
        role="presentation"
        onmouseenter={() => (cleanupOpen = true)}
        onmouseleave={() => (cleanupOpen = false)}
      >
        <button
          class="item"
          disabled={!editing}
          aria-haspopup="menu"
          aria-expanded={cleanupOpen}
          onclick={() => (cleanupOpen = !cleanupOpen)}
        >
          <span>{t('menu.edit.cleanup')}</span>
          <span class="arrow">▸</span>
        </button>
        {#if cleanupOpen}
          <div class="flyout" role="menu" aria-label={t('menu.edit.cleanup')}>
            <button class="item" onclick={() => run(() => onEditorAction?.('cleanupTrailingWhitespace'))}>
              <span>{t('cleanup.trailingWhitespace')}</span>
            </button>
            <button class="item" onclick={() => run(() => onEditorAction?.('cleanupCollapseBlankLines'))}>
              <span>{t('cleanup.collapseBlankLines')}</span>
            </button>
            <button class="item" onclick={() => run(() => onEditorAction?.('cleanupTrailingNewline'))}>
              <span>{t('cleanup.trailingNewline')}</span>
            </button>
            <div class="separator"></div>
            <button class="item" onclick={() => run(() => onEditorAction?.('cleanupAll'))}>
              <span>{t('cleanup.all')}</span>
            </button>
          </div>
        {/if}
      </div>
      <div
        class="submenu-wrap"
        role="presentation"
        onmouseenter={() => (copyAsOpen = true)}
        onmouseleave={() => (copyAsOpen = false)}
      >
        <button class="item" disabled={!editing}>
          <span>{t('menu.edit.copyAs')}</span>
          <span class="hint">▸</span>
        </button>
        {#if copyAsOpen}
          <div class="flyout">
            <button class="item" onclick={() => run(() => onEditorAction?.('copy'))}>
              <span>{t('menu.edit.copyAsPlain')}</span>
            </button>
            <button class="item" onclick={() => run(() => onEditorAction?.('copyHtml'))}>
              <span>{t('menu.edit.copyAsHtml')}</span>
            </button>
            <button class="item" onclick={() => run(() => onEditorAction?.('copyMarkdown'))}>
              <span>{t('menu.edit.copyAsMarkdown')}</span>
            </button>
          </div>
        {/if}
      </div>
      <button
        class="item"
        disabled={!editing}
        onclick={() => run(() => onEditorAction?.('clipboardHistory'))}
      >
        <span>{t('menu.edit.clipboardHistory')}</span>
      </button>
      <div class="separator"></div>
      <button
        class="item"
        disabled={!hasTab || readOnly}
        title={readOnly ? t('toolbar.editReadOnlyHint') : ''}
        onclick={() => run(onToggleEdit)}
      >
        <span>{editing ? t('menu.edit.disableEdit') : t('menu.edit.enableEdit')}</span>
        <span class="hint">Ctrl+E</span>
      </button>
      <button class="item" disabled={!editing || !dirty} onclick={() => run(onSave)}>
        <span>{t('menu.edit.save')}</span>
        <span class="hint">Ctrl+S</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(onSaveAs)}>
        <span>{t('menu.edit.saveAs')}</span>
        <span class="hint">Ctrl+Shift+S</span>
      </button>
    </div>
  {:else if openMenu === 'view'}
    <div class="dropdown" role="menu" style="left: 88px">
      {#each themeEntries as item (item.id)}
        <button class="item" onclick={() => run(() => onThemeChange(item.id))}>
          <span class="radio" class:on={themeId === item.id}></span>
          <span>{item.label}</span>
        </button>
      {/each}
      <div class="separator"></div>
      <button class="item" onclick={() => run(() => onFontIncrease?.())}><span>{t('menu.view.fontIncrease')}</span></button>
      <button class="item" onclick={() => run(() => onFontDecrease?.())}><span>{t('menu.view.fontDecrease')}</span></button>
      <button class="item" onclick={() => run(() => onFontReset?.())}><span>{t('menu.view.fontReset')}</span></button>
      <div class="separator"></div>
      <button class="item" onclick={() => run(() => onToggleAutoScroll?.())}><span>{t('menu.view.autoScroll')}</span><span class="hint">{autoScroll ? '✓' : ''}</span></button>
      <button class="item" onclick={() => run(() => onToggleFocusMode?.())}><span>{t('menu.view.focusMode')}</span><span class="hint">{focusMode ? '✓' : ''}</span></button>
      <button class="item" onclick={() => run(() => onToggleTypewriter?.())}><span>{t('menu.view.typewriter')}</span><span class="hint">{typewriter ? '✓' : ''}</span></button>
        <button class="item" onclick={() => run(onTogglePomodoro)}
          ><span>{pomodoroOn ? t('menu.view.pomodoroStop') : t('menu.view.pomodoroStart')}</span><span
            class="hint">{pomodoroOn ? '✓' : ''}</span
          ></button
        >
      <button class="item" onclick={() => run(onToggleFullscreen)}><span>{t('menu.view.fullscreen')}</span><span class="hint">F11</span></button>
    </div>
  {:else if openMenu === 'help'}
    <div class="dropdown" role="menu" style="left: 130px">
      <button class="item" onclick={() => run(() => onOpenShortcuts?.())}><span>{t('menu.help.shortcuts')}</span></button>
      <button class="item" onclick={() => run(() => onOpenAbout?.())}><span>{t('menu.help.about')}</span></button>
    </div>
  {/if}
</nav>

<style>
  .menu-bar {
    position: relative;
    display: flex;
    align-items: center;
    gap: 2px;
    height: var(--h-menu);
    padding: 0 4px;
    background: var(--chrome);
    border-bottom: 1px solid var(--line);
    user-select: none;
    font-size: 12.5px;
  }

  .title {
    height: 24px;
    padding: 0 10px;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--ink);
    font: inherit;
    cursor: default;
  }

  .title:hover,
  .title.open {
    background: var(--hover);
  }

  .dropdown {
    position: absolute;
    top: calc(100% + 1px);
    z-index: 30;
    min-width: 200px;
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
    text-align: left;
    cursor: default;
  }

  .item:hover:not(:disabled) {
    background: var(--hover);
  }

  .item:disabled {
    color: var(--muted);
  }

  .item .hint {
    margin-left: auto;
    color: var(--muted);
    font-size: 12px;
  }

  .separator {
    height: 1px;
    margin: 4px 6px;
    background: var(--line);
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
  .submenu-wrap {
    position: relative;
  }

  .submenu-wrap .arrow {
    margin-left: auto;
    color: var(--muted);
  }

  .flyout {
    position: absolute;
    left: calc(100% + 2px);
    top: -4px;
    min-width: 200px;
    max-width: 320px;
    padding: 4px;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 6px;
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.16);
    z-index: 40;
  }

  .flyout .recent-name {
    display: block;
    max-width: 280px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
</style>
