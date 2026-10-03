<!--
  App.svelte — 根组件：应用外壳与全局接线。
  已接线：打开（对话框/拖拽）、标签、空状态、Toast、虚拟阅读、编码切换、进度上报、
  编辑/保存/冲突/未保存三态关闭流程（阶段 4b/4c）、冷启动就绪即显。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { open, save } from '@tauri-apps/plugin-dialog';

  import ConfirmDialog from './lib/components/ConfirmDialog.svelte';
  import DataDirDialog from './lib/components/DataDirDialog.svelte';
  import DropOverlay from './lib/components/DropOverlay.svelte';
  import EmptyState from './lib/components/EmptyState.svelte';
  import HistoryPanel from './lib/components/HistoryPanel.svelte';
  import MenuBar from './lib/components/MenuBar.svelte';
  import Onboarding from './lib/components/Onboarding.svelte';
  import ReaderView from './lib/components/ReaderView.svelte';
  import SaveDialog from './lib/components/SaveDialog.svelte';
  import StatusBar from './lib/components/StatusBar.svelte';
  import TabBar from './lib/components/TabBar.svelte';
  import TitleBar from './lib/components/TitleBar.svelte';
  import Toast from './lib/components/Toast.svelte';
  import ToolBar from './lib/components/ToolBar.svelte';
  import UnsavedDialog from './lib/components/UnsavedDialog.svelte';
  import { formatBytes } from './lib/format';
  import type { EditActionType, EditorAction } from './lib/edit/actions';
  import { focusEditorProxy } from './lib/edit/focus';
  import { describeIpcError, ipc, toIpcError, type AppSettings, type EditApplied, type ReaderSettings, type SessionState, type ThemeSummary } from './lib/ipc';
  import { scrollMemory } from './lib/reader/scroll-memory';
  import { saveSessionNow } from './lib/session';
  import { dataDirStore } from './lib/state/data-dir.svelte';
  import { historyStore } from './lib/state/history.svelte';
  import { tabs } from './lib/state/tabs.svelte';
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
  /** 背景图 data URL 缓存与已加载文件名（避免重复读取与闪烁） */
  let bgDataUrl = $state<string | null>(null);
  let bgLoadedFile: string | null = null;
  /** 支持的编码列表（后端提供） */
  let encodings = $state<string[]>([]);
  /** 应用配置（阶段 6：启动加载；设置窗口变更后热刷新） */
  let appSettings = $state<AppSettings | null>(null);
  /** 阅读排版配置（主题 + 排版；变更实时应用） */
  let readerSettings = $state<ReaderSettings | null>(null);
  /** 数据目录不可写状态（共享 store；非空时展示引导弹窗） */
  const dataDirIssue = $derived(dataDirStore.issue);
  /** 历史面板开关（工具栏 / 菜单 / 快捷键共用） */
  let historyOpen = $state(false);
  /** 最近打开（菜单子项；最多 10 条，来自共享历史 store） */
  const recentEntries = $derived(historyStore.entries.slice(0, 10));

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

  /** 生效快捷键绑定（后端为唯一真源；启动加载，设置变更后刷新） */
  let shortcuts = $state<ShortcutMap>({});

  /** 重新载入全部配置快照（启动 / 设置窗口变更事件 / 窗口聚焦兜底） */
  async function reloadSettings(): Promise<void> {
    try {
      const snapshot = await ipc.getSettings();
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
      appSettings = snapshot.app;
      readerSettings = snapshot.reader;
        shortcuts = snapshot.shortcuts.bindings as ShortcutMap;
        themeId = snapshot.reader.theme;
        setLocale(snapshot.app.locale);
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
    let session: SessionState | null = null;
    try {
      session = await ipc.getSession();
    } catch (error) {
      if (import.meta.env.DEV) console.error('[app] 读取会话失败', error);
    }
    try {
      if (!session || session.tabs.length === 0) return;
      const seeds: { tabId: number; row: number }[] = [];
      for (const item of session.tabs) {
        try {
          const info = await ipc.openFile(item.path);
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
          seeds.push({ tabId: info.tabId, row: item.scrollRow });
        } catch (error) {
          const payload = toIpcError(error);
          toasts.error(t('app.session.restoreFailed', { path: item.path, reason: describeIpcError(payload) }));
        }
      }
      // 先预热滚动锚点再应用视图：活动标签首次渲染即可恢复到记录位置
      for (const seed of seeds) {
        scrollMemory.seed(seed.tabId, seed.row);
      }
      tabs.applyView(await ipc.listTabs());
      const target = tabs.tabs[session.activeTabIndex];
      if (target) tabs.select(target.tabId);
    } finally {
      sessionReady = true;
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
    return document.querySelector<HTMLElement>('.reader');
  }

  /** 翻页（阅读态）：约一屏（留 3 行重叠）；平滑动画可经设置关闭 */
  function scrollPages(step: number): void {
    const element = readerElement();
    if (!element) return;
    const span = Math.max(120, element.clientHeight - 96);
    const smooth = readerSettings?.typography.smoothScroll !== false;
    element.scrollBy({ top: step * span, behavior: smooth ? 'smooth' : 'auto' });
  }

  /** 跳到开头 / 结尾（阅读态） */
  function scrollToEdge(edge: 'top' | 'bottom'): void {
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
    }
  }

  /** 窗口标题（自定义标题栏 + document.title：文件名 - 应用名） */
  const windowTitle = $derived(active ? `${active.name} - S-Read-TXT` : 'S-Read-TXT');

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

  /** 切换全屏（查看菜单 / F11；阶段 5 快捷键引擎接入后统一管理） */
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

  /** 退出应用：脏标签走「保存/不保存/取消」三态确认（窗口 X 同样被拦截） */
  async function quit(): Promise<void> {
    if (tabs.tabs.some((tab) => tab.dirty)) {
      pendingClose = { kind: 'quit' };
      return;
    }
    historyStore.flushAll(tabs.tabs);
    await saveSessionNow();
    allowClose = true;
    await getCurrentWindow().close();
  }

  // ---- 编辑与关闭流程（阶段 4b/4c） ----

  /** 保存询问请求（Promise 队列：关闭流程可逐个等待保存结果；
   *  targetPath 存在时表示「另存为」——保存到该路径而非原路径） */
  let saveRequest = $state<{
    tabId: number;
    targetPath?: string;
    resolve: (ok: boolean) => void;
  } | null>(null);
  /** 保存弹窗的备份默认值（首存默认勾选；阶段 8 接入设置后按设置项） */
  const saveBackup = true;
  /** 外部修改冲突弹窗（Promise 化，覆盖/取消都回填原保存请求） */
  let conflictRequest = $state<{
    tabId: number;
    encoding: string | null;
    backup: boolean;
    resolve: (ok: boolean) => void;
  } | null>(null);
  /** 关闭确认待办（脏标签关闭 / 退出应用） */
  let pendingClose = $state<{ kind: 'tab'; tabId: number } | { kind: 'quit' } | null>(null);
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
        defaultPath: tab.path,
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
    } else {
      historyStore.flushAll(tabs.tabs);
      await saveSessionNow();
      allowClose = true;
      await getCurrentWindow().close();
    }
  }

  /** 三态弹窗说明文案 */
  const closeMessage = $derived.by(() => {
    const pending = pendingClose;
    if (!pending) return '';
    if (pending.kind === 'quit') {
      const count = tabs.tabs.filter((item) => item.dirty).length;
      return t('app.close.quitMessage', { count });
    }
    const name = tabs.tabs.find((item) => item.tabId === pending.tabId)?.name ?? t('app.currentFile');
    return t('app.close.tabMessage', { name });
  });

  /** 三态弹窗可见性（保存询问/冲突弹窗进行中时让位，避免叠层） */
  const unsavedOpen = $derived(pendingClose !== null && saveRequest === null && conflictRequest === null);

  /** 保存弹窗当前服务的标签信息（默认编码显示用） */
  const saveDialogTab = $derived(tabs.tabs.find((item) => item.tabId === saveRequest?.tabId) ?? null);

  // 主题应用（P0-5）：设置变化时解析令牌写入 CSS 变量；跟随系统时监听系统明暗切换。
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

  /** 排版变更键（传给 ReaderView 触发行高失效重排；值变化即重排） */
  const typographyKey = $derived(readerSettings ? JSON.stringify(readerSettings.typography) : '');

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
    root.style.setProperty('--reading-pad-x', `${typo.pagePadding}px`);
    root.style.setProperty('--reading-pad-y', `${typo.pagePaddingY}px`);
    root.style.setProperty('--reading-para-spacing', `${typo.paragraphSpacing}px`);
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

    // 窗口关闭拦截（X 按钮/系统关闭）：统一走退出流程——
    // 保存会话 → 脏标签三态确认 → 关闭（避免 X 直关时丢失最后滚动位置）
    let unlistenClose: (() => void) | undefined;
    void getCurrentWindow()
      .onCloseRequested((event) => {
        if (allowClose) return;
        event.preventDefault();
        if (tabs.tabs.some((tab) => tab.dirty)) {
          pendingClose = { kind: 'quit' };
          return;
        }
        void (async () => {
          historyStore.flushAll(tabs.tabs);
          await saveSessionNow();
          allowClose = true;
          await getCurrentWindow().close();
        })();
      })
      .then((stop) => {
        unlistenClose = stop;
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
    let unlistenSettings: (() => void) | undefined;
    void listen('srt://settings-changed', () => void reloadSettings()).then((stop) => {
      unlistenSettings = stop;
    });
    let unlistenFocus: (() => void) | undefined;
    void getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) void reloadSettings();
      })
      .then((stop) => {
        unlistenFocus = stop;
      });

    // 全局快捷键（捕获阶段：先于编辑层与浏览器默认行为）
    const onGlobalKeydown = (event: KeyboardEvent): void => {
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
      unlisten?.();
      unlistenClose?.();
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
</script>

<div class="shell">
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
    onFontIncrease={() => adjustFontSize(1)}
    onFontDecrease={() => adjustFontSize(-1)}
    onFontReset={resetFontSize}
    onOpenShortcuts={() => void ipc.openSettings('shortcuts')}
    onOpenAbout={() => void ipc.openSettings('about')}
    recent={recentEntries}
    onOpenRecent={(entry) => void historyStore.openEntry(entry)}
    onOpenHistory={() => (historyOpen = true)}
    onSettings={() => void ipc.openSettings()}
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
<TabBar
  tabs={tabs.tabs}
  activeId={tabs.activeId}
  onSelect={(tabId) => tabs.select(tabId)}
  onClose={(tabId) => void requestCloseTab(tabId)}
  onCloseOthers={(tabId) => void closeOtherTabs(tabId)}
  onCloseAll={() => void closeAllTabs()}
  onReorder={reorderTab}
/>
  <div class="work-area" class:with-bg={bgActive}>
    {#if bgActive && bgDataUrl}
      <div class="bg-layer" data-bg-layer style={bgLayerStyle}></div>
    {/if}
    {#if active}
      <ReaderView
        tab={active}
        onPercent={(percent) => (readPercent = percent)}
        onEditApplied={handleEditApplied}
        {editorAction}
        layoutKey={typographyKey}
      />
    {:else}
      <EmptyState onOpen={openFile} />
    {/if}
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
    showFileName={readerSettings?.statusBar.showFileName ?? true}
    showPercent={readerSettings?.statusBar.showPercent ?? true}
    showSize={readerSettings?.statusBar.showSize ?? true}
    showEncoding={readerSettings?.statusBar.showEncoding ?? true}
    readOnly={active?.readOnly ?? false}
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
    title={pendingClose?.kind === 'quit' ? t('app.close.quitTitle') : t('app.close.tabTitle')}
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
