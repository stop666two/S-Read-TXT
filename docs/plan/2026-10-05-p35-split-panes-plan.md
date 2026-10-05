# P3-5 分屏（每栏独立标签组）实施计划（2026-10-05）

> **执行方式：** 本会话内逐任务实施（implement + tdd），每任务独立提交；实施者需先读本文件再动手。

**目标：** 窗口内可分屏为最多 4 栏（任意树形切分），每栏拥有独立的标签组与时序（VSCode 编辑组模型）；标签可在栏间、窗口间拖拽；布局与每栏标签随会话恢复。

**架构：** owner 键从「窗口 label」演进为「栏位键 `label#n`」（纯字符串，后端 `orders/active/tabs.owner` 天然兼容）；布局树（方向/比例/栏位引用）由前端持有并随会话 v3 持久化；拖放协议扩展 client_y，跨窗落点改由**目标窗口前端**解析（栏位 + 边缘分屏），源窗与目标窗共用同一套 `resolvePaneDrop` 解析逻辑。

**技术栈：** Rust（AppState/命令/会话）、Svelte 5 runes、Tauri 2 事件、smoke E2E（CDP）。

## 全局约束

- 会话 schema：`src-tauri/src/session/model.rs` `SESSION_SCHEMA_VERSION` 2 → **3**；v1/v2 自动迁移，不得破坏旧文件读取。
- 栏位键格式固定：`` `${windowLabel}#${n}` ``（n 从 1 起）；窗口关闭/前缀匹配一律用 `label#` 前缀。
- 单栏时 UI 与现状**完全一致**（工具条按钮、菜单、快捷键行为不回退）。
- 分屏上限 **4 栏**；超出时 toast 提示并拒绝。
- 快捷键动作 16 → **19**（新增 `splitRight`/`splitDown`/`closePane`），`check-shortcut-parity` 必须通过；所有断言 16 的测试点同步到 19（清单见任务 5）。
- 关栏=标签无损并入相邻栏；唯一栏时禁用关栏。
- 禁止注释代码、禁止 PowerShell 改文本（用 edit/write）、临时物仅入 `tmp/`。
- 每任务完成即 `git commit`（`feat(ai):`/`fix(ai):`/`test(ai):` 前缀），并更新 `docs/plan/progress.md`。

---

## 文件结构（新建/修改一览）

| 文件 | 责任 |
| --- | --- |
| `src-tauri/src/app_state.rs` | 栏位键工具函数、`close_window_panes`、`move_tab` 兼容（owner 即栏位键） |
| `src-tauri/src/commands.rs` | `pane` 参数注入（list/new/open）、`move_tab_to_pane`、`close_window_tabs` 改前缀、`move_tab_to_window` 目标键 |
| `src-tauri/src/session/model.rs` | v3 类型：`PaneSession`/`PaneLayout`/`PaneSplitDir`、`WindowSession` 扩展 |
| `src-tauri/src/session/store.rs` | v2→v3 迁移、布局净化（leaves ⊆ panes） |
| `src-tauri/src/tab_drag.rs` | Hover/Dropped payload 加 `client_y`；跨窗落点只派发事件不搬数据；桌面新窗用 `#1` 键 |
| `src/lib/state/tabs.svelte.ts` | `groups: Record<pane, GroupState>` + `activePane` 重构 |
| `src/lib/session.ts` | collect 输出 panes/layout/focusedPane |
| `src/lib/components/PaneTree.svelte` **新** | 布局树渲染 + 分隔条拖拽 + 边缘/栏位拖放预览 |
| `src/lib/components/PaneView.svelte` **新** | 单栏：TabBar + ReaderView + 空栏占位 + 栏内拖放解析 |
| `src/lib/components/TabBar.svelte` | 多栏紧凑模式、per-pane 工具条、拖拽坐标上抛 App |
| `src/lib/components/MenuBar.svelte` | 查看菜单：向右拆分/向下拆分/关闭栏位 |
| `src/lib/components/EditLayer.svelte` | `paneActive` 门控 focusin 抢焦 |
| `src/lib/edit/focus.ts` | 代理聚焦改为「激活栏内」查询 |
| `src/App.svelte` | 布局状态、activePane、拆分/关栏动作、快捷键、会话、拖放解析 |
| `src/lib/shortcuts/types.ts`、`src-tauri/src/settings/defaults.rs` | 3 个新动作与默认键 |
| `src/lib/i18n/zh-CN.ts`、`en.ts` | 菜单/快捷键/分屏提示文案 |
| `scripts/smoke-split.mjs` **新** | E2E 套件（任务 9 并入 verify-all） |
| 多处测试 | 计数 16→19、schemaVersion 2→3 |

