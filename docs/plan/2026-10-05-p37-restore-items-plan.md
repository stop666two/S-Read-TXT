# P3-7 会话恢复扩展（restoreItems）实现计划（2026-10-05）

> 依据：设计决策 D76；settings-spec D-19（`session.restoreItems`）；用户裁定（2026-10-05）：全部可选项（文件=总开关/光标/滚动/折叠/窗口布局）、光标全部标签都记、越界裁剪到最后一行并 toast、折叠=行号+长度校验、主题移出（设置已持久化）、总开关关时仅恢复窗口数量与几何。
> 前置：P3-6 完成；会话 v3 / 设置 schema v15 现状。

## 目标

1. 设置新增「会话恢复内容」四项开关（注册表 `app.startup.restoreItems.*`）：光标 / 滚动 / 折叠 / 窗口布局。
2. 会话 v4：`SessionTab` 增 `caretRow/caretCol/folds`；折叠以「起始行+长度」保存，恢复时与重算区间逐项校验，不匹配丢弃。
3. 语义：总开关 `startup.restoreSession` 关时仍恢复窗口数量与几何（不恢复标签内容）；窗口布局关时各窗口折叠为单栏（其余栏标签并入首栏）；滚动关时不 seed 滚动记忆（打开在顶部）；光标关时不 seed 光标记忆；折叠关时不恢复折叠。越界：行号≥总行数→裁剪到最后一行并聚合 toast。
4. 新 E2E 套件 smoke-restore 并入 verify-all（52 步）。

## Global Constraints

- 源码/脚本全部纯中文注释风格；不改既有行为（除本计划明确语义）。
- 原子写、失败静默回退、focused pane 语义不变。
- Rust 测试跟随：settings 注册表/迁移/默认值、会话 v4 往返与 v3 兼容。
- 折叠校验：`startRow` 存在于当前计算区间且 `endRow-startRow` 与存档长度一致，否则丢弃该条。
- 光标坐标：存档 0 基（`caretMemory` 的 `CaretPos` 原样）；恢复时经 `rowsTotal` 裁剪。
- i18n 中英双语齐全；文档（configuration.md/README/CHANGELOG/progress/coverage-matrix）同步。

---

### Task 1: 设置 schema v16 与四项开关

**Files:** `src-tauri/src/settings/model.rs`、`defaults.rs`、`migrate.rs`、`registry.rs`、`src/lib/ipc.ts`、`src/lib/i18n/zh-CN.ts`、`src/lib/i18n/en.ts`、`docs/configuration.md`

- `RestoreItems { caret: bool, scroll: bool, folds: bool, layout: bool }`（serde camelCase + default）挂在 `StartupSettings.restore_items`；默认 `caret=false, scroll=true, folds=false, layout=true`（当前行为基线，新项 opt-in）。
- `SCHEMA_VERSION 15→16`，`defaults.rs` 版本史注释补 v16；`migrate.rs` STEPS 追加 `(15, v15_to_v16)` 空步；`apply_chain` 连续性测试会强制。
- `registry.rs` SPECS 追加 4 条：`app.startup.restoreItems.caret|scroll|folds|layout`（`SettingKind::Bool`，group `app.startup`），labelKey/descKey 新键。
- i18n：`setting.app.startup.restoreItems.*`（组标题 + 4 项名称与描述）；设置窗常规页本就渲染 `app.startup` 组，自动出现。
- ipc.ts `StartupSettings` 增 `restoreItems` 字段与类型。
- 测试：defaults 单测（四项默认值）、migrate 链（v15→v16 空步）、bundle/registry 一致性（未知字段拒绝路径已有，SPECS 完整即可）；svelte-check。
- 验证：`cargo test settings`、`node scripts/check-encoding.mjs`；提交。

### Task 2: 会话 v4 模型

**Files:** `src-tauri/src/session/model.rs`、`store.rs`、`src/lib/ipc.ts`、`docs/configuration.md`

- `SESSION_SCHEMA_VERSION 3→4`；`SessionTab` 增 `caret_row: Option<u64>`、`caret_col: Option<u64>`、`folds: Vec<FoldSpan>`（`FoldSpan{start_row:u64,len:u64}` serde camelCase，默认空）。
- `normalize` 无分支改动（结构探测已有）；守卫：`folds` 截断到 ≤512 条去重排序（防御脏数据）。
- 测试更新：`session_roundtrip_v3` 断言改 4 并补 caret/folds 往返；迁移测试断言 `schema_version == 4`；新增「v3 旧文件（无新字段）载入为 4 且字段为空」。
- configuration.md §2.4 表补字段；顺手修正文档中 schemaVersion 旧值（2/3→4）。
- 验证：`cargo test session`；提交。

### Task 3: 折叠记忆与光标记忆接入

**Files:** `src/lib/reader/fold-memory.ts`（新）、`src/lib/reader/scroll-memory.ts`（可不动）、`src/lib/edit/caret-memory.ts`、`src/lib/components/ReaderView.svelte`、`src/lib/session.ts`、`src/App.svelte`

