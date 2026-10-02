<script lang="ts">
  // 菜单栏（应用内自绘）：文件 / 编辑 / 查看 / 帮助。
  // 阶段 2b：完成结构、下拉交互与主题动作；其余动作在后续切片接线（回调缺省即无操作）。
  // 交互约定：点击标题开合；已有菜单打开时悬停切换；点击菜单项执行；Esc / 点击外部关闭。
  import type { ThemeChoice } from '../types';
  import type { EditActionType } from '../edit/actions';

  interface Props {
    /** 当前主题选择（用于「查看」菜单的单选标记） */
    themeChoice: ThemeChoice;
    /** 主题切换回调 */
    onThemeChange: (theme: ThemeChoice) => void;
    /** 打开文件回调（阶段 2c 接线） */
    onOpenFile?: () => void;
    /** 退出回调（阶段 2c 接线） */
    onQuit?: () => void;
    /** 是否处于编辑模式（编辑菜单文案与可用性） */
    editing: boolean;
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
    /** 是否存在活动标签（重新加载可用性） */
    hasTab: boolean;
  }
  let {
    themeChoice,
    onThemeChange,
    onOpenFile,
    onQuit,
    editing,
    dirty,
    onToggleEdit,
    onSave,
    onSaveAs,
    onReload,
    onEditorAction,
    hasTab,
  }: Props = $props();

  /** 菜单名联合类型 */
  type MenuName = 'file' | 'edit' | 'view' | 'help';
  /** 当前展开的菜单（null = 全部收起） */
  let openMenu = $state<MenuName | null>(null);

  /** 主题菜单项（查看菜单内） */
  const themeItems: { value: ThemeChoice; label: string }[] = [
    { value: 'light', label: '浅色' },
    { value: 'dark', label: '深色' },
    { value: 'eye', label: '护眼' },
    { value: 'system', label: '跟随系统' },
  ];

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

<nav class="menu-bar" aria-label="主菜单">
  <button class="title" class:open={openMenu === 'file'} onclick={(e) => { e.stopPropagation(); toggle('file'); }} onmouseenter={() => hoverSwitch('file')}>文件</button>
  <button class="title" class:open={openMenu === 'edit'} onclick={(e) => { e.stopPropagation(); toggle('edit'); }} onmouseenter={() => hoverSwitch('edit')}>编辑</button>
  <button class="title" class:open={openMenu === 'view'} onclick={(e) => { e.stopPropagation(); toggle('view'); }} onmouseenter={() => hoverSwitch('view')}>查看</button>
  <button class="title" class:open={openMenu === 'help'} onclick={(e) => { e.stopPropagation(); toggle('help'); }} onmouseenter={() => hoverSwitch('help')}>帮助</button>

  {#if openMenu === 'file'}
    <div class="dropdown" role="menu" style="left: 4px">
      <button class="item" onclick={() => run(onOpenFile)}><span>打开文件…</span><span class="hint">Ctrl+O</span></button>
      <button class="item" disabled={!hasTab} onclick={() => run(onReload)}><span>重新加载</span></button>
      <div class="separator"></div>
      <button class="item" disabled><span>最近打开</span></button>
      <button class="item" disabled><span>历史记录</span></button>
      <div class="separator"></div>
      <button class="item" onclick={() => run(onQuit)}><span>退出</span></button>
    </div>
  {:else if openMenu === 'edit'}
    <div class="dropdown" role="menu" style="left: 46px">
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('undo'))}>
        <span>撤销</span>
        <span class="hint">Ctrl+Z</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('redo'))}>
        <span>重做</span>
        <span class="hint">Ctrl+Y</span>
      </button>
      <div class="separator"></div>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('cut'))}>
        <span>剪切</span>
        <span class="hint">Ctrl+X</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('copy'))}>
        <span>复制</span>
        <span class="hint">Ctrl+C</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('paste'))}>
        <span>粘贴</span>
        <span class="hint">Ctrl+V</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('selectAll'))}>
        <span>全选</span>
        <span class="hint">Ctrl+A</span>
      </button>
      <div class="separator"></div>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('find'))}>
        <span>查找…</span>
        <span class="hint">Ctrl+F</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(() => onEditorAction?.('replace'))}>
        <span>替换…</span>
        <span class="hint">Ctrl+H</span>
      </button>
      <div class="separator"></div>
      <button class="item" onclick={() => run(onToggleEdit)}>
        <span>{editing ? '退出编辑模式' : '启用编辑模式'}</span>
        <span class="hint">Ctrl+E</span>
      </button>
      <button class="item" disabled={!editing || !dirty} onclick={() => run(onSave)}>
        <span>保存</span>
        <span class="hint">Ctrl+S</span>
      </button>
      <button class="item" disabled={!editing} onclick={() => run(onSaveAs)}>
        <span>另存为…</span>
        <span class="hint">Ctrl+Shift+S</span>
      </button>
    </div>
  {:else if openMenu === 'view'}
    <div class="dropdown" role="menu" style="left: 88px">
      {#each themeItems as item (item.value)}
        <button class="item" onclick={() => run(() => onThemeChange(item.value))}>
          <span class="radio" class:on={themeChoice === item.value}></span>
          <span>{item.label}</span>
        </button>
      {/each}
      <div class="separator"></div>
      <button class="item" disabled><span>字号增大</span><span class="hint">Ctrl+=</span></button>
      <button class="item" disabled><span>字号减小</span><span class="hint">Ctrl+-</span></button>
      <div class="separator"></div>
      <button class="item" disabled><span>全屏</span><span class="hint">F11</span></button>
    </div>
  {:else if openMenu === 'help'}
    <div class="dropdown" role="menu" style="left: 130px">
      <button class="item" disabled><span>快捷键…</span></button>
      <button class="item" disabled><span>关于 S-Read-TXT</span></button>
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
</style>