---

### 任务 1：后端栏位键与命令（契约先行）

**Files:**
- Modify: `src-tauri/src/app_state.rs`、`src-tauri/src/commands.rs`
- Test: `src-tauri/tests/adversarial_flow.rs`、`src-tauri/src/app_state.rs`（`#[cfg(test)] mod tests`）

**Interfaces:**
- 生产：
  - `pub fn pane_key(label: &str, index: u32) -> String`（app_state.rs；`format!("{label}#{index}")`）
  - `pub fn default_pane(label: &str) -> String`（= pane_key(label, 1)）
  - `AppState::close_window_panes(&mut self, label: &str) -> Vec<u64>`：关闭 `owner` 以 `format!("{label}#")` 开头的全部标签（复用 `close()`；父级 order/active 清理由 `settle_after_detach` 完成）
  - `commands`: `list_tabs(window, pane: Option<String>)`、`new_file(window, pane: Option<String>)`、`open_file(window, path, pane: Option<String>)`；内部 `fn resolve_pane(window: &WebviewWindow, pane: Option<String>) -> String`（pane 为 `Some` 且以 `format!("{}#", label)` 开头时采用，否则 `default_pane(label)`）
  - 新命令 `move_tab_to_pane(app, tab_id: u64, target_pane: String, to_index: Option<u32>) -> Result<TabsView, IpcError>`：`guard.move_tab(tab_id, &target_pane, to_index.unwrap_or(usize::MAX as u32) as usize)`；返回**源窗**视图（`owner_of` 先取源）；`app.emit(EVENT_TABS_CHANGED, ())`
  - `move_tab_to_window(app, tab_id, target_label)`：目标键改为 `default_pane(&target_label)`
  - `close_window_tabs(window)`：改调 `close_window_panes(label)`
- 消费（后续任务）：前端 `ipc.listTabs(pane)`、`ipc.openFile(path, pane)`、`ipc.newFile(pane)`、`ipc.moveTabToPane(tabId, pane, index?)`

- [ ] **Step 1：写失败测试（Rust）**

在 `app_state.rs` 测试模块新增：

```rust
#[test]
fn pane_close_closes_only_matching_window_prefix() {
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let dir = tempdir().unwrap();
    // 造文件 a/b
    let a = dir.path().join("a.txt");
    std::fs::write(&a, "a").unwrap();
    let b = dir.path().join("b.txt");
    std::fs::write(&b, "b").unwrap();
    let (ta, _) = state.open_file("main#1", &a, &settings).unwrap();
    let (tb, _) = state.open_file("main#2", &b, &settings).unwrap();
    let (tc, _) = state.open_file("main-2#1", &b, &settings).unwrap();
    let closed = state.close_window_panes("main");
    assert_eq!(closed.len(), 2);
    assert!(closed.contains(&ta.tab_id) && closed.contains(&tb.tab_id));
    assert_eq!(state.tabs_info("main-2#1").len(), 1);
    assert_eq!(state.tabs_info("main-2#1")[0].tab_id, tc.tab_id);
}

#[test]
fn move_tab_between_panes_moves_owner_and_order() {
    let mut state = AppState::new();
    let settings = AppSettings::default();
    let dir = tempdir().unwrap();
    let a = dir.path().join("a.txt");
    std::fs::write(&a, "a").unwrap();
    let (t, _) = state.open_file("main#1", &a, &settings).unwrap();
    assert!(!state.move_tab(t.tab_id, "main#2", usize::MAX).unwrap());
    assert_eq!(state.tabs_info("main#1").len(), 0);
    assert_eq!(state.tabs_info("main#2")[0].tab_id, t.tab_id);
    assert_eq!(state.owner_of(t.tab_id).as_deref(), Some("main#2"));
}
```

