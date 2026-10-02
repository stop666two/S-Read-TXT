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
  import { describeIpcError, ipc, toIpcError, type AppSettings, type EditApplied, type ReaderSettings, type SessionState } from './lib/ipc';
  import { scrollMemory } from './lib/reader/scroll-memory';
  import { saveSessionNow } from './lib/session';
  import { dataDirStore } from './lib/state/data-dir.svelte';
  import { historyStore } from './lib/state/history.svelte';
  import { tabs } from './lib/state/tabs.svelte';
  import { toasts } from './lib/state/toasts.svelte';
  import { decideShortcut, isEditorContext, modalOpen } from './lib/shortcuts/engine';
  import { comboFromEvent } from './lib/shortcuts/keys';
  import type { ShortcutAction, ShortcutMap } from './lib/shortcuts/types';
  import type { ResolvedTheme, ThemeChoice } from './lib/types';

  /** 主题选择（默认跟随系统；阶段 8 起由设置加载/保存） */
  let themeChoice = $state<ThemeChoice>('system');
  /** 文件拖拽悬停（控制遮罩显示） */
  let dragging = $state(false);
  /** 应用版本（无文件时状态栏展示） */
  let version = $state('');
  /** 阅读百分比（由阅读区回报） */
  let readPercent = $state(0);
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
        title: '选择可写的数据目录（本次运行有效）',
      });
      if (typeof picked !== 'string') return;
      const status = await dataDirStore.apply(picked);
      if (status.writable) {
        toasts.show(`数据目录已切换：${status.dir}`);
      } else {
        toasts.error(status.message ?? '所选目录仍不可写，请重试');
      }
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 「数据目录不可写」→ 仅本次只读运行（不保存历史/设置/会话）。 */
  function skipDataDir(): void {
    dataDirStore.skip();
    toasts.show('本次运行不会保存历史、设置与会话数据', 'warn');
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
      themeChoice = snapshot.reader.theme as ThemeChoice;
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
      themeChoice = snapshot.reader.theme as ThemeChoice;
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 字号调整（查看菜单；与后端范围一致的前端钳制，避免无效往返） */
  function adjustFontSize(step: number): void {
    if (!readerSettings) return;
    const current = readerSettings.typography.fontSize;
    const next = Math.min(32, Math.max(12, current + step));
    if (next === current) return;
    void persistReader({ typography: { ...readerSettings.typography, fontSize: next } });
  }

  /** 重置字号为默认（16px） */
  function resetFontSize(): void {
    if (!readerSettings) return;
    void persistReader({ typography: { ...readerSettings.typography, fontSize: 16 } });
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
          toasts.error(`无法恢复「${item.path}」：${describeIpcError(payload)}`);
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

  /** 翻页（阅读态）：约一屏（留 3 行重叠） */
  function scrollPages(step: number): void {
    const element = readerElement();
    if (!element) return;
    const span = Math.max(120, element.clientHeight - 96);
    element.scrollBy({ top: step * span, behavior: 'auto' });
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

  /** 主题切换入口（菜单/工具栏；修改即存） */
  function setTheme(theme: ThemeChoice): void {
    themeChoice = theme;
    void persistReader({ theme });
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

  /** 切换编辑模式（失败走 Toast：超长行 / 标签不存在等） */
  async function toggleEdit(): Promise<void> {
    const tab = active;
    if (!tab) return;
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
      toasts.show(`已保存（${result.encoding}${result.backupPath ? '，已生成 .bak 备份' : ''}）`);
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
      toasts.show(`已另存为「${result.tab.name}」（${result.encoding}）`);
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
          { name: '文本文件', extensions: ['txt', 'log', 'md'] },
          { name: '所有文件', extensions: ['*'] },
        ],
      });
    } catch (error) {
      if (import.meta.env.DEV) console.error('[app] 另存为对话框失败', error);
      toasts.error('无法打开保存对话框');
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
      toasts.show('已重新加载');
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
    const name = tabs.tabs.find((item) => item.tabId === request.tabId)?.name ?? '当前文件';
    return `「${name}」有未保存的修改，重新加载将丢弃这些修改。`;
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
    if (skippedDirty > 0) toasts.show(`已保留 ${skippedDirty} 个有未保存修改的标签`, 'warn');
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
    if (skippedDirty > 0) toasts.show(`已保留 ${skippedDirty} 个有未保存修改的标签`, 'warn');
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
      return `有 ${count} 个标签存在未保存的修改，退出将丢失这些修改。`;
    }
    const name = tabs.tabs.find((item) => item.tabId === pending.tabId)?.name ?? '当前文件';
    return `「${name}」有未保存的修改，关闭将丢失这些修改。`;
  });

  /** 三态弹窗可见性（保存询问/冲突弹窗进行中时让位，避免叠层） */
  const unsavedOpen = $derived(pendingClose !== null && saveRequest === null && conflictRequest === null);

  /** 保存弹窗当前服务的标签信息（默认编码显示用） */
  const saveDialogTab = $derived(tabs.tabs.find((item) => item.tabId === saveRequest?.tabId) ?? null);

  // 解析主题：「跟随系统」依据 prefers-color-scheme，其余直接采用；
  // 结果写入 <html data-theme>，全部令牌随之切换。
  $effect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const apply = (): void => {
      const resolved: ResolvedTheme =
        themeChoice === 'system' ? (media.matches ? 'dark' : 'light') : themeChoice;
      document.documentElement.dataset.theme = resolved;
    };
    apply();
    media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
  });

  /** 排版变更键（传给 ReaderView 触发行高失效重排；值变化即重排） */
  const typographyKey = $derived(readerSettings ? JSON.stringify(readerSettings.typography) : '');

  // 排版令牌写入 CSS 变量：阅读区实时生效（字号/行高/字体/限宽/边距）
  $effect(() => {
    const typo = readerSettings?.typography;
    if (!typo) return;
    const root = document.documentElement;
    root.style.setProperty('--font-reading', `${typo.fontFamily}, system-ui, sans-serif`);
    root.style.setProperty('--reading-size', `${typo.fontSize}px`);
    root.style.setProperty('--reading-line-height', `${typo.lineHeight}`);
    root.style.setProperty('--reading-width', `${typo.contentWidth}px`);
    root.style.setProperty('--reading-pad-x', `${typo.pagePadding}px`);
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
  <TitleBar title={windowTitle} />
  <MenuBar
    {themeChoice}
    onThemeChange={setTheme}
    onOpenFile={openFile}
    onQuit={() => void quit()}
    editing={active?.editing ?? false}
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
  />
  <ToolBar
    {themeChoice}
    onThemeChange={setTheme}
    {encodings}
    encodingOverride={active?.encodingOverride ?? null}
    onEncodingChange={(label) => void changeEncoding(label)}
    onOpenFile={openFile}
    editing={active?.editing ?? false}
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
  <StatusBar
    fileName={active?.name}
    percent={readPercent}
    sizeLabel={active ? formatBytes(active.byteLen) : undefined}
    encodingLabel={active?.encoding}
    {encodings}
    encodingOverride={active?.encodingOverride ?? null}
    onEncodingChange={(label) => void changeEncoding(label)}
    {version}
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
    title="文件已在外部被修改"
    message="磁盘上的文件与打开时不一致，可能被其他程序修改过。仍要覆盖保存吗？"
    confirmLabel="覆盖保存"
    onConfirm={onConflictOverride}
    onCancel={onConflictCancel}
  />
  <ConfirmDialog
    open={reloadRequest !== null}
    title="重新加载"
    message={reloadMessage}
    confirmLabel="重新加载"
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
    title={pendingClose?.kind === 'quit' ? '退出应用' : '关闭标签'}
    message={closeMessage}
    onSave={() => void resolvePendingClose('save')}
    onDiscard={() => void resolvePendingClose('discard')}
    onCancel={() => void resolvePendingClose('cancel')}
  />
</div>

<style>
  .shell {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
</style>
