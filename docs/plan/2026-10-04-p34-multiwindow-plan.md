# P3-4 多窗口、分屏与跨窗口拖放 实现计划（2026-10-04）

> 依据：设计决策 D52（多窗口 + 分屏 ≤4 栏 + 跨窗口拖拽标签，每窗口独立标签组独立会话；标签颜色）。
> 维护者裁定（2026-10-04）：**完整原生拖放**（拖出分离 + 悬停并入 + 拖回重排）；**标签颜色一并实现**。
> 关联：P3-5 分屏、P3-7 会话扩展共用本计划的「按窗口状态」「会话 v2」基础。

## 0. 现状与关键事实（侦察结论）

- 状态单例：`src-tauri/src/main.rs:216` `.manage(Mutex::new(AppState::new()))`；全部 tab 命令取同一 `Mutex<AppState>`；`app_state.rs` 的 `tabs/order/next_tab_id/active_tab` 全进程唯一。
- 能力白名单：`src-tauri/capabilities/default.json` `"windows": ["main","settings"]` 精确匹配，新 label 默认无权限（含事件监听）。
- 窗口创建先例：设置窗口 `commands.rs:1768-1810`（async 直建 + `WebviewUrl::App`）；打印窗 `commands.rs:461-511`（async + `run_on_main_thread`）。
- 事件全广播：`app.emit`（`commands.rs:603` 等）无定向；前端 `srt://cli-open` 在 `App.svelte:1341-1345` 全窗口监听。
- 会话单窗口：`session/model.rs:76-87` 一组 `window/tabs`；前端逐窗口无条件写同一文件（`App.svelte:1425-1436`）→ 多窗口会互覆。
- 标签拖拽：`TabBar.svelte` 指针事件自实现（非 HTML5 DnD），窗口级监听、仅 clientX；Rust `reorder_tab` 仅同窗口排序。
- 分屏渲染技术：`ReaderView.svelte` `.spread` 绝对定位方案（可用作面板布局参考）。
- E2E：CDP `findTarget(port, urlIncludes)` 仅 URL 过滤；同 URL 多 main 需 URL 标记或注入窗口标识。

## 1. 架构决策（本计划锁定）

1. **按窗口状态采用「单 AppState + 窗口维度字段」**（而非 `HashMap<AppState>`）：
   - `Tab` 增加 `owner: String`（窗口 label，默认 `"main"`）；
   - `AppState.order: Vec<u64>` → `orders: HashMap<String, Vec<u64>>`；
   - `AppState.active_tab: Option<u64>` → `active: HashMap<String, Option<u64>>`；
   - 全局 `next_tab_id` 保留（跨窗口唯一，便于诊断）；
   - 45+ 个按 tabId 工作的命令无需改签名；仅 8-10 个需要窗口上下文的命令新增注入参数。
2. **窗口上下文注入**：命令参数用 `window: tauri::WebviewWindow`（Tauri v2 自动注入调用窗口），取 `window.label()`；实现前先加一条最小命令验证注入可用（任务 2 步骤 0）。
3. **新窗口 label**：`main-2`、`main-3`…（能力白名单改 `"main*"`）。
4. **会话 v2**：`SessionState { schemaVersion:2, windows: Vec<WindowSession> }`，`WindowSession { label, window: WindowState, active_tab_index, tabs: Vec<SessionTab> }`；v1 读取时迁移（单窗口 → 数组）。`save_session` 采用「按窗口 read-modify-write」避免多窗口互覆。
5. **跨窗口拖放协议**：Rust 持有拖拽会话（光标轮询 + 按钮态 + 窗口命中测试），前端只负责发起与渲染；原生拖影 = 新增 `drag-ghost` 无边框置顶点击穿透窗口（`drag-ghost.html` 入口）。
6. **标签颜色**：`Tab.color: Option<String>`（来自固定调色板 id：`red/orange/yellow/green/blue/purple`），前端用主题感知 CSS 变量渲染；随会话持久化。

## 2. 任务分解（每个任务可独立验证、独立提交）

### 任务 1：AppState 按窗口分区（纯 Rust）
- 文件：`src-tauri/src/app_state.rs`（改结构与方法）、`src-tauri/src/commands.rs`（List 相关）。
- 接口变化（关键签名）：
  - `Tab.owner: String`；
  - `pub fn open(&mut self, owner: &str, path: &Path, ...) -> Result<(u64, bool)>`（返回是否复用；去重仅同 owner）；
  - `pub fn view(&self, owner: &str) -> TabsView`（过滤 owner）；
  - `pub fn set_active(&mut self, owner: &str, tab: u64)`；
  - `pub fn reorder(&mut self, owner: &str, tab: u64, index: usize)`；
  - `pub fn close(&mut self, owner: &str, tab: u64)`（只允许关自己窗口的标签）；
  - `pub fn close_window_tabs(&mut self, owner: &str) -> Vec<u64>`；
  - `pub fn move_tab(&mut self, tab: u64, target_owner: &str, index: usize)`（跨窗口迁移：改 owner、两端 order、目标 active）。