- [ ] **Step 2：运行确认失败**

`cargo test --manifest-path src-tauri/Cargo.toml pane_close -- --nocapture` → 编译错误/断言失败（`close_window_panes` 未定义）。

- [ ] **Step 3：实现**

`app_state.rs`：加 `pane_key/default_pane`；`close_window_panes`（`let prefix = format!("{label}#"); let ids: Vec<u64> = self.tabs.values().filter(|t| t.owner.starts_with(&prefix)).map(|t| t.id).collect(); for id in &ids { self.close(*id); } ids`）。

`commands.rs`：`resolve_pane`；三个命令加 `pane: Option<String>` 参数（Tauri 可选参数自动按 `null` 传入，前端不传即默认栏）；`move_tab_to_pane`；`move_tab_to_window` 目标键；`close_window_tabs` 改前缀。

- [ ] **Step 4：运行全量 Rust 测试**

`cargo test --manifest-path src-tauri/Cargo.toml` → 全绿（现有 432+ 断言不动；`adversarial_flow.rs` 里 owner 用 `"main"` 的用例不改也能过，栏位键只是字符串）。

- [ ] **Step 5：提交**

`git add -A && git commit -m "feat(ai): 栏位键与命令契约（P3-5 任务1）——close_window_panes/move_tab_to_pane/pane 参数默认 #1"`

---

### 任务 2：会话 v3（每窗 panes/layout/focusedPane）

**Files:**
- Modify: `src-tauri/src/session/model.rs`、`src-tauri/src/session/store.rs`
- Test:同文件 `#[cfg(test)] mod tests`（沿用现有测试风格）

**Interfaces:**
- 生产：
  ```rust
  pub const SESSION_SCHEMA_VERSION: u32 = 3;

  #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
  #[serde(rename_all = "camelCase")]
  pub enum PaneSplitDir { Row, Column }

  #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
  #[serde(tag = "type", rename_all = "camelCase")]
  pub enum PaneLayout {
      Leaf { pane: String },
      Split { dir: PaneSplitDir, sizes: Vec<f64>, children: Vec<PaneLayout> },
  }

  #[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
  #[serde(rename_all = "camelCase")]
  pub struct PaneSession {
      pub pane: String,
      pub active_tab_index: u32,
      pub tabs: Vec<SessionTab>,
  }
  ```
  `WindowSession`：`pub panes: Vec<PaneSession>`、`pub layout: Option<PaneLayout>`、`pub focused_pane: Option<String>`；旧字段改名 `legacy_tabs/legacy_active_tab_index`（`#[serde(rename = "tabs"/"activeTabIndex", skip_serializing, default)]`）。`WindowState` 不变。
- store 净化：`panes` 去重、限 4、`layout` 叶子校验（未知 pane 剔除、缺失 pane 追加为叶；单叶布局收敛为 `Leaf`）；迁移：无 panes 且 legacy 有内容 → `panes=[{pane: default_pane(label), tabs, active_tab_index}]`、`layout=Leaf(pane)`、`focused_pane=Some(pane)`。

- [ ] **Step 1：写失败测试**

```rust
#[test]
fn migrates_v2_slice_to_single_pane() {
    let dir = tempdir().unwrap();
    let v2 = r#"{
      "schemaVersion": 2,
      "focusedLabel": "main",
      "windows": [{ "label": "main", "window": { "width": 1100, "height": 760, "maximized": false },
        "activeTabIndex": 1, "tabs": [ { "path": "C:/a.txt", "encoding": null, "scrollRow": 3, "editMode": false, "color": null } ] }]
    }"#;
    std::fs::write(dir.path().join("session.json"), v2).unwrap();
    let s = store::load(dir.path());
    assert_eq!(s.schema_version, 3);
    let w = &s.windows[0];
    assert_eq!(w.panes.len(), 1);
    assert_eq!(w.panes[0].pane, "main#1");
    assert_eq!(w.panes[0].active_tab_index, 1);
    assert!(matches!(w.layout, Some(PaneLayout::Leaf { ref pane }) if pane == "main#1"));
}

#[test]
fn v3_roundtrip_keeps_layout_and_panes() { /* 构造含 row split 两栏 + focusedPane 的 WindowSession → save → load → 相等 */ }

#[test]
fn layout_sanitize_drops_unknown_and_appends_missing() { /* panes=[m#1,m#2]，layout 引用 m#9 + 缺 m#2 → 叶子收敛为 [m#2, m#1]（或追加）；至少不丢 pane */ }
```