- `fold-memory.ts`：`Map<tabId, FoldSpan[]>` + get/set/snapshot/seed/remove（仿 scroll-memory）。
- ReaderView：`foldRegions` 加载完成后：读 `foldMemory.get(tabId)`，与当前区间校验（start 存在且长度一致）→ `foldedRows = 新集合` → `applyFoldChange()`（顶层行稳定重排）；写回：`toggleFold`/`foldCommand` 效果/标签切换清理前，将「当前 foldedRows ∩ foldRegions」写成 FoldSpan[] 存 foldMemory（含清空时写空数组）。
- `caret-memory.ts`：补 `snapshot()` 与 `seed(tabId,pos)`（seed 仅 row>0 时写，防覆盖）。
- `session.ts` collect：`caretRow/caretCol`（`caretMemory` 无条目则 null）、`folds`（foldMemory 无则空数组）；容器不变。
- `App restorePaneTabs`：读设置 `restoreItems`：
  - caret 开且有记录：`row=min(caretRow, rowsTotal-1)`；裁剪发生时记入 `clipped[]`；`caretMemory.seed(info.tabId,{row,utf16:col})`。
  - folds 开：`foldMemory.seed(tabId, spans)`。
  - scroll 开：现有 `scrollMemory.seed` 保留；关：跳过。
  - 恢复完成后 `clipped.length>0` → 一次 `toasts.show(t('session.positionClipped',{count}))`。
  - layout 关：忽略会话 layout，各窗口用 `leaf(首栏)`；其余栏标签按顺序并入首栏（无损失）。
- EditLayer 初始光标读取已存在（`caretMemory.get(tabId)`），seed 时机在 `tabs.select` 之前即可生效（挂载初值）。
- 验证：vitest（fold-memory 新增小测若抽出纯函数）、svelte-check、手工探针（改行内容制造折叠长度不匹配→丢弃不崩）；提交。

### Task 4: 总开关语义（窗口数量与几何仍恢复）

**Files:** `src-tauri/src/main.rs`、`src/App.svelte`、`src/windows/settings` 文案

- main.rs：其余窗口重建门控从「restore_session」改为「restore_session || restore_items.layout」（内容恢复门控仅在前端），几何仍受 `restore_window`；主窗几何不变。
- App：`restoreSession` 早退条件不变（总开关关→不恢复标签）；但若 layout 开，仍按会话应用布局（空栏）。`restoreItems.layout` 关→单栏。
- 文案更新：`setting.app.startup.restoreSession.desc` 补「关闭时仅恢复窗口数量与几何」；i18n×2。
- 验证：smoke-session 回归（必要时更新其预期并在套件注释说明新语义）；提交。

### Task 5: smoke-restore 套件

**Files:** `scripts/smoke-restore.mjs`（新）

用例（U1-U10，全部真断言；每步说明预期）：
- U1 设置窗常规页出现 4 项「会话恢复内容」开关（data-setting 断言）。
- U2 折叠：开折叠模式（display.folding=indent? 用设置或 `__srt` 保存设置）→ 折叠 2 处 → 重启 → 折叠数一致且隐藏行数一致。
- U3 折叠失效：重启前改写文件使某折叠区间长度变化 → 重启后该条被丢弃、另一条保留、无崩溃。
- U4 光标：编辑标签放置光标到已知行/列 → 重启 → `caretInfo` 行列一致（statusbar 行:列）。
- U5 越界裁剪：记录位置后把文件截短 → 重启 → 位置=末行 + 出现裁剪 toast。
- U6 滚动关：关闭 scroll 开关 → 重启 → 顶部行=0；开回后恢复。
- U7 布局关：两栏布局+标签 → 关闭 layout → 重启 → 单栏且标签全在。
- U8 总开关关：多窗口+标签 → 关闭 restoreSession → 重启 → 窗口数量与几何保持、无任何标签。
- U9 窗口布局恢复（layout 开）回归：两栏+激活项 + 分隔比例（复用 probe-session3 断言，简化为 2 项）。
- U10 截图 `docs/screenshots/phase-p37-restore.png`。
- 脚手架复用 smoke-utility（菜单/设置 IPC/重启管理）；设置开关经设置窗口 UI 点按（真实路径）+ `__srt` 校验。
- 验证：单跑 10/10；提交。

### Task 6: 并入 verify-all 与文档收口

**Files:** `scripts/verify-all.mjs`、`README.md`、`CHANGELOG.md`、`docs/plan/progress.md`、`docs/verify/coverage-matrix.md`

- verify-all 追加 smoke-restore（52 步）；计数同步（Rust / vitest / E2E 43 套 / 52 步）。
- 全量自检（--exclude smoke-uninstall）→ 52/52 绿色后提交证据（latest.md+截图）。
- progress P3-7 节 + 下一切片 P4。

---

## 自检清单（计划完成后逐项核对）

- [ ] 设置 schema v16 迁移链完整（无空档报错）
- [ ] 会话 v4 兼容 v3 文件（旧字段缺省读取）
- [ ] 折叠注入遵循行号+长度校验
- [ ] 光标对编辑标签生效且只读不报错
- [ ] 越界裁剪 + 聚合 toast
- [ ] layout/scroll/caret/folds 全开关双态断言
- [ ] 总开关关：窗口数量几何保留、无标签
- [ ] smoke-restore 10/10 且并入 verify-all
- [ ] 文档与计数同步