- 测试（app_state 单测同步改造 + 新增）：
  - 双 owner 各自列表互不可见；同 owner 去重、跨 owner 可各开同一文件；
  - 关闭自己窗口的标签后 active 回落只影响本 owner；
  - move_tab：源移除/目标插入/active 指向目标处新标签/去重语义（目标已开同文件则合并为移动+激活）；
  - close_window_tabs 返回并移除该 owner 全部标签。
- 提交：`feat(ai): AppState 按窗口分区（P3-4a）`。

### 任务 2：命令层窗口上下文
- 文件：`src-tauri/src/commands.rs`、`src-tauri/src/main.rs`（如需）。
- 步骤 0：加临时命令 `whoami_window(window: tauri::WebviewWindow) -> String`（或直接改 `list_tabs` 后跑 E2E 验证），确认注入的是调用窗口 label；验证后删除临时命令。
- 变更命令（新增 `window: tauri::WebviewWindow` 参数，内部取 label 传入 AppState）：
  `list_tabs`、`open_file`、`new_file`、`set_active_tab`、`reorder_tab`、`close_tab`、`take_cli_files`（见任务 6）、`move_tab_to_window`（任务 8）。
- 其余按 tabId 的命令保持签名（tabId 全局唯一，跨窗口误用自然 `TAB_NOT_FOUND`）。
- 测试：Rust 侧为 AppState 已覆盖；E2E 用 smoke-tabs/smoke.mjs 回归 + smoke-windows 任务 10 验证。
- 提交：`feat(ai): 命令层窗口上下文（P3-4a）`。

### 任务 3：new_window 与能力/快捷键/菜单
- 文件：`main.rs`（窗口创建封装 `create_main_window(app, label, geo)`；把现有 main setup 逻辑抽成函数）、`commands.rs`（`new_window` 命令）、`capabilities/default.json`（`"main*"`）、`src/lib/ipc.ts`、`src/App.svelte`（菜单接线）、`src/lib/components/MenuBar.svelte`（文件菜单「新建窗口」）、`src/lib/shortcuts/defaults.ts`（或 Rust defaults：`newWindow: Ctrl+Shift+N`）、`scripts/check-shortcut-parity.mjs`（动作表）、i18n 两语言。
- 行为：`new_window(initialPaths?: string[], geometry?) -> String`（返回 label）；新窗口加载 `index.html`，显示前恢复其会话几何（任务 5 前先级联偏移默认）。
- 验收：菜单/快捷键开窗；窗口独立标签组（任务 1/2 生效）；快捷键对齐脚本通过。
- 提交：`feat(ai): 新建窗口（P3-4b）`。

### 任务 4：窗口关闭语义
- 文件：`src/App.svelte`（onCloseRequested 按窗口处理）、`commands.rs`（`close_window_tabs`）、`main.rs`（事件监听可选）。
- 行为：
  - 关闭某窗口前：仅对该窗口未保存标签走三态确认（现有 `UnsavedDialog` 逻辑参数化到窗口标签集）；
  - 确认后 `close_window_tabs(ownLabel)`；关闭该窗口；**最后一个 main 窗口关闭时**才 `mark_clean_exit` + 退出（其余情况不写 clean-exit 标记）；
  - 进程内窗口计数由 Rust 注册表提供（`window_count()`），前端退出判断改用它。
- 验收：开两窗各开标签 → 关一个：应用存活、另一窗口标签仍在、数据目录 clean-exit 未写；关最后一个：退出且恢复链路正常；含脏标签的窗口关闭弹确认且取消可留。
- 提交：`feat(ai): 多窗口关闭语义（P3-4b）`。

### 任务 5：会话 v2（每窗口）
- 文件：`session/model.rs`（v2 结构 + v1 迁移）、`session/store.rs`（load 兼容）、`commands.rs`（`get_session`/`save_session` 带 `window_label`）、`main.rs`（启动恢复 main + 按需创建记忆窗口：本期仅恢复窗口几何，多窗口标签组恢复在 P3-7 完整版；本期先保证互不覆盖）、`src/lib/session.ts`、`src/App.svelte`。
- 行为：`save_session({ windowLabel, window, activeTabIndex, tabs })` → Rust read-modify-write 对应 `windows[label]`；`get_session(windowLabel)` 返回该窗口节。
- 测试：Rust 单测（v1→v2 迁移、按窗口合并、损坏自愈）；smoke-session 回归 + smoke-windows 断言两窗口各自几何持久化。
- 提交：`feat(ai): 会话 v2 按窗口持久化（P3-4c）`。