- [ ] **Step 2：运行确认失败**（`migrates_v2_slice_to_single_pane` 失败：schema 仍 2）

- [ ] **Step 3：实现模型与净化迁移**（依 Interfaces；`legacy_*` 反序列化接受旧 JSON）

- [ ] **Step 4：全量 Rust 测试** → 全绿（现有 `session/store` 测试若有 v2 断言同步更新）

- [ ] **Step 5：提交** `feat(ai): 会话 v3（每窗分栏布局与标签）（P3-5 任务2）`

---

### 任务 3：拖放协议扩展（client_y 与「目标窗自解析」）

**Files:**
- Modify: `src-tauri/src/tab_drag.rs`
- （前端消费在任务 7；本任务仅后端 + 保持现有前端不崩：payload 新增字段是向后兼容的）

**Interfaces:**
- `HoverPayload { active: bool, client_x: f64, client_y: f64 }`、`DroppedPayload { tab_id: u64, client_x: f64, client_y: f64 }`
- `drag_end` 分支：
  - 落回源窗 → 发 `EVENT_DRAG_DROPPED`（含 y）；
  - 落其他主窗 → **只发** `EVENT_DRAG_DROPPED` 给目标窗（不再 `move_tab`/广播；由目标前端调 `move_tab_to_pane`）；
  - 桌面 → `build_main_window` 后 `move_tab(tab_id, &default_pane(&label), MAX)` + 广播（该窗无前端状态，`#1` 键确定）。
- `client_y` 计算：`(y - inner.y) / scale`（与 client_x 同源，`hit_test` 已取 inner 位置与 scale）。

- [ ] **Step 1：实现 payload 与分支调整**（无 Rust 单测面；由任务 7/9 E2E 覆盖）
- [ ] **Step 2：`cargo test` 全绿 + `cargo fmt`**
- [ ] **Step 3：提交** `feat(ai): 拖放协议携带 client_y；跨窗落点改为目标窗处理（P3-5 任务3）`

---

### 任务 4：前端分组状态重构（单栏行为不变）

**Files:**
- Modify: `src/lib/state/tabs.svelte.ts`、`src/lib/ipc.ts`、`src/lib/session.ts`、`src/App.svelte`
- Test: `src/lib/state/tabs.svelte.test.ts`（若存在则扩展；否则新增单测）、E2E 回归子集

**Interfaces:**
- `ipc.ts`：`listTabs(pane?: string)`、`openFile(path, pane?)`、`newFile(pane?)`、`moveTabToPane(tabId, targetPane, toIndex?)`；`WindowSession` 类型加 `panes/layout/focusedPane`（任务 8 用，本任务先加类型占位可选字段）
- `tabs.svelte.ts`：
  ```ts
  export interface GroupState { tabs: TabInfo[]; activeId: number | null }
  class TabStore {
    groups = $state<Record<string, GroupState>>({});
    activePane = $state('');
    get group(): GroupState;            // groups[activePane] 或空组
    get active(): TabInfo | null;       // activePane 组的活动标签（状态栏/标题沿用）
    applyView(pane: string, view: TabsView): void;
    async openViaDialog(pane: string): Promise<void>;
    async openPath(pane: string, path: string): Promise<void>;
    select(pane: string, tabId: number): void;
    reorder(pane: string, tabId: number, toIndex: number): void;
    async close(tabId: number): Promise<void>;
    update(info: TabInfo): void;
  }
  export const tabs = new TabStore();
  ```
