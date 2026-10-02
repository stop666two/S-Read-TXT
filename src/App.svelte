<!--
  App.svelte — 根组件：应用外壳与全局接线。
  已接线：打开（对话框/拖拽）、标签、空状态、Toast、虚拟阅读、编码切换、进度上报、
  编辑/保存/冲突/未保存三态关闭流程（阶段 4b/4c）、冷启动就绪即显。
-->
<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { save } from '@tauri-apps/plugin-dialog';

  import ConfirmDialog from './lib/components/ConfirmDialog.svelte';
  import DropOverlay from './lib/components/DropOverlay.svelte';
  import EmptyState from './lib/components/EmptyState.svelte';
  import MenuBar from './lib/components/MenuBar.svelte';
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
  import { describeIpcError, ipc, toIpcError, type EditApplied } from './lib/ipc';
  import { tabs } from './lib/state/tabs.svelte';
  import { toasts } from './lib/state/toasts.svelte';
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

  /** 编辑动作信号（菜单 → 编辑层；seq 递增区分重复动作） */
  let editorAction = $state<EditorAction | null>(null);
  let editorActionSeq = 0;

  /** 分发编辑动作到编辑层 */
  function dispatchEditorAction(type: EditActionType): void {
    editorActionSeq += 1;
    editorAction = { type, seq: editorActionSeq };
  }

  /** 当前活动标签 */
  const active = $derived(tabs.active);

  /** 窗口标题（自定义标题栏 + document.title：文件名 - 应用名） */
  const windowTitle = $derived(active ? `${active.name} - S-Read-TXT` : 'S-Read-TXT');

  // 同步 document.title（任务栏/Alt-Tab 名称）
  $effect(() => {
    document.title = windowTitle;
  });

  /** 主题切换入口 */
  function setTheme(theme: ThemeChoice): void {
    themeChoice = theme;
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
      await tabs.close(tabId);
      return;
    }
    pendingClose = { kind: 'tab', tabId };
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
      await tabs.close(pending.tabId);
      focusEditorProxy();
    } else {
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

  onMount(() => {
    // 冷启动防空白：页面首帧（主题/骨架）渲染完成后才显示窗口。
    // WebView2 初始化在磁盘压力大时可能耗时较长；隐藏期间用户不会看到空白窗口，
    // Rust 侧另有 8 秒兜底强制显示（防前端异常导致不可见的僵尸进程）。
    void (async () => {
      try {
        await tick();
        // 等待真实首帧绘制（双重 rAF）：tick 只保证 DOM 更新，rAF 之后才有像素，
        // 否则「显示瞬间」仍可能是空窗口（内容晚若干帧才出现）。
        await new Promise<void>((resolve) => {
          requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
        });
        const appWindow = getCurrentWindow();
        await appWindow.show();
        await appWindow.setFocus();
      } catch (error) {
        // 不吞错：显示失败时记录（Rust 兜底仍会在 8 秒后显示窗口）
        if (import.meta.env.DEV) console.error('[app] 窗口显示失败', error);
      }
    })();
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

    // 窗口关闭拦截（X 按钮/系统关闭）：脏标签先走三态确认
    let unlistenClose: (() => void) | undefined;
    void getCurrentWindow()
      .onCloseRequested((event) => {
        if (allowClose || !tabs.tabs.some((tab) => tab.dirty)) return;
        event.preventDefault();
        pendingClose = { kind: 'quit' };
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

    return () => {
      unlisten?.();
      unlistenClose?.();
    };
  });
</script>

<svelte:window
  onkeydown={(event) => {
    // F11 全屏（与查看菜单同款；阶段 5 快捷键引擎接入后统一管理）
    if (event.key === 'F11') {
      event.preventDefault();
      void toggleFullscreen();
    }
  }}
/>

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
  />
  <TabBar
    tabs={tabs.tabs}
    activeId={tabs.activeId}
    onSelect={(tabId) => tabs.select(tabId)}
    onClose={(tabId) => void requestCloseTab(tabId)}
  />
  {#if active}
    <ReaderView
      tab={active}
      onPercent={(percent) => (readPercent = percent)}
      onEditApplied={handleEditApplied}
      {editorAction}
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