### 任务 6：事件定向与命令行开窗
- 文件：`commands.rs`/`cli.rs`（pending 队列按目标窗口）、`main.rs`（`WindowEvent::Focused` 记录最后聚焦 main label；`emit_to(label, CLI_OPEN_EVENT, ...)`）、`App.svelte`（监听仅处理自己窗口的 payload）。
- 行为：二次启动转发 → 最后聚焦窗口接收并开标签；若无窗口（冷启动）→ 排空到首个 main；`srt://settings-changed` 保持广播。
- 验收：smoke-cli 回归（单窗口语义不变）+ smoke-windows：两窗时转发进最后聚焦窗。
- 提交：`feat(ai): 事件定向（P3-4c）`。

### 任务 7：标签颜色
- 文件：`app_state.rs`（`Tab.color`、`set_tab_color`）、`commands.rs`、`ipc.ts`、`TabBar.svelte`（渲染色条）、`TabContextMenu.svelte`（颜色子菜单：6 色 + 清除）、`session.rs`（持久化 color）、i18n。
- 测试：Rust（设置/清除/非法色拒绝）；smoke-windows：右键设色 → 色条出现 → 重启（会话 v2）后保留；对比度用现有主题变量（暗/亮均可见）。
- 提交：`feat(ai): 标签颜色（P3-4d）`。

### 任务 8：跨窗口移动（菜单通道，先落地）
- 文件：`commands.rs`（`move_tab_to_window(tab_id, target_label, index)`、`list_windows()` 供菜单）、`TabContextMenu.svelte`（「移动到窗口 →」子菜单，列出其他 main 窗）、i18n。
- 测试：smoke-windows：两窗移动标签（源少一、目标多一且激活）、移动到已开同文件窗口（合并激活）。
- 提交：`feat(ai): 标签跨窗口移动命令（P3-4e）`。

### 任务 9：完整原生拖放
- Rust（新文件 `src-tauri/src/drag_session.rs`）：
  - `begin_tab_drag(tab_id, source_label, cursor: (f64,f64))`；
  - 轮询线程（30–50ms）：`GetCursorPos` + `GetAsyncKeyState(VK_LBUTTON)`；命中测试窗口矩形（注册表维护：`WindowEvent::Moved/Resized` 更新）；
  - 事件：`srt://tab-drag-tick { cursor, overLabel, overIndex }` 定向发源窗/目标窗；
  - 松手：over 其他 main → `move_tab` 到落点 index；over 桌面/无窗 → `new_window` 于落点 + `move_tab`；over 源窗 → 发 `srt://tab-drag-release { index }` 交源窗自处理排序；
  - `drag-ghost` 窗口：Rust 创建，`set_ignore_cursor_events(true)`、置顶、无边框；tick 时 `set_position`；HTML 入口渲染标签片。
- 前端：
  - `TabBar.svelte`：拖拽激活阈值后调 `beginTabDrag`；收到 release 才做本窗排序；tick 里更新本地拖拽指示；不渲染跟随光标（ghost 在原生窗）；
  - 目标窗：收 `tab-drag-hover` 显示插入指示线；释放由 Rust 直接迁移。
- E2E（smoke-windows）：协议级驱动（begin → 模拟 tick/over → release）断言迁移与新建窗口；截图为人工核验证据。
- 提交：`feat(ai): 跨窗口拖拽标签（P3-4e）`。

### 任务 10：smoke-windows E2E + verify-all + 文档
- 新套件 `scripts/smoke-windows.mjs`（≥12 项）：
  W1 菜单新建窗口（label main-2 出现 /json）；W2 快捷键新建；W3 两窗独立标签组（各自开文件互不见）；W4 跨窗口移动（菜单）；W5 色标设置/持久化；W6 关闭一窗应用存活（/json 目标计数）；W7 最后窗口关闭退出与 clean-exit；W8 会话 v2 两窗几何各自恢复；W9 拖拽协议迁移；W10 拖出成新窗；W11 设置窗口与能力（新窗可关闭/全屏命令）；W12 截图。
- 并入 `verify-all.mjs`（48 步）；coverage-matrix（套件行/命令行/计数）；README（多窗口用法）、CHANGELOG、progress；`check-shortcut-parity` 通过。
- 提交：`test(ai): smoke-windows E2E 并入自检` + `docs(ai): P3-4 文档收口`。

## 3. 风险与对策
- **WebviewWindow 注入不可用**：任务 2 步骤 0 先验证；不可用则退化为前端显式传 `label`（`getCurrentWindow().label`），其余设计不变。
- **跨窗口拖放的 DPI/坐标**：统一物理像素（`outer_position`、`GetCursorPos`）；目标窗落点 index 用「光标 x − 窗口内容原点」与标签条命中计算；高 DPI 在 125%/150% 真实机验一次。
- **IPC 拥塞先例**（P3-3 已修）：拖拽轮询不引入新的高频 IPC；ghost 位置移动由 Rust 原生 `set_position`，不经前端。
- **回归面大**：任务 1/2 落地后先跑 smoke-tabs/smoke-session/smoke-cli 全量回归，再继续后续任务。