- `App.svelte`：`const defaultPane = `${getCurrentWindow().label}#1``；`tabs.activePane = defaultPane`（初始化）；所有调用点补 pane（`listTabs(defaultPane)`、`openPath(defaultPane, p)`、`tabs.select(pane, id)`、`tabs.reorder(pane, id, idx)`）；TabBar 传 `tabs.group.tabs/activeId`。

- [ ] **Step 1：单测先行**（若 store 已有测试文件则扩展）：
  - `applyView('main#1', view)` 后 `groups['main#1'].tabs` 相等、`activePane='main#1'` 时 `active` 正确；
  - `activePane='main#2'` 切换后 `active` 指向另一组；
  - `reorder/select` 调用 `ipc` 的 pane 参数正确（mock ipc）。
- [ ] **Step 2：实现重构**（按 Interfaces；保持 TabStore 方法逻辑不变，仅加 pane 维度）
- [ ] **Step 3：静态与单测**：`npx svelte-check`、`npx vitest run` 全绿
- [ ] **Step 4：E2E 回归子集**：`node scripts/smoke-tabs.mjs`、`smoke-windows.mjs`、`smoke-session.mjs`、`smoke-cli.mjs` 全绿（证明 owner 默认 #1 不破坏现有行为；smoke-session 若断言 schemaVersion=2 → 本任务不改断言，任务 8 统一）
- [ ] **Step 5：提交** `refactor(ai): 前端标签状态按栏位分组（P3-5 任务4）`

---

### 任务 5：分屏 UI（拆分/合并/比例/工具条/菜单/快捷键）

**Files:**
- Create: `src/lib/components/PaneTree.svelte`、`src/lib/components/PaneView.svelte`
- Modify: `TabBar.svelte`、`MenuBar.svelte`、`App.svelte`、`shortcuts/types.ts`、`settings/defaults.rs`、i18n ×2
- Test: 覆盖点见 Steps；E2E `smoke-split.mjs`（本任务建立初版）

