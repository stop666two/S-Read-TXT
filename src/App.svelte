<!--
  App.svelte — 根组件：应用外壳与全局接线。
  已接线：打开（对话框/拖拽）、标签、空状态、Toast、虚拟阅读、编码切换、进度上报、
  编辑/保存/冲突/未保存三态关闭流程、冷启动就绪即显。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { listen } from '@tauri-apps/api/event';
import { readText as readClipboardText } from '@tauri-apps/plugin-clipboard-manager';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { open, save } from '@tauri-apps/plugin-dialog';

  import ConfirmDialog from './lib/components/ConfirmDialog.svelte';
  import DataDirDialog from './lib/components/DataDirDialog.svelte';
  import DropOverlay from './lib/components/DropOverlay.svelte';
  import EmptyState from './lib/components/EmptyState.svelte';
  import HistoryPanel from './lib/components/HistoryPanel.svelte';
  import AnnotationsPanel from './lib/components/AnnotationsPanel.svelte';
import OutlinePanel from './lib/components/OutlinePanel.svelte';
import SnapshotsPanel from './lib/components/SnapshotsPanel.svelte';
  import { annotations } from './lib/state/annotations.svelte';
  import MenuBar from './lib/components/MenuBar.svelte';
  import Onboarding from './lib/components/Onboarding.svelte';
  import PaneTree from './lib/components/PaneTree.svelte';
  import PaneView from './lib/components/PaneView.svelte';
  import SaveDialog from './lib/components/SaveDialog.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import TitleBar from './lib/components/TitleBar.svelte';
  import Toast from './lib/components/Toast.svelte';
  import ToolBar from './lib/components/ToolBar.svelte';
  import UnsavedDialog from './lib/components/UnsavedDialog.svelte';
  import WorkspaceFindDialog from './lib/components/WorkspaceFindDialog.svelte';
  import SplitFileDialog from './lib/components/SplitFileDialog.svelte';
  import RenameDialog from './lib/components/RenameDialog.svelte';
  import { formatBytes } from './lib/format';
  import type { EditActionType, EditorAction } from './lib/edit/actions';
  import { focusEditorProxy } from './lib/edit/focus';
  import {
    collectLeaves,
    leaf,
    makeSplit,
    MAX_PANES,
    removeLeaf,
    replaceLeaf,
    siblingLeaf,
    updateSizes,
    type PaneLayout,
    type PaneSplitDir,
  } from './lib/layout/pane-tree';
  import { EMPTY_GROUP } from './lib/state/tabs-groups';
  import { describeIpcError, ipc, toIpcError, type AppSettings, type EditApplied, type ReaderSettings, type TextStats, type ThemeSummary, type WindowOption, type WindowSession, type WorkspaceFileResult, type WorkspaceHit } from './lib/ipc';
  import { scrollMemory } from './lib/reader/scroll-memory';
  import { saveSessionNow, setSessionLayout } from './lib/session';
  import { computeDropHit } from './lib/tab-dnd';
  import { dataDirStore } from './lib/state/data-dir.svelte';
  import { historyStore } from './lib/state/history.svelte';
  import { tabs } from './lib/state/tabs.svelte';
  import { jumpStore } from './lib/state/jump.svelte';
  import { toasts } from './lib/state/toasts.svelte';
  import { i18n, setLocale, t } from './lib/i18n/index.svelte';
  import { decideShortcut, isEditorContext, modalOpen } from './lib/shortcuts/engine';
  import { comboFromEvent } from './lib/shortcuts/keys';
  import type { ShortcutAction, ShortcutMap } from './lib/shortcuts/types';
  import { applyThemeTokens } from './lib/theme';

  /** 当前主题 id（设置值；`system` 表示跟随系统） */
  let themeId = $state('system');
  /** 可用主题清单（内置 + 用户主题；工具栏/菜单共用） */
  let themes = $state<ThemeSummary[]>([]);
  /** 文件拖拽悬停（控制遮罩显示） */
  let dragging = $state(false);
  /** 应用版本（无文件时状态栏展示） */
  let version = $state('');
  /** 阅读百分比（由阅读区回报） */
  let readPercent = $state(0);
  /** 状态栏：文档统计 / 选区统计 / 顶部行 / 光标行列 */
  let docStats = $state<TextStats | null>(null);
  let selectionStats = $state<TextStats | null>(null);
  let topRow = $state<number | null>(null);
  let caretInfo = $state<{ row: number; column: number } | null>(null);
  /** 背景图 data URL 缓存与已加载文件名（避免重复读取与闪烁） */
  let bgDataUrl = $state<string | null>(null);
  let bgLoadedFile: string | null = null;
  /** 支持的编码列表（后端提供） */
  let encodings = $state<string[]>([]);
  /** 应用配置（启动加载；设置窗口变更后热刷新） */
  let appSettings = $state<AppSettings | null>(null);
  /** 阅读排版配置（主题 + 排版；变更实时应用） */
  let readerSettings = $state<ReaderSettings | null>(null);
  /** 数据目录不可写状态（共享 store；非空时展示引导弹窗） */
  const dataDirIssue = $derived(dataDirStore.issue);
  /** 历史面板开关（工具栏 / 菜单 / 快捷键共用） */
  let historyOpen = $state(false);
  let annotationsOpen = $state(false);
