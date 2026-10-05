<script lang="ts">
  // 单栏视图：栏内标签条 + 阅读/编辑视图（或空栏占位）。
  // 数据来源：父级传入的分组镜像（group）；所有操作经父级回调路由到对应栏位。
  import type { Snippet } from 'svelte';

  import type { EditorAction } from '../edit/actions';
  import { t } from '../i18n/index.svelte';
  import type {
    AppSettings,
    EditApplied,
    ReaderSettings,
    TextStats,
    WindowOption,
  } from '../ipc';
  import type { GroupState } from '../state/tabs-groups';
  import ReaderView from './ReaderView.svelte';
  import TabBar from './TabBar.svelte';

  interface Props {
    /** 栏位键（`窗口label#序号`） */
    pane: string;
    /** 是否为当前激活栏 */
    active: boolean;
    /** 多栏紧凑模式（标签条显示栏位操作按钮） */
    compact: boolean;
    /** 该栏的标签分组镜像 */
    group: GroupState;
    /** 其他主窗口（右键菜单移动目标） */
    windows: WindowOption[];
    /** 配置快照（未就绪为 null） */
    appSettings: AppSettings | null;
    readerSettings: ReaderSettings | null;
    /** 外部编辑动作信号（仅激活栏消费） */
    editorAction: EditorAction | null;
    /** 翻页信号（仅激活栏消费） */
    pageTurn: { seq: number; kind: 'up' | 'down' | 'top' | 'bottom' } | null;
    /** 折叠指令（仅激活栏消费） */
    foldCommand: { kind: 'all' | 'none'; seq: number } | null;
    /** 快照恢复请求（仅激活栏消费） */
    snapshotRestore: { name: string; seq: number } | null;
    /** 排版变更键 */
    layoutKey: string;
    /** 编辑态光标显示行（0 基；状态栏） */
    editCaretRow: number | null;
    /** 空栏「打开文件」入口 */
    onOpenDialog: () => void;
    onSelect: (tabId: number) => void;
    onClose: (tabId: number) => void;
    onCloseOthers: (tabId: number) => void;
    onCloseAll: () => void;
    onReorder: (tabId: number, toIndex: number) => void;
    onSetColor: (tabId: number, color: string | null) => void;
    onRequestWindows: () => void;
    onMoveToWindow: (tabId: number, label: string) => void;
    onNewTab: () => void;
    onNewWindow: () => void;
    onSplitRight: () => void;
    onSplitDown: () => void;
    onClosePane: () => void;
    onPercent: (percent: number) => void;
    onEditApplied: (tabId: number, result: EditApplied) => void;
    onUserScroll: () => void;
    onBreadcrumbJump: (row: number) => void;
    onTopRow: (row: number) => void;
    onSelectionStats: (stats: TextStats | null) => void;
    onCaretInfo: (info: { row: number; column: number }) => void;
    /** 单栏空态内容（窗口欢迎页；多栏空栏使用内置占位） */
    children?: Snippet;
  }
  let {
    pane,
    active,
    compact,
    group,
    windows,
    appSettings,
    readerSettings,
    editorAction,
    pageTurn,
    foldCommand,
    snapshotRestore,
    layoutKey,
    editCaretRow,
    onOpenDialog,
    onSelect,
    onClose,
    onCloseOthers,
    onCloseAll,
    onReorder,
    onSetColor,
    onRequestWindows,
    onMoveToWindow,
    onNewTab,
    onNewWindow,
    onSplitRight,
    onSplitDown,
    onClosePane,
    onPercent,
    onEditApplied,
    onUserScroll,
    onBreadcrumbJump,
    onTopRow,
    onSelectionStats,
    onCaretInfo,
    children,
  }: Props = $props();

  /** 本栏当前活动标签 */
  const activeTab = $derived(group.tabs.find((tab) => tab.tabId === group.activeId) ?? null);
</script>

<div class="pane-view" data-pane-view={pane}>
  <TabBar
    tabs={group.tabs}
    activeId={group.activeId}
    {compact}
    canClosePane={compact}
    {windows}
    {onSelect}
    {onClose}
    {onCloseOthers}
    {onCloseAll}
    {onReorder}
    {onSetColor}
    {onRequestWindows}
    {onMoveToWindow}
    {onNewTab}
    {onNewWindow}
    {onSplitRight}
    {onSplitDown}
    {onClosePane}
  />
  <div class="pane-body">
    {#if activeTab}
      <ReaderView
        tab={activeTab}
        {onPercent}
        {onEditApplied}
        editorAction={active ? editorAction : null}
        lineDefaults={appSettings?.editor.lines ?? null}
        multiCursor={appSettings?.editor.multiCursor ?? null}
        findSettings={appSettings?.find ?? null}
        insertSettings={appSettings?.editor.insert ?? null}
        autoPairs={appSettings?.editor.autoPairs ?? null}
        cleanupSettings={appSettings?.editor.cleanup ?? null}
        {onTopRow}
        {onSelectionStats}
        {onCaretInfo}
        displaySettings={appSettings?.display ?? null}
        {editCaretRow}
        readingSettings={readerSettings?.reading ?? null}
        pageTurn={active ? pageTurn : null}
        {onUserScroll}
        {onBreadcrumbJump}
        snapshotRestore={active ? snapshotRestore : null}
        foldCommand={active ? foldCommand : null}
        {layoutKey}
      />
    {:else if compact}
      <div class="pane-empty">
        <div class="pane-empty-title">{t('pane.empty')}</div>
        <div class="pane-empty-hint">{t('pane.emptyHint')}</div>
        <button type="button" class="pane-empty-btn" onclick={onOpenDialog}>
          {t('pane.open')}
        </button>
      </div>
    {:else}
      {@render children?.()}
    {/if}
  </div>
</div>

<style>
  .pane-view {
    display: flex;
    flex-direction: column;
    flex: 1 1 0;
    min-width: 0;
    min-height: 0;
    width: 100%;
    height: 100%;
  }

  .pane-body {
    position: relative;
    display: flex;
    flex: 1 1 0;
    min-height: 0;
    overflow: hidden;
  }

  .pane-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    width: 100%;
    height: 100%;
    color: var(--muted);
  }

  .pane-empty-title {
    font-size: 14px;
  }

  .pane-empty-hint {
    font-size: 12px;
  }

  .pane-empty-btn {
    padding: 5px 14px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--chrome);
    color: var(--ink);
    cursor: default;
  }

  .pane-empty-btn:hover {
    background: var(--hover);
  }
</style>