**Interfaces:**
- `App.svelte` 状态与函数：
  ```ts
  let layout = $state<PaneLayout>({ type: 'leaf', pane: `${label}#1` });
  let paneSeq = 2;
  const paneKeys = $derived(collectLeaves(layout));       // string[]
  const paneCount = $derived(paneKeys.length);
  function splitPane(target: string, dir: 'row' | 'column'): string | null;  // 满 4 返回 null 并 toast
  function closePane(target: string): void;               // 标签迁移后折叠布局
  function setSplitSizes(path: number[], sizes: number[]): void;
  ```
- `PaneTree.svelte` props：`{ layout, groups, activePane, panesMeta… , onSplit, onClose, onSizes, onActivate }`；递归 `{#if layout.type==='leaf'}` → `PaneView`，`{:else}` → 容器 + 子节点 + `.splitter`（pointerdown 捕获、axis 计算、最小 15%）。
- `PaneView.svelte` props：`{ pane, group, active, …TabBar/ReaderView 全部既有 props }`；空栏渲染 `tabs.pane.empty`（提示 + 「打开文件…」按钮 → `tabs.openViaDialog(pane)`）。
- `TabBar.svelte` 新 props：`compact?: boolean`（多栏样式：高度 30px、字号 12px）、`showWindowActions?: boolean`（=paneCount===1）、`onSplitRight?/onSplitDown?/onClosePane?`；`data-pane={pane}` 根属性。多栏按钮序：`[+] [⊞右分] [⊟下分] [✕关栏]`。
- `MenuBar.svelte` 查看菜单插入（separator 后）：`menu.view.splitRight`（hint Ctrl+\）、`menu.view.splitDown`（Ctrl+Shift+\）、`menu.view.closePane`（Ctrl+Shift+W）；`disabled={paneCount >= 4}` / `disabled={paneCount <= 1}`；回调 `onSplitRight/onSplitDown/onClosePane`。
- 快捷键：`types.ts` `ShortcutAction` 加三动作；`defaults.rs` `DEFAULT_BINDINGS` 加 `("splitRight","Ctrl+\\") ("splitDown","Ctrl+Shift+\\") ("closePane","Ctrl+Shift+W")`；i18n `shortcut.splitRight/splitDown/closePane` ×2 语言。
- 布局工具（放 `src/lib/layout/pane-tree.ts` 新文件，纯函数 + 单测）：
  ```ts
  export function collectLeaves(node: PaneLayout): string[];
  export function replaceLeaf(node: PaneLayout, target: string, make: (leaf: string) => PaneLayout): PaneLayout;
  export function removeLeaf(node: PaneLayout, target: string): PaneLayout | null;  // 折叠单孩子
  export function findSplit(node: PaneLayout, pane: string): { path: number[]; dir: 'row'|'column' } | null;
  ```

- [ ] **Step 1：纯函数单测（`src/lib/layout/pane-tree.test.ts`）**：collect/replace/remove（含折叠 2x2→1x2→单叶）、findSplit 路径正确；
- [ ] **Step 2：实现 pane-tree.ts + PaneTree/PaneView 骨架**（拆分按钮可用；空栏提示）
- [ ] **Step 3：接入拆分/关栏/菜单/快捷键**（含 16→19 全清单同步：`defaults.rs` 测试、`shortcut_io.rs:177`、`smoke-settings` 6 处、`smoke-settings-io` E11、`smoke-buttons` D12b）
- [ ] **Step 4：手动冒烟**：tauri build debug → 开文件 → 右分（按钮/菜单/Ctrl+\）→ 4 栏上限 toast → 关栏（标签并入）→ 分隔条拖动 → 单栏回归（smoke-tabs 跑一遍）
- [ ] **Step 5：提交** `feat(ai): 窗口内分屏（P3-5 任务5）——布局树/拆分合并/分隔条/菜单与快捷键 19 动作`

---

### 任务 6：激活栏语义与焦点/信号隔离

**Files:**
- Modify: `App.svelte`、`ReaderView.svelte`、`EditLayer.svelte`、`edit/focus.ts`、`PaneView.svelte`

**Interfaces:**
- `App`：`activePane` 由 `PaneTree` 的 `onActivate`（栏内 pointerdown）设置；包装器属性 `data-pane={key}`、激活栏加 `data-pane-active="true"`。
- `focus.ts`：
  ```ts
  export function focusEditorProxy(): void {
    const scope = document.querySelector('[data-pane-active="true"]') ?? document;
    scope.querySelector<HTMLTextAreaElement>('textarea.input-proxy')?.focus();
  }
  ```
- `App` 内 `readerElement()` 与自动滚动循环：`document.querySelector('[data-pane-active="true"] .reader')`（兜底第一个 `.reader`）。
- 信号过滤：`editorAction`/`pageTurnSignal`/`foldCommand`/`snapshotRestoreRequest` 仅当 `key === activePane` 时传给该 `PaneView`（其余传 `null`）。
- `ReaderView.svelte` 加 prop `activePane?: boolean`（默认 true），透传 `EditLayer` 新 prop `paneActive`；`EditLayer.handleFocusIn` 开头 `if (paneActive === false) return;`。
- 快捷键作用域：`runShortcut` 的 `closeTab/nextTab/prevTab/find/replace/fixedTab` 全部基于 `tabs.group`（即 activePane 组）。

- [ ] **Step 1：实现隔离**（按 Interfaces）
- [ ] **Step 2：E2E 探针（tmp/probe-panescope.mjs，真断言）**：两栏各开一文件 → 点栏 B 输入 Ctrl+W → 仅 B 组变化；点栏 A → Ctrl+F 仅 A 栏出现 `.find-bar`；编辑态两栏轮流点击输入不互抢焦点（焦点后 activeElement 在点击栏内）。
- [ ] **Step 3：`svelte-check` + vitest + 探针全过；`smoke-tabs/find` 回归**
- [ ] **Step 4：提交** `feat(ai): 激活栏作用域（焦点/信号/快捷键按栏路由）（P3-5 任务6）`

---

### 任务 7：栏间与跨窗拖放（重排/移动/边缘分屏）

**Files:**
- Modify: `TabBar.svelte`、`PaneTree.svelte`、`App.svelte`、`tab-dnd.ts`（如需）
- Test: `tmp/probe-panedrag.mjs` + `smoke-split.mjs` 增补

**Interfaces:**
- `App` 新增：
  ```ts
  interface PaneDropTarget { pane: string; zone: 'tabBar' | 'center' | 'edge-left' | 'edge-right' | 'edge-top' | 'edge-bottom' }
  function resolvePaneDrop(clientX: number, clientY: number): PaneDropTarget | null; // 遍历 [data-pane] 矩形
  function handlePaneDrop(tabId: number, clientX: number, clientY: number): void;   // 统一落点执行
  ```
  `handlePaneDrop` 规则：同栏 tabBar → `tabs.reorder`（`computeDropHit`）；同栏 center/edge → 边缘才 `splitPane` 后 `moveTabToPane`，center 忽略；他栏 tabBar → `moveTabToPane(tabId,pane,index)`；他栏 center → `moveTabToPane(tabId,pane,MAX)`；他栏边缘 → `splitPane` + `moveTabToPane`。
- `TabBar` 本地拖动改为**坐标上抛**：`onLocalDragMove?(x,y)`、`onLocalDrop?(x,y)`（仍保留「出窗转原生」）；自身栏内指示线由 App 回传 `dropLineLeft?: number` 渲染（跨窗 hover 同源）。
- 跨窗：目标窗收到 `srt://tab-drag-dropped` `{tabId, clientX, clientY}` → `handlePaneDrop`（该窗自己的 DOM 解析）；源窗收到同事件时若 tab 已不在本窗（被迁走）则 `refreshTabsView` 即可（幂等）。
- 预览：`PaneTree` 渲染三类反馈——栏高亮边框、边缘半区遮罩（行/列方向）、标签条插入线。
- `tab_drag.rs` hover 触发拖出时 `client_y` 已具备（任务 3）；`TabBar` 的 hover 监听改为转发 App。

- [ ] **Step 1：实现本地坐标上抛与 `resolvePaneDrop`**
- [ ] **Step 2：实现落点执行与预览**
- [ ] **Step 3：探针（真断言）**：栏 A 标签拖到栏 B 标签条中段 → B 组插入位置正确；拖到栏 B 右缘 → 新栏出现在右且标签在新栏；拖回 A 重排；拖到桌面 → 新窗口 `main-N#1` 显示该标签；Esc 取消无变化
- [ ] **Step 4：回归** `smoke-tabs`、`smoke-windows` 全绿
- [ ] **Step 5：提交** `feat(ai): 栏间拖放与边缘分屏（P3-5 任务7）`

---

### 任务 8：会话 v3 前端接入（收集/恢复/迁移）

**Files:**
- Modify: `src/lib/session.ts`、`App.svelte`、`ipc.ts` 类型
- Test: `smoke-split.mjs` 持久化用例 + `smoke-session.mjs` 断言更新

**Interfaces:**
- `collectSession`：输出 `panes`（布局叶子顺序；每栏 `{ pane, activeTabIndex: 组内过滤后下标, tabs: 过滤 untitled 后的 SessionTab[] }`）、`layout`（`$state.snapshot(layout)`）、`focusedPane: activePane`；**删除**顶层 `tabs/activeTabIndex`（v3 语义）。
- `restoreSession`：`getSession()` 后若 `session.panes.length > 0`：`layout = session.layout ?? leaf(panes[0].pane)`；`paneSeq = maxSuffix(paneKeys)+1`；`activePane = session.focusedPane ?? paneKeys[0]`；逐栏逐项 `openFile(item.path, pane)`（+encoding/editMode/color）并记录 `{pane, tabId, row}` 种子；全部完成后**逐栏** `applyView(pane, await listTabs(pane))` 并按 `activeTabIndex` `setActiveTab` 选中活动项；`scrollMemory.seed` 沿用。
- `startup.restoreSession === false` 与空会话分支：`applyView(defaultPane, await listTabs())` 不变。
- `smoke-session.mjs`：`schemaVersion` 断言 2 → 3；切片字段从 `windows[0].tabs` 改为 `windows[0].panes[0].tabs`（含 activeTabIndex 位置）。

- [ ] **Step 1：实现收集与恢复**
- [ ] **Step 2：在 `smoke-split.mjs` 增补**：两栏各开不同文件 → quit → 重启 → 两栏结构与每栏标签/激活项恢复；`smoke-session.mjs` 更新并单跑全绿
- [ ] **Step 3：v2 迁移 E2E**：探针写入手造 v2 session.json → 启动 → 单栏正常恢复（验证任务 2 的迁移端到端）
- [ ] **Step 4：提交** `feat(ai): 分屏会话持久化与 v2 迁移（P3-5 任务8）`

---

### 任务 9：E2E 套件收口与全量验证

**Files:**
- Modify: `scripts/smoke-split.mjs`（定稿）、`scripts/verify-all.mjs`、`README.md`、`CHANGELOG.md`、`docs/plan/progress.md`、`docs/verify/coverage-matrix.md`、`docs/plan/2026-10-05-p35-split-panes-plan.md`（勾选完成）

**smoke-split 用例清单（真实断言，编号 S1–S18）：**
- S1 标签栏右分按钮 → 2 栏；S2 右分快捷键 Ctrl+\ → 3 栏；S3 下分菜单 → 4 栏；S4 第 5 次拆分拒绝（toast `tabs.pane.limit`）；
- S5 两栏独立标签组（B 栏打开文件，A 栏 tabs 不变）；S6 空栏占位与「打开文件」入口；
- S7 点击切栏后 Ctrl+W 只影响该栏；S8 点击切栏后 Ctrl+F 只在该栏出现查找条；
- S9 关栏标签并入相邻栏；S10 唯一栏关栏禁用；
- S11 分隔条拖动改变尺寸（DOM 宽度比变化）；S12 尺寸随会话恢复；
- S13 拖标签到另一栏标签条插入；S14 拖到右缘分屏；S15 拖回原栏重排；
- S16 Esc 取消拖拽无变化；S17 会话重启恢复两栏结构与激活项；S18 截图 `docs/screenshots/phase-p35-split.png`。

- [ ] **Step 1：套件定稿**（并入 `verify-all.mjs` 步骤表，49 步；计数口径更新：Rust 断言数、套件数）
- [ ] **Step 2：单跑** `node scripts/smoke-split.mjs` 全绿；`smoke-session`、`smoke-tabs`、`smoke-windows`、`smoke-cli`、`smoke-settings`、`smoke-buttons`、`smoke-settings-io` 全绿
- [ ] **Step 3：文档**（README「分屏」节、CHANGELOG P3-5、progress 阶段小节、coverage-matrix 行与计数、本计划勾选）
- [ ] **Step 4：全量自检** `node scripts/verify-all.mjs --exclude smoke-uninstall` → 48/48（49 步中 1 排除）；`latest.md` 提交
- [ ] **Step 5：提交** `test(ai): smoke-split E2E 并入 verify-all（P3-5 任务9）与文档收口`

---

## 自检清单（写计划后自查）

- [x] 规格覆盖：D52「≤4 栏」→ 任务 5/7；布局持久化 → 任务 8；每栏独立标签组 → 任务 4/5/6；跨窗拖拽沿用 → 任务 3/7；会话扩展（P3-7 共享基础）→ 任务 2/8。
- [x] 无占位符：关键签名、命令、payload、测试代码均给出；界面组件给出 props 契约与步骤。
- [x] 类型一致：`PaneLayout/PaneSession/PaneSplitDir` 前后端同名同形；`resolvePaneDrop` 单一实现被本地/跨窗共用。
- [x] 风险对策：单栏零回归（任务 4 前置回归子集）；重开窗口的 `#1` 键确定性；跨窗落点失败仅「标签留在源窗」不丢数据。

## 已知接受项（记录，不在本阶段处理）

- `scrollMemory` 以 tabId 为键：同一标签同时展示于两栏时滚动位置互相覆盖（罕见，接受）。
- 多栏共享同一套排版 CSS 变量（阅读设置按窗口生效，接受）。
- `<svelte:window onkeydown={markUserInput}>` 每栏各挂一份：任一栏按键会把所有栏标记为「用户输入」（轻微，接受）。