let outlineOpen = $state(false);
  let annotClearOpen = $state(false);

  /** 标注：切换书签（编辑态经编辑层；阅读态放在当前顶部行）。 */
  function toggleBookmarkAnnotation(): void {
    const tab = active;
    if (!tab) return;
    if (tab.editing) {
      dispatchEditorAction('toggleBookmark');
      return;
    }
    void annotations.addBookmark(tab.tabId, topRow ?? 0, 0).catch((error) => {
      toasts.error(describeIpcError(toIpcError(error)));
    });
  }

  /** 标注：清除本文件全部（ConfirmDialog 确认后执行）。 */
  async function clearAnnotationsFlow(): Promise<void> {
    annotClearOpen = false;
    const tab = active;
    if (!tab) return;
    try {
      await annotations.clear(tab.tabId);
      toasts.show(t('annot.cleared'));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }
  /** 工作区查找与替换弹窗开关（编辑菜单入口） */
  let workspaceOpen = $state(false);
  let splitOpen = $state(false);
  let renameOpen = $state(false);

  /** 跳转到工作区命中：必要时切换标签，然后广播定位请求（ReaderView/EditLayer 消费）。 */
  async function jumpToHit(file: WorkspaceFileResult, hit: WorkspaceHit): Promise<void> {
    if (tabs.activeId !== file.tabId) {
      try {
        await ipc.setActiveTab(file.tabId);
        await tabs.refresh();
      } catch (error) {
        toasts.error(describeIpcError(toIpcError(error)));
        return;
      }
    }
    jumpStore.request(file.tabId, hit.row, hit.startUtf16, hit.endUtf16);
  }

  /** 最近打开（菜单子项；最多 10 条，来自共享历史 store） */
  const recentEntries = $derived(historyStore.entries.slice(0, Math.max(0, appSettings?.file.recentLimit ?? 20)));

  /** 编辑动作信号（菜单 → 编辑层；seq 递增区分重复动作） */
  let editorAction = $state<EditorAction | null>(null);
  let editorActionSeq = 0;

  /** 分发编辑动作到编辑层 */
  function dispatchEditorAction(type: EditActionType): void {
    editorActionSeq += 1;
    editorAction = { type, seq: editorActionSeq };
  }

  /** 「数据目录不可写」→ 选择可写目录（系统目录选择器；仅本次运行有效）。 */
  async function chooseDataDir(): Promise<void> {
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: t('app.dataDir.pickTitle'),
      });
      if (typeof picked !== 'string') return;
      const status = await dataDirStore.apply(picked);
      if (status.writable) {
        toasts.show(t('app.dataDir.switched', { dir: status.dir }));
      } else {
        toasts.error(status.message ?? t('app.dataDir.stillUnwritable'));
      }
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 「数据目录不可写」→ 仅本次只读运行（不保存历史/设置/会话）。 */
  function skipDataDir(): void {
    dataDirStore.skip();
    toasts.show(t('app.dataDir.readOnlyRun'), 'warn');
  }

  /** 当前活动标签 */
  const active = $derived(tabs.active);
  /** 活动标签的关键原始值（用作 effect 依赖：避免 tabs 刷新重建对象时重复触发）。 */
  const activeTabId = $derived(active?.tabId);
  const activeRowsTotal = $derived(active?.rowsTotal);
  /** 批量重命名默认目录：激活标签父目录（剥离 \\?\ 前缀）。 */
  const renameDefaultDir = $derived.by(() => {
    const path = active?.path ?? '';
    if (!path) return null;
    const cleaned = path.replace(/^\\\\\?\\/, '');
    const index = Math.max(cleaned.lastIndexOf('\\'), cleaned.lastIndexOf('/'));
    return index > 0 ? cleaned.slice(0, index) : null;
  });

  /** 原生多选文件（返回选中路径数组；取消为空数组）。 */
  async function pickFiles(title: string): Promise<string[]> {
    const picked = await open({
      multiple: true,
      title,
      filters: [{ name: t('app.filter.text'), extensions: ['txt', 'log', 'md'] }],
    });
    if (Array.isArray(picked)) return picked;
    return typeof picked === 'string' ? [picked] : [];
  }

  /** 工具→比较文件：选两个文件并打开比较窗口。 */
  async function openCompareFlow(): Promise<void> {
    try {
      const files = await pickFiles(t('compare.pickTitle'));
      if (files.length < 2) {
        toasts.show(t('compare.needTwo'));
        return;
      }
      await ipc.openCompareWindow({ mode: 'diff', left: files[0], right: files[1], extra: null });
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 工具→三方合并：一次多选按顺序取 底本 → 我方 → 他方。 */
  async function openMergeFlow(): Promise<void> {
    try {
      const files = await pickFiles(t('compare.pickMergeTitle'));
      if (files.length < 3) {
        toasts.show(t('compare.needThree'));
        return;
      }
      await ipc.openCompareWindow({
        mode: 'merge',
        left: files[0],
        right: files[1],
        extra: files[2],
      });
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 窗口 label（栏位键前缀；会话与拖放共用）。 */
  const windowLabel = getCurrentWindow().label;
  /** 初始栏位：窗口首栏（多栏布局建立前，所有标签操作作用于该栏）。 */
  tabs.setActivePane(`${windowLabel}#1`);
  /** 窗口内分屏布局（叶子=栏位；随会话 v3 持久化）。 */
  let layout = $state<PaneLayout>(leaf(`${windowLabel}#1`));
  /** 布局中的全部栏位键（前序） */
  const paneKeys = $derived(collectLeaves(layout));
  /** 下一个新栏位序号：现有栏位最大序号 + 1（折叠后可复用空闲序号，键保持紧凑稳定）。 */
  function nextPaneSeq(): number {
    return (
      paneKeys.reduce((max, key) => {
        const seq = Number(key.split('#').pop());
        return Number.isFinite(seq) ? Math.max(max, seq) : max;
      }, 0) + 1
    );
  }
  /** 栏位数量（多栏渲染与上限判断） */
  const paneCount = $derived(paneKeys.length);
  /** 拖拽悬停的栏位与落点区（预览；null = 无） */
  let hoverPane = $state<{ pane: string; zone: string } | null>(null);
  /** 边缘分屏命中区：占栏位边长的比例与像素上下限。 */
  const PANE_EDGE_RATIO = 0.25;
  const PANE_EDGE_MIN = 40;
  const PANE_EDGE_MAX = 160;

  // 同步会话来源（布局与聚焦栏；collectSession 异步采集时读取最新值）
  $effect(() => {
    setSessionLayout($state.snapshot(layout), tabs.activePane);
  });

  /** 生效快捷键绑定（后端为唯一真源；启动加载，设置变更后刷新） */
  let shortcuts = $state<ShortcutMap>({});

  /** 重新载入全部配置快照（启动 / 设置窗口变更事件 / 窗口聚焦兜底）。
   *  序号守卫：并发「设置变更广播」的响应可能乱序，仅采纳最后一次
   *  （S4 实测缺陷：连续拖两个滑块会回跳）。 */
  let settingsLoadSeq = 0;
  async function reloadSettings(): Promise<void> {
    const seq = ++settingsLoadSeq;
    try {
      const snapshot = await ipc.getSettings();
      if (seq !== settingsLoadSeq) return;
      appSettings = snapshot.app;
      readerSettings = snapshot.reader;
        shortcuts = snapshot.shortcuts.bindings as ShortcutMap;
        themeId = snapshot.reader.theme;
        setLocale(snapshot.app.locale);
        void loadThemes();
      } catch (error) {
        if (import.meta.env.DEV) console.error('[app] 载入配置失败', error);
    }
  }

  /** 保存阅读排版配置（修改即存；以本地快照为基准合并补丁）。
   *  并发保护：快速连续修改（如连点主题按钮）时，仅采纳**最后一次**请求的响应，
   *  避免较旧的快照回灌覆盖更新的本地选择（主题回跳的真实缺陷根因）。 */
  let readerSaveSeq = 0;
  async function persistReader(patch: Partial<ReaderSettings>): Promise<void> {
    if (!readerSettings || !appSettings) return;
    const next: ReaderSettings = { ...readerSettings, ...patch };
    const seq = ++readerSaveSeq;
    try {
      const snapshot = await ipc.saveSettings({
        app: appSettings,
        reader: next,
        shortcuts: shortcuts as Record<string, string>,
      });
      if (seq !== readerSaveSeq) return;
      settingsLoadSeq += 1;
      appSettings = snapshot.app;
      readerSettings = snapshot.reader;
        shortcuts = snapshot.shortcuts.bindings as ShortcutMap;
        themeId = snapshot.reader.theme;
        setLocale(snapshot.app.locale);
      } catch (error) {
        toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 拉取文档统计（标签/行数变化或换行转换后调用；过期响应丢弃）。 */
  function refreshDocStats(tabId: number): void {
    void ipc
      .documentStats(tabId)
      .then((stats) => {
        if (active?.tabId === tabId) docStats = stats;
      })
      .catch(() => {
        if (active?.tabId === tabId) docStats = null;
      });
  }

  /** 状态栏：跳转到行（1 基）→ 借助工作区跳转通道定位当前标签。 */
  function handleGotoLine(row1: number): void {
    const current = active;
    if (!current) return;
    jumpStore.request(current.tabId, Math.max(0, row1 - 1), 0, 0);
  }

  /** 状态栏：换行符转换（编辑态；引擎登记为单撤销步，可用 Ctrl+Z 撤销）。 */
  async function handleConvertEol(target: 'lf' | 'crlf' | 'cr'): Promise<void> {
    const current = active;
    if (!current || !current.editing) return;
    try {
      await ipc.convertEol(current.tabId, target);
      await tabs.refresh();
      toasts.show(t('status.converted', { style: target.toUpperCase() }));
      refreshDocStats(current.tabId);
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 字号调整（查看菜单；与后端范围一致的前端钳制，避免无效往返） */
  function adjustFontSize(step: number): void {
    if (!readerSettings) return;
    const current = readerSettings.typography.fontSize;
    const next = Math.min(72, Math.max(8, current + step));
    if (next === current) return;
    void persistReader({ typography: { ...readerSettings.typography, fontSize: next } });
  }

  /** 重置字号为默认（16px） */
  function resetFontSize(): void {
    if (!readerSettings) return;
    void persistReader({ typography: { ...readerSettings.typography, fontSize: 16 } });
  }

  /** 自定义字体加载缓存（file → FontFace 家族名；加载中为 null 占位防并发重复） */
  const customFontFamilies = new Map<string, string | null>();
  /** 字体加载版本（加载完成后递增，触发排版变量效果重算） */
  let customFontVersion = $state(0);

  /** 确保自定义字体已注册（`custom:<文件>` 约定；读字节 → FontFace → document.fonts）。
   *  失败时允许后续重试（不缓存失败态）。 */
  async function ensureCustomFont(file: string): Promise<void> {
    if (customFontFamilies.has(file)) return;
    customFontFamilies.set(file, null);
    try {
      const base64 = await ipc.readFontData(file);
      const bytes = Uint8Array.from(atob(base64), (ch) => ch.charCodeAt(0));
      const family = `SRT Custom ${file}`;
      const face = new FontFace(family, bytes);
      await face.load();
      document.fonts.add(face);
      customFontFamilies.set(file, family);
    } catch (error) {
      customFontFamilies.delete(file);
      if (import.meta.env.DEV) console.error('[app] 自定义字体加载失败', error);
    }
    customFontVersion += 1;
  }

  /** 读取已加载的自定义字体家族名（读取 customFontVersion 以建立依赖）。 */
  function customFontFamily(file: string): string | null {
    void customFontVersion;
    return customFontFamilies.get(file) ?? null;
  }

  /** 首启引导可见性（仅启动时按配置判定一次） */
  let onboardingOpen = $state(false);
  let onboardingChecked = false;
  /** 会话恢复完成标记（恢复期间不触发自动保存，避免写回半成品状态） */
  let sessionReady = false;
  let sessionSaveTimer: ReturnType<typeof setTimeout> | null = null;

  /** 防抖保存会话（2s；窗口移动/缩放与标签变化共用） */
  function scheduleSessionSave(): void {
    if (!sessionReady) return;
    if (sessionSaveTimer) clearTimeout(sessionSaveTimer);
    sessionSaveTimer = setTimeout(() => {
      sessionSaveTimer = null;
      void saveSessionNow();
    }, 2000);
  }

  /** 历史刷新防抖（打开/关闭文件后刷新「最近打开」子菜单与面板数据源） */
  let historyRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  function scheduleHistoryRefresh(): void {
    if (historyRefreshTimer) clearTimeout(historyRefreshTimer);
    historyRefreshTimer = setTimeout(() => {
      historyRefreshTimer = null;
      void historyStore.load();
    }, 800);
  }

  /** 启动初始化：配置载入 + 首启引导判定 */
  async function initSettings(): Promise<void> {
    await reloadSettings();
    if (!onboardingChecked) {
      onboardingChecked = true;
      onboardingOpen = appSettings?.showOnboarding ?? false;
    }
  }

  /** 关闭首启引导（可勾选不再显示 → 持久化） */
  async function closeOnboarding(dontShowAgain: boolean): Promise<void> {
    onboardingOpen = false;
    if (!dontShowAgain || !appSettings || !readerSettings) return;
    const next: AppSettings = { ...appSettings, showOnboarding: false };
    try {
      const snapshot = await ipc.saveSettings({
        app: next,
        reader: readerSettings,
        shortcuts: shortcuts as Record<string, string>,
      });
      appSettings = snapshot.app;
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 会话恢复：逐个打开上次的标签（缺失/失败经 Toast 跳过）、补齐编码覆盖与编辑态，
   *  恢复滚动锚点与活动标签；恢复期间由 sessionReady 门控自动保存。 */
  async function restoreSession(): Promise<void> {
    // 启动行为设置：关闭「恢复上次会话」时直接进入空状态
    if (appSettings?.startup.restoreSession === false) return;
    let session: WindowSession | null = null;
    try {
      session = await ipc.getSession();
    } catch (error) {
      if (import.meta.env.DEV) console.error('[app] 读取会话失败', error);
    }
    try {
      const panes = Array.isArray(session?.panes)
        ? session.panes.filter((item) => item.pane.startsWith(`${windowLabel}#`))
        : [];
      const legacyTabs = session?.tabs ?? [];
      if (!session || (panes.length === 0 && legacyTabs.length === 0)) {
        // 无会话切片时仍需同步后端已有标签（如拖放迁入的新窗口/CLI 先到的文件）
        await tabs.refresh();
        return;
      }
      if (panes.length === 0) {
        // v2 旧切片（后端已迁移，理论不再出现）：并入首栏恢复
        await restorePaneTabs(`${windowLabel}#1`, legacyTabs, session.activeTabIndex ?? 0, true);
        return;
      }
      // 应用布局（后端已净化；此处再防御性校验叶子与栏位集合一致）
      const sessionLayout = session.layout ?? null;
      const layoutKeys = sessionLayout ? collectLeaves(sessionLayout) : [];
      const paneSet = new Set(panes.map((item) => item.pane));
      const layoutOk =
        sessionLayout !== null &&
        layoutKeys.length === paneSet.size &&
        layoutKeys.every((key) => paneSet.has(key));
      layout = layoutOk ? sessionLayout : leaf(panes[0].pane);
      const restoredKeys = collectLeaves(layout);
      const focused =
        session.focusedPane && paneSet.has(session.focusedPane)
          ? session.focusedPane
          : restoredKeys[0];
      // 逐栏恢复标签（含编码/编辑态/颜色）；先种子滚动位置再应用视图
      for (const paneSession of panes) {
        await restorePaneTabs(paneSession.pane, paneSession.tabs, paneSession.activeTabIndex, false);
      }
      tabs.setActivePane(focused);
    } finally {
      sessionReady = true;
    }
  }

  /** 恢复单个栏位的标签（含编码/编辑态/颜色与活动下标）。 */
  async function restorePaneTabs(
    pane: string,
    items: NonNullable<WindowSession['tabs']>,
    activeIndex: number,
    initialPane: boolean,
  ): Promise<void> {
    const seeds: { tabId: number; row: number }[] = [];
    for (const item of items) {
      try {
        const info = await ipc.openFile(item.path, pane);
        if (item.encoding) {
          await ipc.setEncoding(info.tabId, item.encoding);
        }
        if (item.editMode) {
          try {
            await ipc.toggleEdit(info.tabId);
          } catch {
            // 编辑态恢复失败（文件已变化等）：保持只读，不阻塞其余标签
          }
        }
        if (item.color) {
          try {
            await ipc.setTabColor(info.tabId, item.color);
          } catch {
            // 颜色恢复失败（调色板变化等）：忽略，不影响其余标签
          }
        }
        seeds.push({ tabId: info.tabId, row: item.scrollRow });
      } catch (error) {
        const payload = toIpcError(error);
        toasts.error(t('app.session.restoreFailed', { path: item.path, reason: describeIpcError(payload) }));
      }
    }
    // 先预热滚动锚点再应用视图：活动标签首次渲染即可恢复到记录位置
    for (const seed of seeds) {
      // 进度记忆关闭：不恢复历史位置（会话仍保存窗口与标签）
      if (readerSettings?.reading.progressMemory !== false) {
        scrollMemory.seed(seed.tabId, seed.row);
      }
    }
    await tabs.refresh(pane);
    const group = tabs.groups[pane];
    const target = group?.tabs[activeIndex];
    if (target) {
      if (initialPane) tabs.select(target.tabId);
      else tabs.selectIn(pane, target.tabId);
    }
  }

  /** 设置标签颜色（右键菜单；后端返回更新后的标签信息并同步会话）。 */
  async function setTabColor(tabId: number, color: string | null): Promise<void> {
    try {
      tabs.update(await ipc.setTabColor(tabId, color));
      void saveSessionNow();
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[app] 设置标签颜色失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 其他主窗口列表（右键菜单「移动到窗口」用；排除自身） */
  let windowOptions = $state<WindowOption[]>([]);

  /** 刷新可移动目标窗口（右键菜单打开时调用；失败静默保留空列表）。 */
  async function refreshWindowOptions(): Promise<void> {
    try {
      const self = getCurrentWindow().label;
      windowOptions = (await ipc.listWindows()).filter((item) => item.label !== self);
    } catch {
      windowOptions = [];
    }
  }

  /** 移动标签到其他主窗口（返回源窗口剩余视图；目标窗口经事件刷新）。 */
  async function moveTabToWindow(tabId: number, targetLabel: string): Promise<void> {
    const pane = tabs.paneOf(tabId) ?? tabs.activePane;
    try {
      await ipc.moveTabToWindow(tabId, targetLabel);
      await tabs.refresh(pane);
      void saveSessionNow();
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[app] 移动标签失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 刷新本窗口全部栏位的标签视图（跨窗口移动广播后调用；失败保持现状）。 */
  async function refreshTabsView(): Promise<void> {
    try {
      for (const key of collectLeaves(layout)) {
        await tabs.refresh(key);
      }
      void saveSessionNow();
    } catch {
      // 刷新失败保持现状（下一次交互会再次校准）
    }
  }

  /** 循环切换标签（nextTab / prevTab） */
  function cycleTab(step: number): void {
    const ids = tabs.tabs.map((tab) => tab.tabId);
    if (ids.length < 2) return;
    const index = tabs.activeId === null ? -1 : ids.indexOf(tabs.activeId);
    const next = ids[(index + step + ids.length) % ids.length];
    if (next !== undefined) tabs.select(next);
  }

  /** 阅读区滚动容器（虚拟滚动；无文件打开时为 null） */
  function readerElement(): HTMLElement | null {
    return (
      document.querySelector<HTMLElement>('[data-pane-active="true"] .reader') ??
      document.querySelector<HTMLElement>('.reader')
    );
  }

  /** 翻页（阅读态）：约一屏（留 3 行重叠）；平滑动画可经设置关闭 */
  /** 分屏（分页/双页/双栏）路由：快捷键翻屏交由 ReaderView 处理 */
  let pageTurnSignal = $state<{ seq: number; kind: 'up' | 'down' | 'top' | 'bottom' } | null>(null);
  let pageTurnSeq = 0;

  function readerSplitMode(): boolean {
    const reading = readerSettings?.reading;
    if (!reading || active?.editing) return false;
    const cols = reading.pageMode === 'double' ? 2 : Math.min(2, Math.max(1, reading.columns));
    return cols > 1 || reading.pageMode !== 'scroll';
  }

  function sendPageTurn(kind: 'up' | 'down' | 'top' | 'bottom'): void {
    pageTurnSeq += 1;
    pageTurnSignal = { seq: pageTurnSeq, kind };
  }

  function scrollPages(step: number): void {
      if (readerSplitMode()) {
        sendPageTurn(step > 0 ? 'down' : 'up');
        return;
      }
    const element = readerElement();
    if (!element) return;
    const span = Math.max(120, element.clientHeight - 96);
    const smooth = readerSettings?.typography.smoothScroll !== false;
    element.scrollBy({ top: step * span, behavior: smooth ? 'smooth' : 'auto' });
  }

  /** 跳到开头 / 结尾（阅读态） */
  function scrollToEdge(edge: 'top' | 'bottom'): void {
      if (readerSplitMode()) {
        sendPageTurn(edge);
        return;
      }
    const element = readerElement();
    if (!element) return;
    element.scrollTop = edge === 'top' ? 0 : element.scrollHeight;
  }

  /** 执行快捷键动作（分发到既有功能函数） */
  function runShortcut(action: ShortcutAction): void {
    switch (action) {
      case 'openFile':
        openFile();
        break;
      case 'newWindow':
        void newWindowFlow();
        break;
      case 'save':
        if (active?.editing) openSaveDialog();
        break;
      case 'saveAs':
        if (active?.editing) void saveAsFlow();
        break;
      case 'toggleEdit':
        void toggleEdit();
        break;
      case 'closeTab':
        if (active) void requestCloseTab(active.tabId);
        break;
      case 'nextTab':
        cycleTab(1);
        break;
      case 'prevTab':
        cycleTab(-1);
        break;
      case 'pageDown':
        scrollPages(1);
        break;
      case 'pageUp':
        scrollPages(-1);
        break;
      case 'firstLine':
        scrollToEdge('top');
        break;
      case 'lastLine':
        scrollToEdge('bottom');
        break;
      case 'fullscreen':
        void toggleFullscreen();
        break;
      case 'find':
        if (active?.editing) dispatchEditorAction('find');
        break;
      case 'replace':
        if (active?.editing) dispatchEditorAction('replace');
        break;
      case 'historyPanel':
        historyOpen = true;
        break;
      case 'splitRight':
        splitPane(tabs.activePane, 'row');
        break;
      case 'splitDown':
        splitPane(tabs.activePane, 'column');
        break;
      case 'closePane':
        void closePane(tabs.activePane);
        break;
    }
  }

  /** 窗口标题（自定义标题栏 + document.title：文件名 - 应用名） */
  const windowTitle = $derived(
    active
      ? `${active.untitled != null ? t('untitled.name', { n: active.untitled }) : active.name} - S-Read-TXT`
      : 'S-Read-TXT',
  );

  // 同步 document.title（任务栏/Alt-Tab 名称）
  $effect(() => {
    document.title = windowTitle;
  });

  /** 主题切换入口（菜单/工具栏/主题菜单；修改即存） */
  function setTheme(id: string): void {
    themeId = id;
    void persistReader({ theme: id });
  }

  /** 主题清单本地化名（当前语言） */
  function themeDisplayName(theme: ThemeSummary): string {
    return i18n.locale === 'en' ? theme.nameEn : theme.name;
  }

  /** 重新载入主题清单（启动 / 导入 / 删除后；失败保持现状） */
  async function loadThemes(): Promise<void> {
    try {
      themes = await ipc.listThemes();
    } catch (error) {
      if (import.meta.env.DEV) console.error('[app] 载入主题清单失败', error);
    }
  }

  /** 解析并应用当前主题（设置变化 / 系统明暗切换；失败保持现状） */
  async function applyTheme(): Promise<void> {
    try {
      const resolved = await ipc.getTheme(null);
      applyThemeTokens(resolved, {
        enabled: readerSettings?.themeAnimEnabled ?? true,
        durationMs: readerSettings?.themeAnimMs ?? 200,
      });
    } catch (error) {
      if (import.meta.env.DEV) console.error('[app] 应用主题失败', error);
    }
  }

  /** 导入用户主题（原生文件选择 → 强校验入库；与内置同名拒绝） */
  async function importThemeFlow(): Promise<void> {
    try {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [{ name: t('theme.jsonFilter'), extensions: ['json'] }],
      });
      if (typeof path !== 'string') return;
      const summary = await ipc.importTheme(path);
      await loadThemes();
      toasts.show(t('theme.imported', { name: themeDisplayName(summary) }));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 导出当前主题（system 导出解析后的实际主题） */
  async function exportThemeFlow(): Promise<void> {
    try {
      const resolved = await ipc.getTheme(null);
      const path = await save({
        defaultPath: `${resolved.id}.json`,
        filters: [{ name: t('theme.jsonFilter'), extensions: ['json'] }],
      });
      if (typeof path !== 'string') return;
      await ipc.exportTheme(resolved.id, path);
      toasts.show(t('theme.exported'));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 切换全屏（查看菜单 / F11；由全局快捷键引擎统一管理） */
  async function toggleFullscreen(): Promise<void> {
    try {
      const win = getCurrentWindow();
      await win.setFullscreen(!(await win.isFullscreen()));
    } catch (error) {
      if (import.meta.env.DEV) console.error('[app] 全屏切换失败', error);
    }
  }

  /** 打开文件（对话框；菜单/工具栏/空状态共用） */
  function openFile(): void {
    void tabs.openViaDialog();
  }

  /** 切换活动标签编码（null = 自动检测）；成功后刷新标签信息，失败走 Toast。 */
  async function changeEncoding(label: string | null): Promise<void> {
    const tab = active;
    if (!tab) return;
    try {
      tabs.update(await ipc.setEncoding(tab.tabId, label));
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[app] 编码切换失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 退出应用：多窗口走两阶段「退出所有窗口」协议（广播后各窗自行确认）；单窗口本地处理。 */
  async function quit(): Promise<void> {
    const multiWindow = await ipc.beginQuitAll().catch(() => false);
    if (multiWindow) return; // 广播会到达本窗口，统一由 handleQuitRequest 处理
    if (tabs.tabs.some((tab) => tab.dirty)) {
      pendingClose = { kind: 'quit' };
      return;
    }
    await finalizeClose();
  }

  /** 落盘/移除会话记录并关闭当前窗口；仅最后一个主窗口写「干净退出」标记。
   *  keepSlice=false（多窗口下关闭单个窗口、应用继续运行）时移除本窗口会话记录；
   *  整体退出（quit / quit-all / 最后窗口）保留记录以便下次启动恢复。 */
  async function finalizeClose(keepSlice = true): Promise<void> {
    historyStore.flushAll(tabs.tabs);
    if (keepSlice) await saveSessionNow();
    else await ipc.forgetWindowSession().catch(() => {});
    await ipc.closeWindowTabs().catch(() => {});
    const remaining = await ipc.mainWindowCount().catch(() => 1);
    if (remaining <= 1) await ipc.markCleanExit().catch(() => {});
    allowClose = true;
    await getCurrentWindow().close();
  }

  /** 收到「退出所有窗口」请求：本窗口处理脏标签，无脏直接报就绪。 */
  async function handleQuitRequest(): Promise<void> {
    if (tabs.tabs.some((tab) => tab.dirty)) {
      pendingClose = { kind: 'quit-all' };
      return;
    }
    await ipc.reportQuitReady().catch(() => {});
  }

  // ---- 编辑与关闭流程 ----

  /** 保存询问请求（Promise 队列：关闭流程可逐个等待保存结果；
   *  targetPath 存在时表示「另存为」——保存到该路径而非原路径） */
  let saveRequest = $state<{
    tabId: number;
    targetPath?: string;
    resolve: (ok: boolean) => void;
  } | null>(null);
  /** 保存弹窗的备份勾选默认值 */
  const saveBackup = true;
  /** 外部修改冲突弹窗（Promise 化，覆盖/取消都回填原保存请求） */
  let conflictRequest = $state<{
    tabId: number;
    encoding: string | null;
    backup: boolean;
    resolve: (ok: boolean) => void;
  } | null>(null);
  /** 关闭确认待办（脏标签关闭 / 退出应用 / 退出所有窗口 / 关闭窗口） */
  let pendingClose = $state<
    { kind: 'tab'; tabId: number } | { kind: 'quit' } | { kind: 'quit-all' } | { kind: 'close-window' } | null
  >(null);
  /** 允许窗口关闭（绕过 onCloseRequested 拦截；仅在用户确认后置 true） */
  let allowClose = false;

  /** 打开保存询问并等待结果（关闭流程逐个调用；取消返回 false） */
  function askSave(tabId: number): Promise<boolean> {
    return new Promise<boolean>((resolve) => {
      saveRequest = { tabId, resolve };
    });
  }

  /** 切换编辑模式（失败走 Toast：只读 / 标签不存在等） */
  async function toggleEdit(): Promise<void> {
    const tab = active;
    if (!tab) return;
    if (tab.readOnly) {
      toasts.show(t('app.editReadOnlyHint'), 'warn', 5000);
      return;
    }
    try {
      tabs.update(await ipc.toggleEdit(tab.tabId));
      focusEditorProxy();
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[app] 切换编辑失败', payload);
      toasts.error(describeIpcError(payload));
    }
  }

  /** 编辑结果回报：同步标签信息（行数/字节数/脏态） */
  function handleEditApplied(tabId: number, result: EditApplied): void {
    const tab = tabs.tabs.find((item) => item.tabId === tabId);
    if (!tab) return;
    tabs.update({
      ...tab,
      rowsTotal: result.rowsTotal,
      byteLen: result.byteLen,
      dirty: result.dirty,
    });
  }

  /** 打开保存弹窗（编辑态；编码询问走弹窗，默认保持当前编码） */
  function openSaveDialog(): void {
    if (!active?.editing) return;
    // 未命名标签：保存入口重定向到另存为（先选路径，再走编码询问）
    if (active.untitled != null) {
      void saveAsFlow();
      return;
    }
    void askSave(active.tabId);
  }

  /** 保存弹窗确认（回填 askSave 的等待） */
  function onSaveDialogConfirm(encoding: string | null, backup: boolean): void {
    const request = saveRequest;
    if (!request) return;
    saveRequest = null;
    if (request.targetPath) {
      void performSaveAs(request.tabId, request.targetPath, encoding, backup).then(
        request.resolve,
      );
    } else {
      void performSave(request.tabId, encoding, backup, false).then(request.resolve);
    }
    focusEditorProxy();
  }

  /** 保存弹窗取消（回填 false：调用方据此中止关闭流程） */
  function onSaveDialogCancel(): void {
    const request = saveRequest;
    if (!request) return;
    saveRequest = null;
    request.resolve(false);
    focusEditorProxy();
  }

  /** 执行保存（Promise<boolean>）；外部修改冲突时经覆盖弹窗再回填 */
  async function performSave(
    tabId: number,
    encoding: string | null,
    backup: boolean,
    force: boolean,
  ): Promise<boolean> {
    if (!tabId) return false;
    // 未命名标签：保存重定向到另存为；关闭链路会因未保存而中止，用户完成另存为后需再次关闭
    if (active?.tabId === tabId && active.untitled != null) {
      await saveAsFlow();
      return false;
    }
    try {
      const result = await ipc.saveTab(tabId, encoding, backup, force);
      tabs.update(result.tab);
      toasts.show(
        result.backupPath
          ? t('app.save.savedBackup', { encoding: result.encoding })
          : t('app.save.saved', { encoding: result.encoding }),
      );
      return true;
    } catch (error) {
      const payload = toIpcError(error);
      if (payload.code === 'FILE_CONFLICT') {
        return await new Promise<boolean>((resolve) => {
          conflictRequest = { tabId, encoding, backup, resolve };
        });
      }
      if (import.meta.env.DEV) console.error('[app] 保存失败', payload);
      toasts.error(describeIpcError(payload));
      return false;
    }
  }

  /** 冲突弹窗：覆盖保存 */
  function onConflictOverride(): void {
    const request = conflictRequest;
    if (!request) return;
    conflictRequest = null;
    void performSave(request.tabId, request.encoding, request.backup, true).then(request.resolve);
    focusEditorProxy();
  }

  /** 冲突弹窗：取消（保存失败） */
  function onConflictCancel(): void {
    const request = conflictRequest;
    if (!request) return;
    conflictRequest = null;
    request.resolve(false);
    focusEditorProxy();
  }

  /** 另存为执行（成功后标签重定向；标题/历史由后端同步） */
  async function performSaveAs(
    tabId: number,
    newPath: string,
    encoding: string | null,
    backup: boolean,
  ): Promise<boolean> {
    try {
      const result = await ipc.saveTabAs(tabId, newPath, encoding, backup);
      tabs.update(result.tab);
      toasts.show(t('app.save.savedAs', { name: result.tab.name, encoding: result.encoding }));
      return true;
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[app] 另存为失败', payload);
      toasts.error(describeIpcError(payload));
      return false;
    }
  }

  /** 另存为入口（选择目标路径 → 复用保存弹窗询问编码） */
  async function saveAsFlow(): Promise<void> {
    const tab = active;
    if (!tab?.editing) return;
    let filePath: string | null = null;
    try {
      filePath = await save({
        defaultPath: tab.untitled != null ? `${t('untitled.name', { n: tab.untitled })}.txt` : tab.path,
        filters: [
          { name: t('app.filter.text'), extensions: ['txt', 'log', 'md'] },
          { name: t('app.filter.all'), extensions: ['*'] },
        ],
      });
    } catch (error) {
      if (import.meta.env.DEV) console.error('[app] 另存为对话框失败', error);
      toasts.error(t('app.save.dialogFailed'));
      return;
    }
    if (!filePath) return;
    saveRequest = { tabId: tab.tabId, targetPath: filePath, resolve: () => {} };
  }

  /** 新建文件：后端生成未命名文件并以编辑模式打开（默认当前激活栏）。 */
  async function newFileFlow(pane: string = tabs.activePane): Promise<void> {
    try {
      await ipc.newFile(pane);
      await tabs.refresh(pane);
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 新建窗口：后端创建新的主窗口（级联偏移）；新窗口自行完成启动同步 */
  async function newWindowFlow(): Promise<void> {
    try {
      await ipc.newWindow();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 栏位获得焦点（指针按下于栏内）。 */
  function activatePane(pane: string): void {
    tabs.setActivePane(pane);
  }

  /** 分隔条拖动：更新所属分支权重并持久化。 */
  function updateSplitSizes(path: number[], sizes: number[]): void {
    layout = updateSizes(layout, path, sizes);
    void saveSessionNow();
  }

  /** 拆分栏位：目标叶子替换为 [原栏, 新栏] 分支（上限 4 栏；side=before 时新栏在前）。返回新栏键。 */
  function splitPane(
    target: string,
    dir: PaneSplitDir,
    side: 'after' | 'before' = 'after',
  ): string | null {
    if (paneCount >= MAX_PANES) {
      toasts.show(t('pane.limit'), 'warn');
      return null;
    }
    const pane = `${windowLabel}#${nextPaneSeq()}`;
    layout = replaceLeaf(layout, target, (old) =>
      side === 'before' ? makeSplit(dir, leaf(pane), leaf(old)) : makeSplit(dir, leaf(old), leaf(pane)),
    );
    tabs.setActivePane(pane);
    void saveSessionNow();
    return pane;
  }

  /** 拖拽落点解析：指针坐标 → 目标栏位与区（标签条/内容边缘/内容中心）。 */
  function resolvePaneDrop(clientX: number, clientY: number): { pane: string; zone: string } | null {
    const cells = document.querySelectorAll<HTMLElement>('[data-pane]');
    for (const cell of cells) {
      const rect = cell.getBoundingClientRect();
      if (clientX < rect.left || clientX > rect.right || clientY < rect.top || clientY > rect.bottom)
        continue;
      const pane = cell.dataset.pane ?? '';
      const bar = cell.querySelector<HTMLElement>('.tab-bar');
      if (bar) {
        const barRect = bar.getBoundingClientRect();
        if (clientY >= barRect.top && clientY <= barRect.bottom) return { pane, zone: 'tabBar' };
      }
      const edgeX = Math.min(clientX - rect.left, rect.right - clientX);
      const edgeY = Math.min(clientY - rect.top, rect.bottom - clientY);
      const limitX = Math.min(Math.max(rect.width * PANE_EDGE_RATIO, PANE_EDGE_MIN), PANE_EDGE_MAX);
      const limitY = Math.min(Math.max(rect.height * PANE_EDGE_RATIO, PANE_EDGE_MIN), PANE_EDGE_MAX);
      if (edgeY < limitY && edgeY <= edgeX) {
        return { pane, zone: clientY < rect.top + rect.height / 2 ? 'edge-top' : 'edge-bottom' };
      }
      if (edgeX < limitX) {
        return { pane, zone: clientX < rect.left + rect.width / 2 ? 'edge-left' : 'edge-right' };
      }
      return { pane, zone: 'center' };
    }
    return null;
  }

  /** 拖拽移动预览（本地拖拽与跨窗悬停共用）。 */
  function paneDragMove(clientX: number, clientY: number): void {
    hoverPane = resolvePaneDrop(clientX, clientY);
  }

  /** 拖拽释放：按落点执行栏内重排 / 跨栏移动 / 边缘分屏。
   *  跨窗拖入时本窗即目标窗，由事件驱动调用（源窗标签经广播刷新）。 */
  async function paneDragDrop(tabId: number, clientX: number, clientY: number): Promise<void> {
    const sourcePane = tabs.paneOf(tabId);
    let target = resolvePaneDrop(clientX, clientY);
    if (!target) {
      // 落点在窗体装饰区（标题栏/菜单/工具栏）：本窗拖拽保持原位；外来标签落到首栏
      if (sourcePane !== null) return;
      target = { pane: `${windowLabel}#1`, zone: 'center' };
    }
    hoverPane = null;
    const edgeDir: PaneSplitDir | null =
      target.zone === 'edge-left' || target.zone === 'edge-right'
        ? 'row'
        : target.zone === 'edge-top' || target.zone === 'edge-bottom'
          ? 'column'
          : null;
    try {
      if (edgeDir) {
        const side = target.zone === 'edge-left' || target.zone === 'edge-top' ? 'before' : 'after';
        const newPane = splitPane(target.pane, edgeDir, side);
        if (newPane) await ipc.moveTabToPane(tabId, newPane);
      } else if (target.zone === 'tabBar') {
        const bar = document.querySelector<HTMLElement>(`[data-pane="${target.pane}"] .tab-bar`);
        const index = bar ? computeDropHit(bar, clientX, tabId).index : 0;
        if (sourcePane === target.pane) tabs.reorder(tabId, index);
        else await ipc.moveTabToPane(tabId, target.pane, index);
      } else if (sourcePane !== target.pane) {
        await ipc.moveTabToPane(tabId, target.pane);
      }
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
    await tabs.refresh(target.pane);
    if (sourcePane && sourcePane !== target.pane) await tabs.refresh(sourcePane);
    void saveSessionNow();
  }

  /** 关闭栏位：标签无损并入最近相邻栏，随后折叠布局（唯一栏时忽略）。 */
  async function closePane(target: string): Promise<void> {
    if (paneCount <= 1) return;
    const sibling =
      siblingLeaf(layout, target) ?? paneKeys.find((key) => key !== target) ?? tabs.activePane;
    const ids = (tabs.groups[target]?.tabs ?? []).map((tab) => tab.tabId);
    for (const id of ids) {
      try {
        await ipc.moveTabToPane(id, sibling);
      } catch (error) {
        toasts.error(describeIpcError(toIpcError(error)));
        return;
      }
    }
    layout = removeLeaf(layout, target) ?? leaf(sibling);
    tabs.dropGroup(target);
    tabs.setActivePane(sibling);
    await tabs.refresh(sibling);
    void saveSessionNow();
  }

  /** 导出…：格式由扩展名推断，不改变标签状态 */
  async function exportFlow(): Promise<void> {
    const tab = active;
    if (!tab) return;
    const suggested = tab.name.replace(/\.[^.]+$/, '');
    let filePath: string | null = null;
    try {
      filePath = await save({
        defaultPath: `${suggested}.html`,
        filters: [
          { name: 'HTML', extensions: ['html', 'htm'] },
          { name: 'JSON', extensions: ['json'] },
          { name: 'CSV', extensions: ['csv'] },
          { name: 'Markdown', extensions: ['md'] },
          { name: 'TXT', extensions: ['txt'] },
          { name: t('export.filterAll'), extensions: ['*'] },
        ],
      });
    } catch (error) {
      toasts.error(t('export.failed', { message: String(error) }));
      return;
    }
    if (!filePath) return;
    try {
      const bytes = await ipc.exportText(tab.tabId, filePath);
      toasts.show(t('export.done', { bytes }));
    } catch (error) {
      toasts.error(t('export.failed', { message: describeIpcError(toIpcError(error)) }));
    }
  }

  /** 打印…：打开打印窗口，由页面自动调起系统打印 */
  async function printFlow(): Promise<void> {
    const tab = active;
    if (!tab) return;
    try {
      await ipc.printDocument(tab.tabId);
      toasts.show(t('print.opened'));
    } catch (error) {
      toasts.error(t('print.failed', { message: describeIpcError(toIpcError(error)) }));
    }
  }

  /** 重新加载待确认（脏标签） */
  let reloadRequest = $state<{ tabId: number } | null>(null);

  /** 重新加载入口：脏标签先确认（放弃未保存修改） */
  function reloadFlow(): void {
    const tab = active;
    if (!tab) return;
    if (tab.dirty) {
      reloadRequest = { tabId: tab.tabId };
      return;
    }
    void performReload(tab.tabId);
  }

  /** 执行重新加载（后端从磁盘重建索引，丢弃编辑文档） */
  async function performReload(tabId: number): Promise<void> {
    try {
      tabs.update(await ipc.reloadTab(tabId));
      toasts.show(t('app.reload.done'));
    } catch (error) {
      const payload = toIpcError(error);
      if (import.meta.env.DEV) console.error('[app] 重新加载失败', payload);
      toasts.error(describeIpcError(payload));
    } finally {
      focusEditorProxy();
    }
  }

  /** 重载确认弹窗文案 */
  const reloadMessage = $derived.by(() => {
    const request = reloadRequest;
    if (!request) return '';
    const name = tabs.tabs.find((item) => item.tabId === request.tabId)?.name ?? t('app.currentFile');
    return t('app.reload.message', { name });
  });

  // ---- 关闭流程 ----

  /** 请求关闭标签（脏标签先经三态确认；不脏直接关闭） */
  async function requestCloseTab(tabId: number): Promise<void> {
    const tab = tabs.tabs.find((item) => item.tabId === tabId);
    if (!tab) return;
    if (!tab.dirty) {
      historyStore.flushTab(tab);
      await tabs.close(tabId);
      return;
    }
    pendingClose = { kind: 'tab', tabId };
  }

  /** 右键菜单：关闭其他标签（脏标签保留并计数提示，避免静默丢改动）。 */
  async function closeOtherTabs(keepId: number): Promise<void> {
    let skippedDirty = 0;
    for (const tab of [...tabs.tabs]) {
      if (tab.tabId === keepId) continue;
      if (tab.dirty) {
        skippedDirty += 1;
        continue;
      }
      historyStore.flushTab(tab);
      await tabs.close(tab.tabId);
    }
    if (skippedDirty > 0) toasts.show(t('app.close.keptDirty', { count: skippedDirty }), 'warn');
  }

  /** 右键菜单：关闭全部标签（脏标签保留并计数提示）。 */
  async function closeAllTabs(): Promise<void> {
    let skippedDirty = 0;
    for (const tab of [...tabs.tabs]) {
      if (tab.dirty) {
        skippedDirty += 1;
        continue;
      }
      historyStore.flushTab(tab);
      await tabs.close(tab.tabId);
    }
    if (skippedDirty > 0) toasts.show(t('app.close.keptDirty', { count: skippedDirty }), 'warn');
  }

  /** 拖拽排序（前端乐观更新；后端失败时 store 自动回读校准）。 */
  function reorderTab(tabId: number, toIndex: number): void {
    tabs.reorder(tabId, toIndex);
  }

  /** 逐个保存指定标签（任一取消即中止，返回是否全部完成） */
  async function saveTabsSequentially(tabIds: number[]): Promise<boolean> {
    for (const tabId of tabIds) {
      const ok = await askSave(tabId);
      if (!ok) return false;
    }
    return true;
  }

  /** 三态弹窗动作：保存 / 不保存 / 取消 */
  async function resolvePendingClose(action: 'save' | 'discard' | 'cancel'): Promise<void> {
    const pending = pendingClose;
    if (!pending) return;
    if (action === 'cancel') {
      pendingClose = null;
      if (pending.kind === 'quit-all') await ipc.reportQuitCancel().catch(() => {});
      return;
    }
    const dirtyIds =
      pending.kind === 'tab'
        ? [pending.tabId].filter(
            (tabId) => tabs.tabs.find((item) => item.tabId === tabId)?.dirty ?? false,
          )
        : tabs.tabs.filter((item) => item.dirty).map((item) => item.tabId);
    if (action === 'save') {
      const saved = await saveTabsSequentially(dirtyIds);
      if (!saved) {
        // 取消保存：保留待办（用户可重新选择），不执行关闭
        return;
      }
    }
    pendingClose = null;
    if (pending.kind === 'tab') {
      const closingTab = tabs.tabs.find((tab) => tab.tabId === pending.tabId);
      if (closingTab) historyStore.flushTab(closingTab);
      await tabs.close(pending.tabId);
      focusEditorProxy();
    } else if (pending.kind === 'quit-all') {
      // 本窗口确认完毕 → 报就绪；等全部窗口就绪后由 proceed 事件统一关闭
      await ipc.reportQuitReady().catch(() => {});
    } else if (pending.kind === 'close-window') {
      await finalizeClose(false);
    } else {
      await finalizeClose();
    }
  }

  /** 三态弹窗说明文案 */
  const closeMessage = $derived.by(() => {
    const pending = pendingClose;
    if (!pending) return '';
    if (pending.kind === 'tab') {
      const name =
        tabs.tabs.find((item) => item.tabId === pending.tabId)?.name ?? t('app.currentFile');
      return t('app.close.tabMessage', { name });
    }
    const count = tabs.tabs.filter((item) => item.dirty).length;
    if (pending.kind === 'quit-all') return t('app.close.quitAllMessage', { count });
    if (pending.kind === 'close-window') return t('app.close.closeWindowMessage', { count });
    return t('app.close.quitMessage', { count });
  });

  /** 三态弹窗可见性（保存询问/冲突弹窗进行中时让位，避免叠层） */
  const unsavedOpen = $derived(pendingClose !== null && saveRequest === null && conflictRequest === null);

  /** 保存弹窗当前服务的标签信息（默认编码显示用） */
  const saveDialogTab = $derived(tabs.tabs.find((item) => item.tabId === saveRequest?.tabId) ?? null);

  // 主题应用：设置变化时解析令牌写入 CSS 变量；跟随系统时监听系统明暗切换。
  $effect(() => {
    void themeId;
    void readerSettings?.themeAnimEnabled;
    void readerSettings?.themeAnimMs;
    void applyTheme();
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = (): void => {
      if (themeId === 'system') void applyTheme();
    };
    media.addEventListener('change', onChange);
    return () => media.removeEventListener('change', onChange);
  });

  /** 主题菜单条目（跟随系统 + 清单，当前语言） */
  const themeEntries = $derived.by(() => [
    { id: 'system', label: t('theme.system') },
    ...themes.map((theme) => ({ id: theme.id, label: themeDisplayName(theme) })),
  ]);

  /** 当前主题显示名（未知 id 原样展示，避免空文案） */
  const themeLabel = $derived.by(() => {
    if (themeId === 'system') return t('theme.system');
    const found = themes.find((theme) => theme.id === themeId);
    return found ? themeDisplayName(found) : themeId;
  });

  /** 文档统计拉取（标签切换 / 行数变化时刷新）。 */
  $effect(() => {
    const tabId = activeTabId;
    void activeRowsTotal;
    if (tabId === undefined) {
      docStats = null;
      return;
    }
    refreshDocStats(tabId);
  });

  /** 切换标签时清空选区/光标残留（仅 tabId 变化触发；顶部行由阅读区首帧上报覆盖，勿清以避免竞态）。 */
  $effect(() => {
    void activeTabId;
    selectionStats = null;
    caretInfo = null;
  });

  /** 排版变更键（传给 ReaderView 触发行高失效重排；值变化即重排） */
  const typographyKey = $derived(
    readerSettings
      ? JSON.stringify(readerSettings.typography) + `:wrap=${appSettings?.display?.wordWrap ?? true}`
      : '',
  );

  // 排版令牌写入 CSS 变量：阅读区实时生效
  // （字号/行高/字体/限宽/边距/段间距/首行缩进/对齐；自定义字体按需动态加载）
  $effect(() => {
    const typo = readerSettings?.typography;
    if (!typo) return;
    const root = document.documentElement;
    let fontStack = `${typo.fontFamily}, system-ui, sans-serif`;
    if (typo.fontFamily.startsWith('custom:')) {
      const file = typo.fontFamily.slice('custom:'.length);
      const loaded = customFontFamily(file);
      if (loaded === null) void ensureCustomFont(file);
      fontStack = `"${loaded ?? 'Microsoft YaHei'}", system-ui, sans-serif`;
    }
    root.style.setProperty('--font-reading', fontStack);
    root.style.setProperty('--reading-size', `${typo.fontSize}px`);
    root.style.setProperty('--reading-line-height', `${typo.lineHeight}`);
    root.style.setProperty('--reading-width', `${typo.contentWidth}px`);
    root.style.setProperty('--reading-para-spacing', `${typo.paragraphSpacing}px`);
    const margins = readerSettings?.margins;
    if (margins) {
      const reading = margins.reading;
      const editing = margins.editing;
      root.style.setProperty('--reading-pad-top', `${reading.top}px`);
      root.style.setProperty('--reading-pad-right', `${reading.right}px`);
      root.style.setProperty('--reading-pad-bottom', `${reading.bottom}px`);
      root.style.setProperty('--reading-pad-left', `${reading.left}px`);
      root.style.setProperty('--edit-pad-top', `${editing.top}px`);
      root.style.setProperty('--edit-pad-right', `${editing.right}px`);
      root.style.setProperty('--edit-pad-bottom', `${editing.bottom}px`);
      root.style.setProperty('--edit-pad-left', `${editing.left}px`);
    }
    root.style.setProperty('--reading-indent', `${typo.firstLineIndent * typo.fontSize}px`);
    root.style.setProperty('--reading-align', typo.textAlign === 'justify' ? 'justify' : 'left');
  });

  /** 背景图设置（快照未就绪时为 null） */
  const background = $derived(readerSettings?.background ?? null);
  /** 背景层是否启用（启用且有文件） */
  const bgActive = $derived(Boolean(background?.enabled && background.file));
  /** 背景图层内联样式（填充/重复/透明度/模糊/亮度） */
  const bgLayerStyle = $derived.by(() => {
    if (!background || !bgDataUrl) return '';
    const size =
      background.fill === 'stretch'
        ? '100% 100%'
        : background.fill === 'tile'
          ? 'auto'
          : background.fill;
    const repeat = background.fill === 'tile' ? 'repeat' : 'no-repeat';
    const bright = 1 + background.dim / 100;
    const blur = background.blur > 0 ? ` blur(${background.blur}px)` : '';
    const inset = background.blur > 0 ? `inset:${-2 * background.blur}px;` : '';
    return `background-image:url("${bgDataUrl}");background-size:${size};background-repeat:${repeat};opacity:${background.opacity / 100};filter:brightness(${bright})${blur};${inset}`;
  });

  // 背景图加载：启用且文件变化时读取一次（base64 → data URL）；失败静默降级为无背景
  $effect(() => {
    const file = bgActive ? (background?.file ?? null) : null;
    if (!file) {
      bgLoadedFile = null;
      bgDataUrl = null;
      return;
    }
    if (file === bgLoadedFile) return;
    bgLoadedFile = file;
    void ipc
      .readBackgroundImage(file)
      .then((data) => {
        if (bgLoadedFile === file) bgDataUrl = `data:${data.mime};base64,${data.dataBase64}`;
      })
      .catch(() => {
        if (bgLoadedFile === file) bgDataUrl = null;
      });
  });

  // 标签集合/活动标签变化：防抖保存会话 + 刷新历史数据源
  // （「最近打开」子菜单与面板共用；复用打开可只改活动标签，因此监听活动标签而非仅数量）
  $effect(() => {
    void tabs.tabs.map((tab) => tab.tabId).join(',');
    void tabs.activeId;
    scheduleSessionSave();
    scheduleHistoryRefresh();
  });

  /** 专注模式生效中（阅读态且未编辑；标题栏保留窗口控制，其余栏隐藏） */
    const focusReading = $derived(
    (readerSettings?.reading.focusMode ?? false) && !(active?.editing ?? false),
  );

  /** 自动滚动（菜单开关；rAF 循环；用户主动滚动即停止） */
  let autoScrollOn = $state(false);
  let autoScrollRaf = 0;
  /** 折叠指令：菜单广播给 ReaderView（seq 去重） */
  let foldCommand = $state<{ kind: 'all' | 'none'; seq: number } | null>(null);
  let foldSeq = 0;
  const foldingEnabled = $derived((appSettings?.display.folding ?? 'off') !== 'off');
  function foldAll(): void {
    foldCommand = { kind: 'all', seq: ++foldSeq };
  }
  function foldNone(): void {
    foldCommand = { kind: 'none', seq: ++foldSeq };
  }

  function stopAutoScroll(): void {
    cancelAnimationFrame(autoScrollRaf);
    autoScrollRaf = 0;
  }

  function startAutoScroll(): void {
    stopAutoScroll();
    let last = performance.now();
    const step = (now: number): void => {
      if (!autoScrollOn) return;
      const el = readerElement();
      if (!el) {
        autoScrollOn = false;
        return;
      }
      const dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      const speed = readerSettings?.reading.autoScrollSpeed ?? 30;
      const before = el.scrollTop;
      el.scrollTop = before + speed * dt;
      const atBottom = before + el.clientHeight >= el.scrollHeight - 1;
      if (el.scrollTop === before && atBottom) {
        autoScrollOn = false;
        toasts.show(t('reading.autoScrollEnd'));
        return;
      }
      autoScrollRaf = requestAnimationFrame(step);
    };
    autoScrollRaf = requestAnimationFrame(step);
  }

  function toggleAutoScroll(): void {
    if (!active || active.editing) return;
    autoScrollOn = !autoScrollOn;
    if (autoScrollOn) startAutoScroll();
    else stopAutoScroll();
  }

  /** 用户主动滚动 → 停止自动滚动（阅读节奏让位于用户） */
  function handleUserScroll(): void {
    if (autoScrollOn) {
      autoScrollOn = false;
      stopAutoScroll();
    }
  }

  function toggleFocusMode(): void {
    if (!readerSettings) return;
    void persistReader({
      reading: { ...readerSettings.reading, focusMode: !readerSettings.reading.focusMode },
    });
  }

  function toggleTypewriter(): void {
    if (!readerSettings) return;
    void persistReader({
      reading: { ...readerSettings.reading, typewriter: !readerSettings.reading.typewriter },
    });
  }

  // 进入编辑模式时自动停止自动滚动
  $effect(() => {
    if ((active?.editing ?? false) && autoScrollOn) {
      autoScrollOn = false;
      stopAutoScroll();
    }
  });

  /** 阅读时长 / 护眼提醒 / 番茄钟 */
  let readingSeconds = $state<number | null>(null);
  let pomodoroOn = $state(false);
  let windowFocused = $state(true);
  let readingPendingSeconds = 0;
  let lastStatsTick = Date.now();
  let statsTimer: ReturnType<typeof setInterval> | undefined;
  let eyeCareTimer: ReturnType<typeof setInterval> | undefined;
  let pomodoroTimer: ReturnType<typeof setTimeout> | undefined;

  /** 当前是否计入阅读时长（窗口聚焦 + 阅读态 + 开关开启） */
  function readingCounts(): boolean {
    return (
      windowFocused && !!active && !active.editing && (readerSettings?.reading.readingStats ?? true)
    );
  }

  /** 每 10 秒累计阅读时长；满 60 秒上报并刷新显示 */
  function tickReading(): void {
    const now = Date.now();
    const delta = Math.min(60, Math.max(0, Math.round((now - lastStatsTick) / 1000)));
    lastStatsTick = now;
    if (!readingCounts() || delta === 0) return;
    readingPendingSeconds += delta;
    if (readingPendingSeconds >= 60) void flushReadingSeconds();
  }

  /** 上报待记秒数（退出/满一分钟）；失败静默（统计非关键路径） */
  async function flushReadingSeconds(): Promise<void> {
    const seconds = Math.min(3600, readingPendingSeconds);
    readingPendingSeconds = 0;
    if (seconds <= 0) return;
    try {
      const stats = await ipc.addReadingSeconds(seconds);
      readingSeconds = stats.todaySeconds;
    } catch {
      // 统计失败不影响使用
    }
  }

  /** 重建护眼提醒定时器（间隔 0 = 关闭；失焦不提醒） */
  function restartEyeCare(): void {
    if (eyeCareTimer) clearInterval(eyeCareTimer);
    eyeCareTimer = undefined;
    const minutes = readerSettings?.reading.eyeCareIntervalMin ?? 0;
    if (minutes <= 0) return;
    eyeCareTimer = setInterval(
      () => {
        if (windowFocused) toasts.show(t('reading.eyeCareToast'));
      },
      minutes * 60_000,
    );
  }

  /** 番茄钟开始/停止（时长取设置；到时提示） */
  function togglePomodoro(): void {
    if (pomodoroTimer) clearTimeout(pomodoroTimer);
    if (pomodoroOn) {
      pomodoroOn = false;
      return;
    }
    const minutes = readerSettings?.reading.pomodoroMin ?? 25;
    pomodoroOn = true;
    toasts.show(t('reading.pomodoroStart', { min: minutes }));
    pomodoroTimer = setTimeout(
      () => {
        pomodoroOn = false;
        toasts.show(t('reading.pomodoroDone'));
      },
      minutes * 60_000,
    );
  }

  // 护眼间隔/聚焦状态变化时重建提醒定时器
  $effect(() => {
    void readerSettings?.reading.eyeCareIntervalMin;
    void windowFocused;
    restartEyeCare();
  });

  /** 打开剪贴板中的路径：取首行、去空白后走统一打开链路。 */
  async function pastePathOpen(): Promise<void> {
    let text = '';
    try {
      text = await readClipboardText();
    } catch {
      text = '';
    }
    const firstLine = text.split(/\r?\n/, 1)[0]?.trim() ?? '';
    if (firstLine === '') {
      toasts.show(t('cli.pasteEmpty'));
      return;
    }
    await tabs.openPath(firstLine);
  }

  // 命令行/单实例：启动队列由主窗口排空（先挂载）；运行期转发事件定向到目标窗口，
  // 事件仅作唤醒信号，统一经 take_cli_files 原子排空（事件与队列叠加不丢失、不重复打开）。
  const isPrimaryWindow = getCurrentWindow().label === 'main';
  onMount(() => {
    const drainCliFiles = (): void => {
      void ipc
        .takeCliFiles()
        .then((paths) => {
          for (const path of paths) void tabs.openPath(path);
        })
        .catch(() => {});
    };
    if (isPrimaryWindow) drainCliFiles();
    let unlistenCli: (() => void) | undefined;
    void listen<unknown>('srt://cli-open', () => drainCliFiles()).then((stop) => {
      unlistenCli = stop;
    });
    return () => unlistenCli?.();
  });

  
onMount(() => {
    // 启动显示策略（维护者确认）：窗口由 Rust 侧在启动时立即显示
    // （主题背景色 + HTML 内置占位先行）；窗口几何恢复也已在 Rust 侧完成
    // （显示之前，避免可见跳动）。这里只归还键盘焦点。
    void getCurrentWindow()
      .setFocus()
      .catch(() => {
        // 忽略：聚焦失败不影响使用（用户点击窗口后仍可正常操作）
      });
    // 版本号（状态栏无文件时展示；失败不阻塞启动）
    void ipc.getAppInfo().then(
      (info) => {
        version = `v${info.version}`;
      },
      (error: unknown) => toasts.error(describeIpcError(toIpcError(error))),
    );
    // 编码列表（工具栏编码下拉；失败同样显式提示）
    void ipc.listEncodings().then(
      (list) => {
        encodings = list;
      },
      (error: unknown) => toasts.error(describeIpcError(toIpcError(error))),
    );
    // 数据目录可写性探测（不可写 → 弹引导：选择可写目录 / 仅本次只读运行）
    void dataDirStore.check();
    // 自动化测试钩子：主题切换走真实应用路径（E2E 截图与断言用）
    if (window.__srt) window.__srt.setTheme = (id: string) => setTheme(id);
    // 历史记录预载（面板与「最近打开」子菜单共用数据源）
    void historyStore.load();

    // 窗口关闭拦截（X 按钮/系统关闭）：仅处理**本窗口**标签——
    // 有脏标签走三态确认；确认后落盘会话并关闭（最后一窗写干净退出标记）
    let unlistenClose: (() => void) | undefined;
    void getCurrentWindow()
      .onCloseRequested((event) => {
        if (allowClose) return;
        event.preventDefault();
        void (async () => {
          if (!tabs.tabs.some((tab) => tab.dirty)) {
            // 多窗口下关闭单个窗口：应用继续运行 → 移除本窗口会话记录；
            // 最后一个窗口：应用退出 → 保留记录供下次恢复。
            const count = await ipc.mainWindowCount().catch(() => 1);
            await finalizeClose(count <= 1);
            return;
          }
          const count = await ipc.mainWindowCount().catch(() => 1);
          pendingClose = { kind: count > 1 ? 'close-window' : 'quit' };
        })();
      })
      .then((stop) => {
        unlistenClose = stop;
      });

    // 退出所有窗口（多窗口两阶段协议）：请求 → 各窗确认 → 放行关闭 / 取消
    let unlistenQuitRequest: (() => void) | undefined;
    let unlistenQuitProceed: (() => void) | undefined;
    let unlistenQuitCancelled: (() => void) | undefined;
    void listen('srt://quit-request', () => void handleQuitRequest()).then((stop) => {
      unlistenQuitRequest = stop;
    });
    void listen('srt://quit-proceed', () => void finalizeClose()).then((stop) => {
      unlistenQuitProceed = stop;
    });
    void listen('srt://quit-cancelled', () => {
      if (pendingClose?.kind === 'quit-all') pendingClose = null;
    }).then((stop) => {
      unlistenQuitCancelled = stop;
    });

    // 标签跨窗口移动：目标窗口收到广播后刷新自身标签视图
    let unlistenTabsChanged: (() => void) | undefined;
    void listen('srt://tabs-changed', () => void refreshTabsView()).then((stop) => {
      unlistenTabsChanged = stop;
    });

    // 拖放落点：先刷新视图，再按落点坐标解析目标栏位（栏内重排/跨栏移动/边缘分屏）
    let unlistenDropped: (() => void) | undefined;
    void listen<{ tabId: number; clientX: number; clientY: number }>(
      'srt://tab-drag-dropped',
      (event) => {
        void (async () => {
          await refreshTabsView();
          await paneDragDrop(event.payload.tabId, event.payload.clientX, event.payload.clientY);
        })();
      },
    ).then((stop) => {
      unlistenDropped = stop;
    });

    // 跨窗悬停：按指针坐标预览目标栏位与边缘/插入落点
    let unlistenHover: (() => void) | undefined;
    void listen<{ active: boolean; clientX: number; clientY: number }>(
      'srt://tab-drag-hover',
      (event) => {
        hoverPane = event.payload.active
          ? resolvePaneDrop(event.payload.clientX, event.payload.clientY)
          : null;
      },
    ).then((stop) => {
      unlistenHover = stop;
    });

    // 拖拽结束（原生路径）：清理栏位预览
    let unlistenDragEnd: (() => void) | undefined;
    void listen('srt://tab-drag-end', () => {
      hoverPane = null;
    }).then((stop) => {
      unlistenDragEnd = stop;
    });

    // 文件拖拽（Tauri 原生事件：over → 遮罩；drop → 逐个打开）
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === 'over') {
          dragging = true;
        } else if (event.payload.type === 'drop') {
          dragging = false;
          for (const path of event.payload.paths) {
            void tabs.openPath(path);
          }
        } else {
          dragging = false;
        }
      })
      .then((stop) => {
        unlisten = stop;
      });

    // 会话：启动恢复 + 窗口移动/缩放防抖保存 + 定时兜底保存
    void restoreSession();
    let unlistenMoved: (() => void) | undefined;
    void getCurrentWindow()
      .onMoved(() => scheduleSessionSave())
      .then((stop) => {
        unlistenMoved = stop;
      });
    let unlistenResized: (() => void) | undefined;
    void getCurrentWindow()
      .onResized(() => scheduleSessionSave())
      .then((stop) => {
        unlistenResized = stop;
      });
    const sessionInterval = setInterval(() => void saveSessionNow(), 30000);

    // 配置：启动加载 + 设置窗口变更事件刷新 + 窗口聚焦兜底刷新
    void initSettings();
    void ipc
      .getReadingStats()
      .then((stats) => (readingSeconds = stats.todaySeconds))
      .catch(() => undefined);
    lastStatsTick = Date.now();
    statsTimer = setInterval(tickReading, 10_000);
    let unlistenSettings: (() => void) | undefined;
    void listen('srt://settings-changed', () => void reloadSettings()).then((stop) => {
      unlistenSettings = stop;
    });
    let unlistenFocus: (() => void) | undefined;
    void getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        windowFocused = focused;
        if (focused) void reloadSettings();
      })
      .then((stop) => {
        unlistenFocus = stop;
      });

    // 全局快捷键（捕获阶段先于编辑层与浏览器默认行为）
  const onGlobalKeydown = (event: KeyboardEvent): void => {
    // 专注模式：Esc 退出（阅读态专属；编辑/弹窗各自处理 Esc）
    if (event.key === 'Escape' && focusReading) {
      event.preventDefault();
      toggleFocusMode();
      return;
    }
      const decision = decideShortcut(comboFromEvent(event), shortcuts, {
        editorContext: isEditorContext(event.target),
        modalOpen: modalOpen(),
        defaultPrevented: event.defaultPrevented,
        composing: event.isComposing,
      });
      if (!decision) return;
      event.preventDefault();
      event.stopPropagation();
      if (decision.kind === 'fixedTab') {
        const target = tabs.tabs[decision.index];
        if (target) tabs.select(target.tabId);
        return;
      }
      runShortcut(decision.action);
    };
    window.addEventListener('keydown', onGlobalKeydown, true);

    return () => {
      stopAutoScroll();
      if (statsTimer) clearInterval(statsTimer);
      if (eyeCareTimer) clearInterval(eyeCareTimer);
      if (pomodoroTimer) clearTimeout(pomodoroTimer);
      void flushReadingSeconds();
      unlisten?.();
      unlistenClose?.();
      unlistenQuitRequest?.();
      unlistenQuitProceed?.();
      unlistenQuitCancelled?.();
      unlistenTabsChanged?.();
      unlistenDropped?.();
      unlistenHover?.();
      unlistenDragEnd?.();
      unlistenSettings?.();
      unlistenFocus?.();
      unlistenMoved?.();
      unlistenResized?.();
      clearInterval(sessionInterval);
      if (sessionSaveTimer) clearTimeout(sessionSaveTimer);
      if (historyRefreshTimer) clearTimeout(historyRefreshTimer);
      window.removeEventListener('keydown', onGlobalKeydown, true);
    };
  });
  // ---------- P3-1 快照与自动保存 ----------

  /** 自动保存（快照式）：编辑中且脏的标签按设置间隔生成快照（后端去重）。 */
  let autosaveTimer: ReturnType<typeof setInterval> | null = null;
  $effect(() => {
    const intervalSec = appSettings?.file.autosaveIntervalSec ?? 30;
    const enabled = (appSettings?.file.versionHistory ?? true) && intervalSec >= 5;
    if (autosaveTimer !== null) {
      clearInterval(autosaveTimer);
      autosaveTimer = null;
    }
    if (!enabled) return;
    autosaveTimer = setInterval(() => {
      for (const tab of tabs.tabs) {
        if (tab.editing && tab.dirty) void ipc.createSnapshot(tab.tabId).catch(() => {});
      }
    }, intervalSec * 1000);
    return () => {
      if (autosaveTimer !== null) {
        clearInterval(autosaveTimer);
        autosaveTimer = null;
      }
    };
  });

  /** 上次异常退出提示：首次拿到设置后检查一次。 */
  let crashChecked = false;
  $effect(() => {
    if (crashChecked || !appSettings) return;
    crashChecked = true;
    void ipc
      .takeCrashFlag()
      .then((crashed) => {
        if (crashed && appSettings?.file.versionHistory) toasts.show(t('snapshot.crashToast'));
      })
      .catch(() => {});
  });

  let snapshotsOpen = $state(false);
  let snapshotsRefresh = $state(0);
  let snapshotRestoreRequest = $state<{ name: string; seq: number } | null>(null);
  let snapshotSeq = 0;

  /** 立即生成快照（仅编辑态）。 */
  async function snapshotNow(): Promise<void> {
    const current = active;
    if (!current || !current.editing) return;
    try {
      const created = await ipc.createSnapshot(current.tabId);
      toasts.show(created ? 'snapshot.created' : 'snapshot.unchanged');
      snapshotsRefresh += 1;
    } catch (error) {
      toasts.error(toIpcError(error).message);
    }
  }

  /** 请求从快照恢复（转发给 EditLayer 执行，结果走单撤销步）。 */
  function requestSnapshotRestore(name: string): void {
    if (!active || !active.editing) {
      toasts.error(t('snapshot.needEdit'));
      return;
    }
    snapshotRestoreRequest = { name, seq: ++snapshotSeq };
  }
</script>

<div class="shell" class:focus-reading={focusReading}>
  <TitleBar
    title={windowTitle}
    showSettings
    onSettings={() => void ipc.openSettings()}
  />
  <MenuBar
    {themeId}
    {themeEntries}
    onThemeChange={setTheme}
    onOpenFile={openFile}
    onQuit={() => void quit()}
    editing={active?.editing ?? false}
    readOnly={active?.readOnly ?? false}
    dirty={active?.dirty ?? false}
    onToggleEdit={() => void toggleEdit()}
    onSave={openSaveDialog}
    hasTab={active !== null}
    onSaveAs={() => void saveAsFlow()}
    onReload={reloadFlow}
    onEditorAction={dispatchEditorAction}
    onToggleFullscreen={() => void toggleFullscreen()}
    autoScroll={autoScrollOn}
    onToggleAutoScroll={toggleAutoScroll}
    focusMode={readerSettings?.reading.focusMode ?? false}
    onToggleFocusMode={toggleFocusMode}
    typewriter={readerSettings?.reading.typewriter ?? false}
    onToggleTypewriter={toggleTypewriter}
          foldingEnabled={foldingEnabled}
  outlineEnabled={appSettings?.display.outline ?? true}
  onToggleOutline={() => (outlineOpen = !outlineOpen)}
  versionHistoryEnabled={appSettings?.file.versionHistory ?? true}
  snapshotEditing={active?.editing ?? false}
  onSnapshotNow={() => void snapshotNow()}
  onSnapshotHistory={() => (snapshotsOpen = true)}
  onNewFile={() => void newFileFlow()}
  onNewWindow={() => void newWindowFlow()}
              onPastePathOpen={() => void pastePathOpen()}
  onExport={() => void exportFlow()}
  onPrint={() => void printFlow()}
          onFoldAll={foldAll}
          onFoldNone={foldNone}
    pomodoroOn={pomodoroOn}
    onTogglePomodoro={togglePomodoro}
    onFontIncrease={() => adjustFontSize(1)}
    onFontDecrease={() => adjustFontSize(-1)}
    onFontReset={resetFontSize}
    onOpenShortcuts={() => void ipc.openSettings('shortcuts')}
    onOpenAbout={() => void ipc.openSettings('about')}
    recent={recentEntries}
    onOpenRecent={(entry) => void historyStore.openEntry(entry)}
    onOpenHistory={() => (historyOpen = true)}
    onToggleBookmark={toggleBookmarkAnnotation}
    onAnnotationsPanel={() => (annotationsOpen = true)}
    onClearAnnotations={() => (annotClearOpen = true)}
    onWorkspaceFind={() => (workspaceOpen = true)}
    onOpenSplit={() => (splitOpen = true)}
    onOpenRename={() => (renameOpen = true)}
    onOpenCompare={() => void openCompareFlow()}
    onOpenMerge={() => void openMergeFlow()}
    workspaceFindEnabled={appSettings?.find.multifileEnabled !== false}
    onSettings={() => void ipc.openSettings()}
    canSplit={paneCount < MAX_PANES}
    canClosePane={paneCount > 1}
    onSplitRight={() => splitPane(tabs.activePane, 'row')}
    onSplitDown={() => splitPane(tabs.activePane, 'column')}
    onClosePane={() => void closePane(tabs.activePane)}
  />
  <ToolBar
    {themeId}
    {themeEntries}
    themeLabel={themeLabel}
    onThemeChange={setTheme}
    onThemeImport={() => void importThemeFlow()}
    onThemeExport={() => void exportThemeFlow()}
    {encodings}
    encodingOverride={active?.encodingOverride ?? null}
    onEncodingChange={(label) => void changeEncoding(label)}
    onOpenFile={openFile}
    editing={active?.editing ?? false}
    readOnly={active?.readOnly ?? false}
    canSave={active?.dirty ?? false}
    onToggleEdit={() => void toggleEdit()}
    onSave={openSaveDialog}
    onSettings={() => void ipc.openSettings()}
    onHistory={() => (historyOpen = true)}
  />
{#snippet paneView(pane: string, _zone: string | null)}
  <PaneView
    {pane}
    active={pane === tabs.activePane}
    compact={paneCount > 1}
    group={tabs.groups[pane] ?? EMPTY_GROUP}
    windows={windowOptions}
    appSettings={appSettings}
    readerSettings={readerSettings}
    editorAction={pane === tabs.activePane ? editorAction : null}
    pageTurn={pane === tabs.activePane ? pageTurnSignal : null}
    foldCommand={pane === tabs.activePane ? foldCommand : null}
    snapshotRestore={pane === tabs.activePane ? snapshotRestoreRequest : null}
    layoutKey={typographyKey}
    editCaretRow={pane === tabs.activePane && caretInfo ? caretInfo.row - 1 : null}
    onOpenDialog={() => void tabs.openViaDialog(pane)}
    onSelect={(tabId) => tabs.select(tabId)}
    onClose={(tabId) => void requestCloseTab(tabId)}
    onCloseOthers={(tabId) => {
      tabs.setActivePane(pane);
      void closeOtherTabs(tabId);
    }}
    onCloseAll={() => {
      tabs.setActivePane(pane);
      void closeAllTabs();
    }}
    onReorder={reorderTab}
    onSetColor={(tabId, color) => void setTabColor(tabId, color)}
    onRequestWindows={() => void refreshWindowOptions()}
    onMoveToWindow={(tabId, label) => void moveTabToWindow(tabId, label)}
    onNewTab={() => void newFileFlow(pane)}
    onNewWindow={() => void newWindowFlow()}
    onSplitRight={() => splitPane(pane, 'row')}
    onSplitDown={() => splitPane(pane, 'column')}
    onClosePane={() => void closePane(pane)}
    onLocalDragMove={(clientX, clientY) => paneDragMove(clientX, clientY)}
    onLocalDrop={(tabId, clientX, clientY) => void paneDragDrop(tabId, clientX, clientY)}
    onLocalDragCancel={() => (hoverPane = null)}
    onPercent={(percent) => {
      if (pane === tabs.activePane) readPercent = percent;
    }}
    onEditApplied={handleEditApplied}
    onUserScroll={handleUserScroll}
    onBreadcrumbJump={(row) => {
      const group = tabs.groups[pane];
      if (group?.activeId != null) jumpStore.request(group.activeId, row, 0, 0);
    }}
    onTopRow={(row) => {
      if (pane === tabs.activePane) topRow = row;
    }}
    onSelectionStats={(stats) => {
      if (pane === tabs.activePane) selectionStats = stats;
    }}
    onCaretInfo={(info) => {
      if (pane === tabs.activePane) caretInfo = info;
    }}
  >
    <EmptyState
      onOpen={openFile}
      recent={recentEntries}
      onOpenRecent={(entry) => void historyStore.openEntry(entry)}
      bindings={shortcuts as Record<string, string>}
    />
  </PaneView>
{/snippet}
  <div class="work-area" class:with-bg={bgActive}>
    {#if bgActive && bgDataUrl}
      <div class="bg-layer" data-bg-layer style={bgLayerStyle}></div>
    {/if}
    <PaneTree
      {layout}
      activePane={tabs.activePane}
      onSizes={updateSplitSizes}
      onActivate={activatePane}
      hover={hoverPane}
      view={paneView}
    />
  </div>
  <StatusBar
    fileName={active?.name}
    percent={readPercent}
    sizeLabel={active ? formatBytes(active.byteLen) : undefined}
    encodingLabel={active?.encoding}
    {encodings}
    encodingOverride={active?.encodingOverride ?? null}
    onEncodingChange={(label) => void changeEncoding(label)}
    {version}
    statusSettings={appSettings?.status ?? null}
    editing={active?.editing ?? false}
    eol={active?.eol ?? null}
    onConvertEol={handleConvertEol}
    docStats={docStats}
    selectionStats={selectionStats}
    lineCol={caretInfo}
    topRow={topRow}
    dirty={active?.dirty ?? false}
    onGotoLine={handleGotoLine}
    readOnly={active?.readOnly ?? false}
    readingSeconds={readingSeconds}
  />
  <Toast />
{#if onboardingOpen}
<Onboarding onClose={(dontShowAgain) => void closeOnboarding(dontShowAgain)} />
{/if}
{#if dataDirIssue}
<DataDirDialog
  dir={dataDirIssue.dir}
  message={dataDirIssue.message}
  onChoose={chooseDataDir}
  onSkip={skipDataDir}
/>
{/if}
<HistoryPanel open={historyOpen} onClose={() => (historyOpen = false)} />
  <AnnotationsPanel
    open={annotationsOpen}
    tabId={active?.tabId ?? 0}
    onClose={() => (annotationsOpen = false)}
    onJump={(row) => {
      if (active) jumpStore.request(active.tabId, row, 0, 0);
    }}
  />
  <OutlinePanel
    tabId={active?.tabId ?? null}
    open={outlineOpen}
    refreshKey={active ? `${active.tabId}:${active.rowsTotal}` : ''}
    onJump={(row) => {
      if (active) jumpStore.request(active.tabId, row, 0, 0);
    }}
    onClose={() => (outlineOpen = false)}
  />
  <SnapshotsPanel
    tabId={active?.tabId ?? null}
    open={snapshotsOpen}
    refreshKey={snapshotsRefresh}
    canRestore={active?.editing ?? false}
    onRestore={requestSnapshotRestore}
    onClose={() => (snapshotsOpen = false)}
  />
  <ConfirmDialog
    open={annotClearOpen}
    title={t('annot.clearTitle')}
    message={t('annot.clearMessage')}
    confirmLabel={t('annot.clear')}
    onConfirm={() => void clearAnnotationsFlow()}
    onCancel={() => (annotClearOpen = false)}
  />
{#if workspaceOpen}
  <WorkspaceFindDialog
    onClose={() => (workspaceOpen = false)}
    onJump={(file, hit) => void jumpToHit(file, hit)}
  />
{/if}
{#if splitOpen}
  <SplitFileDialog currentPath={active?.path ?? null} onClose={() => (splitOpen = false)} />
{/if}
{#if renameOpen}
  <RenameDialog initialDir={renameDefaultDir} onClose={() => (renameOpen = false)} />
{/if}
  <DropOverlay visible={dragging} />
  <SaveDialog
    open={saveRequest !== null}
    currentEncoding={saveDialogTab?.encoding ?? ''}
    {encodings}
    defaultBackup={saveBackup}
    onConfirm={onSaveDialogConfirm}
    onCancel={onSaveDialogCancel}
  />
  <ConfirmDialog
    open={conflictRequest !== null}
    title={t('app.conflict.title')}
    message={t('app.conflict.message')}
    confirmLabel={t('app.conflict.confirmLabel')}
    onConfirm={onConflictOverride}
    onCancel={onConflictCancel}
  />
  <ConfirmDialog
    open={reloadRequest !== null}
    title={t('app.reload.title')}
    message={reloadMessage}
    confirmLabel={t('app.reload.confirmLabel')}
    onConfirm={() => {
      const request = reloadRequest;
      reloadRequest = null;
      if (request) void performReload(request.tabId);
    }}
    onCancel={() => {
      reloadRequest = null;
      focusEditorProxy();
    }}
  />
  <UnsavedDialog
    open={unsavedOpen}
        title={pendingClose?.kind === 'tab' ? t('app.close.tabTitle') : t('app.close.quitTitle')}
    message={closeMessage}
    onSave={() => void resolvePendingClose('save')}
    onDiscard={() => void resolvePendingClose('discard')}
    onCancel={() => void resolvePendingClose('cancel')}
  />
</div>

<style>
  /* 工作区（阅读/空状态）+ 背景图层：背景图启用时内容背景透明，图层垫底 */
  .work-area {
    position: relative;
    display: flex;
    flex: 1;
    min-height: 0;
  }

  /* 专注模式：隐藏菜单/工具栏/标签栏/状态栏（标题栏保留以维持窗口控制与拖拽；Esc 退出） */
  .focus-reading :global(.menu-bar),
  .focus-reading :global(.toolbar),
  .focus-reading :global(.tab-bar),
  .focus-reading :global(.status-bar) {
    display: none;
  }

  .bg-layer {
    position: absolute;
    inset: 0;
    z-index: 0;
    background-position: center;
    pointer-events: none;
  }

  :global(.work-area.with-bg .reader),
  :global(.work-area.with-bg .empty) {
    position: relative;
    z-index: 1;
    background: transparent;
  }

  .shell {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
</style>
