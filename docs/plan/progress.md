# S-Read-TXT 项目进度台账

- 作用：记录阶段与切片的执行状态、验证证据、提交记录；是实施计划（`docs/plan/implementation-plan.md`）的实时执行账本。
- 维护规则（文档同步义务）：
  - **每完成一个切片**：立即更新本文件对应状态与证据；
  - **每阶段边界**：更新 CHANGELOG.md（Keep a Changelog）；接口/配置变更同步 README 与 `docs/configuration.md`；
  - **每个提交**：Conventional Commits（`feat(ai):` / `fix(ai):` / `chore(ai):` / `docs(ai):`）。
- 构建环境提示：所有 cargo/tauri 命令前需前置 `D:\msys64\ucrt64\bin` 到 PATH（Tesseract 旧版 DLL 遮蔽问题，详见 README「Windows 构建环境注意」）。

---

## 阶段 0：工程初始化 ✅

- 完成日期：2026-10-02；提交：`d67cf77`（85 文件）
- 验证证据：
  - `npm run check`：0 错误 0 警告；`npm run build`：JS 35.5KB（gzip 13.5KB）+ CSS 3.6KB
  - `cargo build` + `tauri build --debug --no-bundle` 成功 → `target/debug/s-read-txt.exe`
  - CDP 冒烟：标题 `S-Read-TXT`、正文含 `v0.0.1-beta` 与便携数据目录路径（IPC 端到端 OK）
  - 截图：`docs/screenshots/phase0-smoke.png`
  - 防护钩子：提交 AGENTS.md 被拦截（exit 1）；`SRT_DATA_DIR` 环境变量文档化
- 关键发现：windres 故障根因（PATH 中 Tesseract-OCR 的旧版 `libgcc_s_seh-1.dll` 遮蔽 MSYS2，cc1 报 `0xC0000139`）；Vite 8 需用 Oxc 默认压缩；identifier 定为 `com.sreadtxt.desktop`。

## 阶段 1：Rust 核心（已完成 ✅ 2026-10-02）

| 切片 | 内容 | 状态 | 证据 |
|---|---|---|---|
| 1.1 | storage：便携目录解析 + 可写性探测 | ✅ 已提交 `334e8ad` | `cargo test` 8/8；`data_dir_status` 命令接线 |
| 1.2 | atomic + json_io：原子写 / JSON 容错读写 | ✅ 代码完成 | 单测 10 项；累计 18/18（原子替换/无残留/损坏检测/无 BOM/pretty） |
| 1.3 | settings/reader/shortcuts：配置模型 + 原子读写 | ✅ 代码完成 | 单测 10 项；累计 28/28；损坏自愈备份、未知值归一、范围裁剪、快捷键覆盖合并；**修复 `maxFileSizeMB` 字段名**（serde 自动转换生成 `maxFileSizeMb` 与文档不符，字段级 rename 对齐） |
| 1.4 | logging：JSON 行日志 + 轮转 + 级别开关 + 链路上下文 | ✅ 已提交 `36e6735` | 单测 12 项；轮转/上下文恢复/级别解析；`log` crate 启用 alloc feature（set_boxed_logger） |
| 1.4b | save_settings 接线 + IPC 请求上下文 + 告警清零 + 运行冒烟 | ✅ 代码完成 | 42/42；`cargo build` **0 警告**（原 11 条未接线告警清零）；运行冒烟：`target/debug/data/logs/app.log` 落盘启动行（JSON/UTC/中文正常） |
| 1.5 | history：JSONL 追加/去重/剪枝/自愈压缩 | ✅ 代码完成 | 单测 7 项；累计 53/53；`get_history`/`remove_history`/`clear_history` 接线；已知 2 条 dead_code 告警（`append`/`now_rfc3339` 待阶段 2 open_file 接线） |
| 1.6 | session：窗口/标签锚点会话读写 + 自愈载入下沉共享 | ✅ 代码完成 | 单测 4 项；累计 60/60；窗口尺寸越界回退、空路径剔除、活动下标收敛；`get_session`/`save_session` 接线；`storage::json_io::load_json_or_default` 共用（settings 同步改用） |
| 1.7a | textfile：编码检测/解码 + mmap 只读映射 | ✅ 代码完成 | 单测 12；BOM/采样检测（WHATWG 名映射——对象比较实测未命中 GB18030）、`encode()` Web 表单语义坑规避、空文件映射包装 |
| 1.7b | textfile：稀疏行索引 + 文本窗口 + FileSession | ✅ 代码完成 | 单测 20；累计 92/92；512 行检查点、8KB 字符边界分块（UTF-8/UTF-16/传统多字节）、换行族含 UTF-16 双字节扫描、百分比互逆；设计文档 §4.1 已同步 |
| 1.8 | lib+bin 拆分 + 集成测试 + 阶段收尾 | ✅ 完成 | **97/97**（lib 92 + 集成 5）、`cargo build` 0 告警、CLI 产物运行冒烟通过（标题/正文/IPC） |

### 切片 1.1 详情（storage）

- 文件：`src-tauri/src/storage/{mod.rs, paths.rs, data_dir.rs}`；`main.rs` 接线新增 `data_dir_status` 命令、`get_app_info` 改用便携解析。
- 行为：`SRT_DATA_DIR`（非空白）覆盖 → 否则 `exe目录/data`；可写性 = 真实写探针（`.srt-write-probe`，探测后删除）。
- 测试：4（paths）+ 4（data_dir）= 8/8；覆盖：覆盖优先/空白回退/默认便携/无父目录退化/可写/父级为文件失败/幂等/无残留。

### 跨阶段已知事项（持续更新）

- **构建/运行命令配方（重要）**：
  - 所有 cargo/tauri 命令前置 `D:\msys64\ucrt64\bin` 到 PATH（Tesseract 旧版 DLL 遮蔽，见 README）；
  - `cargo test` 需再把 `src-tauri\target\debug` 加入 PATH（WebView2Loader.dll 由构建脚本放在该目录，GNU 测试目标从 deps 目录运行找不到它）；
  - **运行冒烟必须使用 `npm run tauri build -- --debug --no-bundle` 产物，且冒烟前重新构建一次**：`cargo build`/`cargo test` 的 debug 产物可能按 Tauri dev 语义指向 `http://localhost:1420`（不内嵌前端 → 页面无 `window.__srt`）；smoke 脚本已对「前端未就绪」给出明确诊断。
  - **严禁按进程名结束 `msedgewebview2`（安全红线，抹录在案）**：WebView2 是系统共享运行时，其他应用也在使用；清理残留只能按本应用 PID 结束整树（`taskkill /PID <pid> /T /F`，其 WebView2 子进程随树回收）。此前排查时曾误杀全系统 WebView2 进程一次，已作为反面教训记录。
- **GNU 工具链测试目标清单问题（架构性规避）**：链接 GUI 依赖的测试目标缺 Common-Controls v6 清单 → 加载旧 comctl32 → `TaskDialogIndirect` 入口点缺失（`0xC0000139`）；lib+bin 拆分后库测试不链接 GUI 即规避；应用二进制由 tauri-build 注入清单，不受影响。
- **阶段 9 打包清单（预登记）**：确认 NSIS 是否携带 `WebView2Loader.dll`（构建脚本已置于 profile 目录；如未携带需加入 `bundle.resources`）；同机安装/便携运行核验。
- 阶段 1 完成：2026-10-02（97/97 测试；0 告警；运行冒烟通过）。

## 阶段 2：阅读界面（已完成 ✅ 2026-10-02）

| 切片 | 内容 | 状态 | 证据 |
|---|---|---|---|
| 2a | 后端阅读状态：AppState（标签表/去重/上限）+ open_file/get_rows/set_encoding/list_encodings + IpcError + 历史接线 | ✅ 代码完成 | cargo test **106/106**（lib 101 + 集成 5）；0 告警；`scripts/smoke.mjs` 运行冒烟 **9/9** |
| 2b | 前端布局骨架（菜单/工具栏/标签栏/阅读区/状态栏、四主题）+ 界面稿评审 | 🔎 骨架完成，待用户评审 | 截图 `docs/screenshots/phase2b-{light,dark,eye,light-menu}.png`；svelte-check 0/0；三主题计算色实测（light #FAF9F7 / dark #1E1E1E / eye #F5EFE0） |
| 2c | 前端阅读接线：①打开/拖拽/标签栏/空态/Toast/退出 ②虚拟滚动/编码下拉/百分比 | ✅ 完成 | 2c-1：单测 9/9、E2E **14/14**；2c-2：虚拟滚动引擎（行高模型/行文本 LRU/视口计算）+ 编码下拉 + 进度上报；单测 **30/30**、E2E **18/18**（2 万行文件仅渲染 53 行、滚动至中部 50%）；截图 `docs/screenshots/phase2c-reader.png` |

### 切片 2a 详情

- 新增 `src-tauri/src/app_state.rs`：标签表（BTreeMap + 单调 tabId）、重复打开去重（canonicalize 路径比较，命中则激活复用）、`maxTabs` 上限、`MAX_ROWS_PER_FETCH=2048` IPC 防御上限、编码切换、关闭后活动标签回落。
- 新增 `src-tauri/src/ipc_error.rs`：稳定错误码（FILE_NOT_FOUND / FILE_TOO_LARGE / MAX_TABS / TAB_NOT_FOUND / INVALID_ENCODING / IO / CONFIG_SAVE / HISTORY_SAVE / SESSION_SAVE / INTERNAL）；前端按 code 映射固定文案（FILE_TOO_LARGE 用需求逐字提示）。
- bin 拆分：`commands.rs`（13 个命令：原 9 个迁移至 IpcError + 新增 4 个）；`main.rs` 精简为「日志初始化 + 状态托管 + 命令注册」。
- `textfile/window.rs`：RowText 增加 Serialize（IPC 载荷嵌套用）。
- 接线：`open_file` 成功后写入历史（复用打开不重复写；历史失败仅告警，不影响打开）。
- 冒烟工具 `scripts/smoke.mjs`（零依赖，可复用）：启动 CLI 产物 → CDP 就绪轮询 → 9 项断言（含错误载荷形状）→ taskkill 清理。两个自我诊断修复：①CDP 目标出现可能早于页面脚本执行（新增 `waitForReady` 轮询）；②CDP 会把 Promise 拒绝转成异常（新增 `invokeCaught` 页面内捕获，保留 code/message 结构）。
- 编码标签实测：UTF-8 / GB18030 / UTF-16LE / UTF-16BE / Big5 / Shift_JIS / EUC-KR / windows-1252。

## 阶段 3：编辑引擎 A（片表后端）【最高风险】（已完成 ✅ 2026-10-02）

| 切片 | 内容 | 状态 | 证据 |
|---|---|---|---|
| 3a | 片表 + 双 Fenwick（字节/行单元）+ 行↔位置映射 + 跨片 CRLF 安全计数 + 编辑守卫 | ✅ 代码完成 | 新增单测 **20**；lib **121/121** + 集成 5；0 告警；与读模式 RowIndex 逐行一致性断言（UTF-8/GB18030/UTF-16/BOM/空文件/换行族）；>64KB 单行拒绝编辑 |
| 3b | apply_edits（插入/删除/替换）+ 撤销重做（50MB/1000 步）+ 行内 UTF-16→字节换算 | ✅ 代码完成 | 新增单测 **10**；插删替换/跨行合并拆分/批量单撤销步/GB18030↔UTF-16 换算/代理对吸附/跨片 CRLF/脏标记/步数上限/越界原子性 |
| 3c | 保存链：编码询问流 / 直拷 vs 转码 / .bak / 原子写 / 冲突检测 / 另存为 | ✅ 代码完成 | 新增单测 **8**；往返一致性/BOM 保留/GB→UTF-8 转码/不可表示原子失败/`.bak` 备份/冲突+强制/另存为/UTF-16 混合往返 |

### 切片 3a 详情

- 新增 `src-tauri/src/textfile/editing/{mod.rs, fenwick.rs, piece.rs, edit_doc.rs}`；`line_index` 的 `find_newline`/`read_utf16_unit` 提升为 `pub(crate)` 复用（换行语义单一来源）。
- 行模型：编辑态按**逻辑行**（换行族分隔），不做读模式的 8KB 显示分块；`EDIT_MAX_ROW_BYTES = 64KB` 守卫——打开时存在超长行则拒绝编辑（读取不受影响，错误携带行字节数）。
- 位置模型：内部 `DocPos(片段下标, 片段内偏移)`；对外为 `(行号, UTF-16 偏移)`（与前端 JS 字符串索引一致；字节换算在 3b 实现）。
- 跨片 CRLF：`count_units(bytes, enc, start, end, prev_cr)` 抑制前导 `\n`；片段元数据 `PieceMeta{units, ends_with_cr}`；`trailing_newline` 参与行数换算（`rows = units + 1 - 尾换行`）。
- 一致性策略：与只读路径（`RowIndex`/`window::fetch_rows`）同字节流逐行断言相等（含 `\r\n`/`\r`/`\n` 混合、连续空行、结尾换行、BOM、空文件）；三套编码（UTF-8/GB18030/UTF-16LE）均覆盖。

### 切片 3b/3c 详情

- **编辑应用**（3b）：`apply_edits(&[Insert|Delete|Replace])` 批次原子、单个撤销步；位置 = `(行号, UTF-16 偏移)`（与 JS 字符串索引一致；代理对中间吸附到字符起点）；解析为全局字节偏移后**降序应用**（后位编辑不影响前位坐标）；`apply_range` = 边界拆分 → 删除 → 插入（新增缓冲）→ 右邻上下文修正；`split_piece` 按**较小侧扫描**推导两侧计数（`left + right = parent`，大文件顶端编辑不整片重扫）；`coalesce` 合并同源连续片段。
- **撤销/重做**：交换式状态快照（pieces + metas + trailing + state_id），双上限 **50MB / 1000 步**（先到先裁剪，预算 = 新增 + 删除 + 快照开销）；脏标记 = 当前 state_id 与保存标记比对（分支编辑后不会误判干净）；新编辑清空重做栈。
- **保存链**（3c）：冲突检测（磁盘快照：mtime+size；`force` 强制覆盖）、`.bak`（`<文件名>.bak`，覆盖前 `fs::copy` 当前磁盘内容）、BOM 策略（UTF-16 目标恒写对应 BOM；UTF-8 仅原文件带 BOM 时保留；传统编码无 BOM）、同编码**原文直拷**/异编码流式转码（`Encoder::encode_from_utf8_without_replacement`，不可表示 → `Unrepresentable{ch}` 原子失败）、临时文件 + `sync_all` + `rename` 原子替换（**持 mmap 覆盖原文件实测通过**）。
- **本次修复的三个真实缺陷**：①`boundary_at` 拆分片段后前缀和未同步 → 同一批次第二次边界定位错位（删除范围少删/多删）；修复为每次变更即时 `rebuild_trees`（全局偏移语义不受拆分影响）。②跨片 `\r`+`\n`：单元终点恰在片段末尾时，行起点必须越过配对的另一半 `\n`（`unit_span` 扩展）。③encoding_rs 陷阱实锤：普通 `encode_from_utf8` 会把不可表示字符替换为数字字符引用（不会报错），必须用 `encode_from_utf8_without_replacement` 才能拿到 `EncoderResult::Unmappable` 致命错误。
- **已知行为（记录）**：Windows 上第三方工具若对已打开文件做 in-place 写入，会被我们的 mmap 拒绝（`ERROR_USER_MAPPED_FILE = 1224`）；编辑器惯例的「临时文件 + rename」不受影响（我们的保存即此路径）。阶段 9 文档收尾时纳入 README 已知限制。
- **测试**：lib **139/139** + 集成 5；阶段 3 新增 **38** 项；`cargo build` 0 告警。

## CI/CD：持续集成与发布流水线（已完成 ✅ 2026-10-02）

> 维护者约定：推送代码后自动完整构建 + 完整测试；Releases 只保留最新一个（旧 Release 全删，含测试版与正式版）；tag **永不删除**；**未经维护者明确允许，不执行任何 push**。

| 项 | 内容 | 状态 | 证据 |
|---|---|---|---|
| CI 工作流 | `.github/workflows/ci.yml`：仓库卫生（AGENTS.md 变体 / data/ / 大文件）、前端检查测试与构建、Rust 全量测试 + 完整构建、i686/aarch64 兼容检查；触发 = 任意分支 push / PR / 手动 | ✅ 完成 | 本地验证：prettier YAML 解析两文件均通过（退出码 0） |
| 发布工作流 | `.github/workflows/release.yml`：仅 tag（`v*`）触发；测试摘要 → 三架构 NSIS 构建（x64/x86/ARM64，MSVC）→ 详尽描述 → 创建 Release → **删除其他全部旧 Release**（删除 Release 不影响 tag）；tag 含 `-` 自动 prerelease；同 tag 幂等重建；`gh release create --verify-tag` 保证不创建新 tag | ✅ 完成 | 本地验证：描述生成全链路实测（4980 字符、11 个章节、无重复标题） |
| CI 脚本 | `scripts/ci/`：check-hygiene（正反例已测）/ collect-test-summary / env-info / sha256sums / release-notes（Node 零依赖；npm 版本检测含 Windows .cmd shell 回退） | ✅ 完成 | 卫生脚本：违规 exit 1、合规 exit 0；摘要：Rust 144 / vitest 30 / svelte-check 0-0；sha256：6 文件实测 |
| 文档 | README（真实仓库地址、CI 徽章、发布与 CI/CD 说明、MSVC 说明）；`docs/known-issues.md`（发布描述自动纳入） | ✅ 完成 | — |

发布描述板块（维护者已确认）：基础五件套 + 构建环境细节 + 变更明细 + 测试报告附件 + 已知问题与限制。

**首次推送前待办**：① 推送后检查 CI 首跑（32 位/ARM 兼容检查、MSVC 构建）；② 首个 tag（建议 `v0.0.1-beta`）发布演练；③ 若 ARM64 的 NSIS 打包在 CI 遇到问题，回退方案为该架构改传原始可执行文件（仅需调整 `release.yml` 的产物汇总步骤）。

## 阶段 4：编辑交互层（已完成 ✅ 2026-10-02）

| 切片 | 内容 | 状态 | 证据 |
|---|---|---|---|
| 4a | 编辑命令接线：`toggle_edit`（首次创建编辑文档 + 磁盘基准快照）/ `apply_edits` / `undo_edit` / `redo_edit` / `save_tab` / `save_tab_as` / `reload_tab`；`get_rows` 编辑态供数切换；`EditOp` 反序列化（kind + camelCase）；`EditDoc::percent_at_row`；脏态阻止编码切换（重载为无条件重建，确认由前端负责）；错误码映射（INVALID_POSITION / FILE_CONFLICT / ENCODING_UNREPRESENTABLE / NOT_EDITING / EDIT_DIRTY） | ✅ 完成 | lib **148/148**（新增 9：编辑文档生命周期/撤销重做/保存与冲突/另存为重定向/重载丢弃/脏态拦截/超长行拒绝 + 错误映射 2）；`cargo build` 0 告警 |
| 4b | 前端编辑 UI：切换入口与脏标记、光标/选区（点击/拖选/Shift+方向键/全选）、输入/删除/回车、IME 隐藏锚点、剪切复制粘贴、Ctrl+S 保存流（编码询问/冲突/.bak）、脏关闭三态确认 | ✅ 完成 | 见「切片 4b 详情」；vitest **47/47**、编辑冒烟 **12/12**、对抗冒烟 **40/40**、cargo **170/170**；截图 `docs/screenshots/phase4b-{edit,abuse}.png` |
| 4c | 查找/替换（大小写、下一个/替换/全部）、另存为 UI、重载 UI、E2E 实测（含 IME composition 模拟）、**超长行完整分段渲染（维护者确认）** | ✅ 完成 | 超长行分段（100MB 验收 9/9）；查找/替换 E2E 14/14；**IME 组合输入 E2E 7/7**（组合显示/候选更新/提交入库/取消无副作用/UTF-8 保存）；截图 phase4c-{longline,find,ime}.png |
| 5a | 快捷键引擎：15 动作默认方案、应用内全局捕获、Ctrl+1~9 固定键、编辑态让位、绑定热刷新 | ✅ 完成（`09cb953`） | E2E `smoke-shortcuts` **17/17**；修复 `select` 未同步后端活动标签的真实缺陷（含 close 东侧回落）；新增 `set_active_tab`/`get_default_shortcuts`；cargo 174 lib、vitest 70 |
| 5b | 设置窗口（按需创建）+ 快捷键自定义（录制/冲突/保留键/恢复默认/修改即存/主窗热刷新） | ✅ 完成 | E2E `smoke-settings` **17/17**；`smoke-buttons` **28/28**（C8 改开设置窗）；回归 edit 12/12、abuse 41/41、shortcuts 17/17；截图 `docs/screenshots/phase5-settings.png` |

> 扩展点预留（维护者要求）：`textfile::source::DocumentSource` 契约已落地——未来解析器/新格式实现该 trait 并在打开流程分派即可接入（AppState 取行与前端渲染零改动）；编辑契约仅绑定文本引擎，解析类文档默认只读。

### 切片 4b 详情

- 前端新增（`src/lib/`）：`edit/caret.ts`（CaretPos/Selection/UTF16_END、移动与选区工具）、`edit/ops.ts`（InsertOp/DeleteOp/ReplaceOp、退格/前删含代理对整对）、`edit/caret-memory.ts`（跨标签光标记忆）、`edit/focus.ts`（弹窗后归还键盘焦点）；组件 `EditLayer.svelte`（叠加层：光标/选区/preedit/隐藏输入框 textarea，`caretRangeFromPoint` 点击定位、IME 组合三段、剪贴板、快捷键路由）、`SaveDialog.svelte`（编码询问 + 备份勾选）、`ConfirmDialog.svelte`（冲突「覆盖保存/取消」）、`UnsavedDialog.svelte`（保存/不保存/取消）。
- 行高/行缓存：`invalidateFrom(row)`（编辑后从受影响行起失效——行号平移的最小正确范围）；`ReaderView` 编辑层挂载与 `data-row` 属性。
- 两个真实缺陷（探针脚本定位）：
  1. **编辑叠加层失步**：`nodes` Map 注册表在属性更新触发的 effect 重建时被 `clear()`，但 keyed `{#each}` 未重建 DOM → 注册表永久为空 → 光标/选区不渲染；修复 = 按 `data-row` 实时 DOM 查询（`rowNodeOf`），`measureRendered` 同步改 DOM 迭代。
  2. **全选哨兵越界**：`selectAll` 用 `Number.MAX_SAFE_INTEGER` 作为行内 UTF-16 偏移直发后端 → `Utf16OutOfRange` 静默失败；修复 = 前端取末行真实长度并 `resolveSelection()` 两端钳制；后端追加防御测试 `oversized_utf16_is_rejected_safely`。
- 窗口/启动修复（跨阶段问题排查结论）：① `windows_subsystem = "windows"` 无条件（debug 不再弹控制台窗口）；② 主窗 `visible:false` + 前端双 rAF 后 `show()/setFocus()` + Rust 8s 兜底（capabilities 增 `core:window:allow-show`/`allow-set-focus`，缺失时 show 被静默拒绝）；③ 窗口图标：Tauri v2 WindowConfig 不支持 icon 字段 → setup 中 `set_icon(include_bytes ../icons/128x128.png)`（Cargo 启用 `image-png`），任务栏图标修复；④ 背景色 `#FAF9F7` 防白闪；⑤ WebView2 用户数据目录重定向到便携 `data/webview`（`WEBVIEW2_USER_DATA_FOLDER`）。实测 TIME_TO_VISIBLE **1139ms 且显示即完整 UI**。
- 发布体积实测（红线核对）：release exe **3.95MB**、NSIS 安装包 **1.52MB**（目标 <10MB）；debug exe 244.6MB 仅为开发产物。
- 安全规则（新增）：严禁按进程名结束 `msedgewebview2`（系统共享运行时）；清理残留仅按本应用 PID 整树回收。
- 对抗冒烟 `scripts/smoke-abuse.mjs`（40 项：空文件/换行族/BOM/连打/撤销重做狂按/全选替换/首行边界/回车狂按/emoji+RTL+零宽/大粘贴/超长行拒绝/三态关闭/冲突流）；编辑冒烟 `smoke-edit.mjs` 重构复用 `scripts/lib/smoke-cdp.mjs`（统一 CDP 客户端与就绪护栏）。

### 切片 4c-1 详情（超长行显示分段，后端）

- 设计：编辑视图对 >8KB 逻辑行按 [`DISPLAY_SEGMENT_BYTES`]（与只读 `MAX_ROW_BYTES` 共用常量）生成显示段；`LongRowSegments{starts, utf16_bases}` 段表按需构建（普通行不登记）。
- 映射：`seg_of_row_utf16`（逻辑→段号+段内偏移，二分）、`seg_to_row_utf16`（段→逻辑行+段首 UTF-16 基准）；`fetch_display_rows` 按段解码；`percent_at_seg` 段首字节百分比。
- 编辑维护：`update_long_rows`（旧受影响键删除 + 后续键位移 + 新区间复查），撤销/重做将段表并入快照并计入预算；`EditApplied.touched_row` 与 `rows_total` 均改为显示行语义。
- 契约：`DocumentSource.rows_total/fetch_rows/percent_at_row` 统一为显示行语义（`FileSession` 天然一致）；`RowText` 增加 `logicalRow`/`baseUtf16`（`skip_serializing_if`，只读视图省略）；移除 `EDIT_MAX_ROW_BYTES` 守卫、`UnsupportedLongLine` 错误与 `EDIT_LINE_TOO_LONG` 错误码。
- 顺带修复真实缺陷：只读 `snap_row_boundary` UTF-8 分支按 `bytes[position-1]` 判定续字节（应为 `bytes[position]`），导致多字节长行在 8KB 边界被切进字符中间（两侧各出替换符）；已修复并加回归测试。
- 验证：cargo **173/173**（新增：多字节分段与只读逐段一致且拼接无损、长行内编辑/撤销、长行拆分与恢复、只读多字节分块回归）；fmt 通过；0 告警。

### 切片 4c-1 详情（续：前端适配与 100MB 验收）

- 前端：`ipc.ts` RowText +`logicalRow`/`baseUtf16`、EditApplied +`caretRow`/`caretUtf16`；`row-cache` 值改 `CachedRow{text,logicalRow,baseUtf16}`；新增 `edit/longline.ts`（`toLogical`、跨段退格/前删计划 `planBackspace`/`planDeleteForward`、代理对工具）；`EditLayer` 全部操作改经逻辑坐标换算、落点用后端权威 caret；同逻辑行相邻段复制直接拼接；`ReaderView` 传递分段元数据。后端补：`EditApplied` 携带权威光标落点（`advance_caret`），新增测试。
- **发现并修复 O(n²) 卡死（关键）**：`scan_row` 对无换行超长行每切一个 8KB 块都把 `find_newline` 扫到文件尾才能确认“无换行”——100MB 单行 ≈ 12800 块×平均 50MB ≈ 640GB 扫描量（debug 下界面冻结数分钟，即维护者报告的“卡死”）。修复：有界窗口 `[start, start+8KB+2)` 查找（CRLF 跨窗保持原语义）；新增 16MB 单行线性构建回归测试 + 窗口边界测试。
- E2E 工具加固：`smoke-longline.mjs` 增加**看门狗**（超时自动退出并打印最后步骤，杜绝无限挂起）、`.bak` 清理、随机端口、独立数据目录、仅按本应用 PID 整树回收。
- 验收证据（2026-10-02）：cargo **177/177**（157 lib + 15 对抗 + 5 集成）；vitest **56/56**；svelte-check 0/0；smoke-edit **12/12**；smoke-abuse **41/41**（L 段更新为“超长行可编辑”新语义）；**smoke-longline 100MB 单行 9/9**（rows=12800 精确、中部渲染、文档末编辑、保存 +1 字节尾 'aX'）；截图 `docs/screenshots/phase4c-longline.png`。

### 切片 4c-3 详情（查找/替换 UI 与菜单补全）

- 查找条 `FindBar.svelte`（fixed 锚定阅读容器右上；聚焦信号重聚焦；输入框 Enter=下一个/替换；Esc=关闭；`--base/--surface/--line/--accent` 令牌）；`EditLayer` 接入 Ctrl+F/Ctrl+H、命中选中（显示坐标）、替换（优先当前选区起点）、全部替换（单撤销步）、后端权威落点；`ipc.ts` 新增 findInEdit/replaceInEdit/replaceAllInEdit；`FindHit` 改为显示坐标（后端命中构造后按段映射，超长行对前端透明）。
- 编辑菜单补全（撤销/重做/剪切/复制/粘贴/全选/查找/替换/另存为）；文件菜单新增重新加载；动作经 `src/lib/edit/actions.ts` 信号通道（seq 去重）从菜单传到编辑层；另存为流程（save 对话框 → 复用编码询问弹窗 → `save_tab_as` → 标签重定向）。后端：AppState find/replace 三方法 + 3 个命令 + `QUERY_TOO_BROAD` 码；`reload_tab` 为无条件重建（丢弃编辑文档），前端负责脏态确认。
- E2E `scripts/smoke-find.mjs` **14/14**（F1–F14：查找条/替换行/连续替换不区分大小写/全部替换+提示/三次撤销全还原/大小写未找到/命中选中/Esc/菜单撤销/另存为链路/脏态重载）；截图 `docs/screenshots/phase4c-find.png`；smoke-edit 回归 12/12；svelte-check 0/0。
- 已知：系统原生「另存为」文件选择框无法脚本化，E2E 经 IPC 直调覆盖保存链路，对话框点选由人工核验（已登记）。

### 切片 4c-4 详情（IME 组合输入 E2E）

- `scripts/smoke-ime.mjs` **7/7**：I1 打开并进入编辑 → I2 组合中 preedit 悬浮显示（ni）→ I3 候选更新（你）→ I4 提交入库（你alpha + 脏态 + preedit 消失）→ I5 取消组合无副作用 → I6 保存后磁盘 UTF-8 中文 → I7 截图。
- 关键发现：本 WebView2 的 CDP 协议面无 `Input.imeCommitComposition`；**组合中的 `Input.insertText` 会被 Blink 路由为“提交当前组合”并触发 `compositionend`**（探针记录完整事件序列：compositionstart → update → beforeinput/input → … → compositionend）；空文本 imeSetComposition 为取消路径。
- 阶段 4 全部完成：4a 后端接线、4b 交互层、4c 超长行分段 / 查找替换 / 菜单补全 / 另存为 / 重载 / IME 验证。

## 附加：自定义标题栏（2026-10-02，维护者指示，先于阶段 5）

- 动机：原生标题栏（图标/名字/三大件按钮）视觉不合格；维护者要求自绘并经其验收后再进入阶段 5。
- 实现：`decorations:false`；新增 `src/lib/components/TitleBar.svelte`（应用图标 16px + 「文件名 - S-Read-TXT」 + 拖拽区（双击最大化/还原） + 最小化/最大化(还原)/关闭，46×32、关闭悬停 #E81123）；`document.title` 随活动文件同步；`--h-titlebar:32px` 令牌；图标资产 `src/assets/app-icon.png`。
- 权限（最小集新增）：allow-start-dragging / allow-minimize / allow-is-minimized / allow-toggle-maximize / allow-is-maximized。
- E2E `scripts/smoke-titlebar.mjs` **7/7**（T1 标题联动 / T2 结构 / T3 按钮最大化 / T4 还原 / T5 双击切换 / T6 双主题截图 / T7 最小化经 is_minimized 断言——WebView2 最小化不改变 visibilityState）；smoke-edit 回归 12/12；svelte-check 0/0。
- 截图：`docs/screenshots/phase5-titlebar-{light,dark}.png`。
- 待人工核验：无边框窗口的边缘/角落拖拽缩放与窗口阴影（Windows 下由框架处理）；如发现缺失将补边缘拖拽手柄。

## 附加：全按钮审计与修复（2026-10-02，维护者指示）

- 维护者要求：所有显示出来的按钮都必须测试；未实现的功能保持灰态、不测点击。
- 交付 `scripts/smoke-buttons.mjs` **27/27**：空状态/工具栏全按钮/菜单栏全部项/标签栏/查找条关闭/状态栏/退出流；原生对话框（打开/另存为）经 user32 枚举进程内 `#32770` 窗口 + `WM_CLOSE` 自动开合并关闭；PS5.1 参数改经环境变量传递（`-Command` 会拼接尾随参数，不可用）。
- **审计发现并修复的真实缺陷**：
  1. **关闭按钮失效**：标题栏 × / 菜单「退出→不保存」/三态「不保存」都无法关闭应用——`onCloseRequested` 未被阻止时 JS 侧调用内部 `destroy`，而最小权限集缺 `core:window:allow-destroy`；已补权限（独立探针 ALIVE→GONE 实证）。
  2. **剪贴板权限弹窗**：菜单「粘贴」弹出「http://tauri.localhost 想要查看剪贴板」且读取失败；复制/剪切/粘贴全部改走 `tauri-plugin-clipboard-manager`（Rust 侧，无弹窗；版本两边统一 =2.4.1）。
  3. 工具栏「历史记录」「设置」可点但无反应 → 改灰态（待阶段 7/8）。
  4. 状态栏「编码（点击切换）」是假标签 → 改为真实上弹菜单（新增共享组件 `EncodingMenu.svelte`，工具栏/状态栏同款）。
  5. 查看菜单「全屏」灰 → 启用（含 F11）。
- 回归：smoke-edit 12/12、smoke-titlebar 7/7、smoke-abuse 41/41；截图 `docs/screenshots/phase5-buttons-{menu,statusbar}.png`。

## 阶段 5 详情（快捷键引擎与设置窗口；2026-10-02）

### 5a 引擎 + 默认方案（提交 09cb953）
- `src/lib/shortcuts/{types,keys,engine}.ts` + 单测（vitest 70，其中新增 18）：捕获阶段全局监听、组合键规范化（与 Rust 默认表同格式 `Ctrl+Shift+Tab`/`PgDn`/`F11`）、弹窗挂起、编辑上下文让位（PgUp/PgDn/Home/End 归编辑器）、Ctrl+1~9 固定标签跳转（不参与自定义）
- 15 动作接线（打开/保存/另存为/编辑切换/关闭/循环标签/翻页/首尾/全屏/查找/替换/历史提示）；绑定加载 `get_settings` + `srt://shortcuts-changed` 事件 + 窗口聚焦双通道热刷新
- **修复真实缺陷**：前端 `tabs.select` 只改前端镜像 → 后端活动标签不同步（点击标签后关闭回落错误）；新增 `set_active_tab` 命令；后端 close 回落改为「东侧相邻优先，无则西侧」（`app_state.rs`）+ 2 测试
- 新增 `get_default_shortcuts` 命令（默认表唯一真源）；`scripts/lib/dialog.mjs` 抽取共享原生对话框探针
- E2E `scripts/smoke-shortcuts.mjs` **17/17**（固定键/循环/关闭/翻页首尾/F11/编辑切换/查找条/打开对话框）

### 5b 设置窗口 + 自定义（本次提交）
- Rust `open_settings` 命令：**按需创建**独立窗口（`WebviewWindowBuilder`，640×520、无边框、不可最大化）；已存在则显示+聚焦——避免常驻隐藏 WebView 的内存开销
- 前端多入口：`settings.html` + `src/windows/settings/{main.ts,SettingsApp.svelte,ShortcutsTab.svelte}`；五页签（快捷键可用，其余占位）；`TitleBar` 增 `showMaximize` prop；`vite.config.ts` 多入口
- 快捷键页签：15 动作列表、点击录制（Esc 取消、仅修饰键等待主键）、冲突检测（重复动作拒绝）、保留键拒绝（Ctrl+1~9）、单条/全部恢复默认、**修改即存**（覆盖表落盘）→ `emitTo('main','srt://shortcuts-changed')` 主窗口热刷新
- 工具栏「设置」启用；ipc 增 `openSettings`/`getSettings`/`saveSettings`/`getDefaultShortcuts` 与配置类型
- 修复：`save_settings` 前端入参形状错误（后端 `shortcuts` 为**扁平映射**，非 `{schemaVersion, bindings}`）——首次真实调用该命令时暴露
- E2E `scripts/smoke-settings.mjs` **17/17**（录制→落盘→冲突→保留键→取消→单条恢复→全部恢复→主窗口自定义生效/旧键解绑/恢复后复原）；`smoke-buttons` C8 改「打开设置窗口→关闭」**28/28**；`smoke-cdp.findTarget` 增 URL 过滤（多窗口目标选择）
- 截图 `docs/screenshots/phase5-settings.png`
- **教训（写入本文件）**：①冒烟套件必须串行运行；②`cargo test`/`cargo build` 会用 dev 语义覆盖 `target/debug/s-read-txt.exe`（窗口显示「localhost 拒绝连接」）——冒烟前必须 `npm run tauri build -- --debug --no-bundle` 重新构建；③关闭窗口的 CDP 调用不可 `await`（窗口销毁后响应永不到达）——改走标题栏关闭按钮的 fire-and-forget 点击。

### 5c 全量自检、多语言与扩展测试（本次提交）
- **根治「localhost 拒绝连接」**（教训②的永久修复）：根因 = Tauri 以 `custom-protocol` feature 区分打包/开发模式，裸 `cargo build`/`cargo test` 未启用 → dev 语义；修复 = `Cargo.toml` 常开 `custom-protocol`（故意裸 `cargo build` 后直接启动实测正常）；README 已记录与代价（不支持 `tauri dev` HMR，本项目不用）
- **`scripts/verify-all.mjs` 全量自检**：一条命令串行 16 步（编码/svelte-check/vitest/fmt/cargo 全测/构建/11 套 E2E），报告落盘 `docs/verify/latest.md`；首跑 11/15 暴露 4 项问题（fmt 未过、settings 缺对话框助手、shortcuts 打字竞态、abuse 偶发）→ 修复后 **15/15**
- **多语言**：Rust +10 项（编码往返/EUC-KR 检测/日韩西里尔查找/ZWJ emoji 编辑/组合附加符/Shift_JIS 保存字节往返/会话自动检测）；新 E2E `smoke-i18n.mjs` **27/27**（8 语言内容 + Shift_JIS/EUC-KR/Big5/windows-1252 的检测/显示/切换/编辑/保存字节一致）
- **E2E 扩展**：`smoke-settings` 30 项（录制/冲突/保留键/恢复默认/主窗生效/**跨重启持久化**/页签占位）、`smoke-shortcuts` 19 项（PgUp/历史提示/无标签安全/Ctrl+S 保存弹窗/Ctrl+Shift+S 另存为/三态弹窗与模态挂起）、`smoke-buttons` C8（开设置窗口→关闭）
- **真实缺陷（测试发现）**：替换/全部替换后焦点停留在查找条 → `Ctrl+Z` 失效；修复 = 动作成功后 `focusEditorProxy()` 归还焦点
- 测试总量（截至 5c）：cargo **186 lib + 15 对抗 + 5 集成**；vitest **77**；E2E 套件 11 个（edit/find/ime/i18n/titlebar/buttons/settings/shortcuts/abuse/longline/…）；`node scripts/verify-all.mjs` 一键复现全部质量门禁

## 阶段 6：设置全部落地 + 会话恢复 + 数据目录兜底（进行中；2026-10-02）

### 6a/6b 设置全部落地（提交 5814785）
- 主窗口设置状态：`appSettings`/`readerSettings` 载入与持久化（`reloadSettings`/`persistReader`/`srt://settings-changed` 广播热刷新）；排版实时预览（CSS 变量字体/字号/行高/限宽/边距 + `layoutKey` 触发行高模型失效重排）；查看菜单字号增大/减小/重置字号。
- 设置窗口五页签全部落地：常规（文件大小上限/标签上限/日志级别/保存备份开关）/ 阅读排版（主题/字体/字号/行高/限宽/边距，改动即存）/ 快捷键（沿用 5b）/ 历史（保留条数与天数/清空二次确认）/ 关于（版本 + 仓库占位）；`open_settings(tab)` 支持指定页签（`take_settings_tab` 握手）。
- 证据：ACTIVE_TAB=阅读排版 / READER_JSON_HAS_22=true / MAIN_CSS_SIZE=22px / GENERAL_MAXFILE=100；smoke-settings 30/30 回归。

### 6c 会话与窗口恢复 + 首启引导（本切片）
- 新增 `src/lib/session.ts`：采集（窗口几何/最大化、标签路径/编码覆盖/滚动行/编辑态、活动下标；异常回退默认 1100×760）、静默保存、启动恢复（先 seed 滚动记忆 → applyView → 激活标签；缺失文件跳过 + Toast）。
- 触发时机：标签集合/活动标签变化 2s 防抖；窗口移动/缩放事件监听 + 30s 周期兜底；退出统一先保存再关闭（X / 菜单退出 / 三态不保存同一路径，`onCloseRequested` 统一拦截）。
- 首启引导 `Onboarding.svelte`（打开文件/多标签/历史/快捷键四要点 + 「不再显示」勾选，持久化）。
- **本切片修复的两个真实缺陷**：①滚动记忆只在切换标签时写入 → 滚动后直接退出恢复不到位置；改为滚动 rAF 内实时记录。②冷启动恢复早于首屏渲染时容器尚无足量可滚动高度，`scrollTop` 被钳到 0；新增 `applyInitialScroll` 逐帧重试至赋值生效（约 2s 上限）。
- E2E `scripts/smoke-session.mjs` **11/11**：建现场（两标签 / b 滚动 1500 / a 编码覆盖 GB18030 / 窗口移动 100,100,900×600）→ X 关闭 → 重启核对（标签/活动/编码覆盖/几何 ±3/±45/滚动 1498）→ 删除文件后缺失跳过。
- 已知怪癖（记录）：无边框窗口经 `GetWindowRect` 读取含不可见缩放边框（宽约 16px、高约 31px），E2E 几何断言容差：位置 ±3、尺寸 ±45。

### 6d 数据目录不可写引导（本切片）
- 后端：`storage/paths.rs` 三级优先级 `resolve_data_dir_core(runtime, env, exe)`（运行时覆盖 > `SRT_DATA_DIR` > 便携目录）+ `DataDirOrigin::RuntimeOverride`；`logging` 重构为「全局可替换日志槽」（`CURRENT: RwLock<Option<FileLogger>>`，`FacadeLogger` 变空结构读取当前槽）+ `retarget(data_dir)`；命令 `set_data_dir(dir)`（探测→设覆盖→日志重定向→返回状态）；main.rs 注册。
- 前端：`DataDirDialog.svelte`（路径/原因展示 + 选择可写目录（推荐）/ 仅本次只读运行）；共享状态 `src/lib/state/data-dir.svelte.ts`（`dataDirStore`：check/apply/skip；**App 与自动化钩子走同一 apply 路径**）；App 启动探测 + `plugin-dialog` 目录选择器 + Toast 反馈。
- E2E `scripts/smoke-datadir.mjs` **12/12**：`SRT_DATA_DIR` 指向一个「文件」模拟不可写 → D1 弹窗（路径 + os error 183）；D2 切换可写目录（`writable=true`、`origin=runtimeOverride`、弹窗消失）；D3 `history.jsonl` 与 `logs/app.log` 实落新目录、原路径未被误建；D4 重启再弹窗（会话级语义）→ 「仅本次只读运行」后应用可用。
- 行为说明：WebView2 用户数据目录仅在启动时重定向（启动不可写则跳过）；引导切换的会话级目录只影响应用数据文件，WebView 缓存本次仍用系统默认位置（不违反“不写入程序目录之外”的本意：程序目录不可写时无便携位置可选，且已在弹窗中告知）。

### 6e E2E 扩展（本切片）
- `smoke-settings.mjs` 修正与增强：默认页签已改为「常规」→ 快捷键操作前显式切换页签（S1/S9/reopenSettings）；**首启引导模态会挂起全局快捷键** → 按真实用户路径勾选「不再显示」并关闭（S8/S9/S11/S12 恢复）；`openPath` 改 fire-and-forget 防 WebView2 `Promise was collected`；S13 改为真实断言（常规页签字段 / 快捷键行数 / **字号修改实时应用到主窗口**）→ **27/27**。
- `verify-all.mjs` 新增两步：`E2E 会话恢复（smoke-session）`、`E2E 数据目录引导（smoke-datadir）`；报告仍落盘 `docs/verify/latest.md`。

### 6f 全量自检与回归修复（本切片）
- 全量自检 **18/18 通过**（189.0s；报告 `docs/verify/latest.md`）——12 套 E2E + 6 个前置门禁全绿（含 100MB 长行 9/9）。
- **修复真实缺陷（渲染层）**：`ReaderView` 排版变更 effect 内 `version += 1` 同时读写同一 `$state`，被 Svelte 5 依赖收集后自触发死循环（实测每帧数千次，行文本不渲染/渲染管线被持续冲刷）；修复 = `untrack` 包裹内部写入。用临时探针（get_rows 载荷/缓存/计数）二分定位到渲染层后取舍证据，调试钩子已移除。
- **测试基建（CDP/WebView2 兼容）**：
  - 新增共享 `openPathDone()`：WebView2 对「直接调用函数返回的 Promise」经 CDP `awaitPromise` 必报 “Promise was collected”（2026-10-02 起实测必现）；10 个套件统一迁移 + 旧 `smoke.mjs` 同步。
  - 新增共享 `dismissOnboarding()`：首启引导模态会挂起全局快捷键并遮挡 `[role="dialog"]` 选择器（8 个套件因此失败）；按真实用户路径关闭（勾选「不再显示」+ 开始使用）。
  - `smoke-buttons` D9/D12 的“灰态”断言更新为实测行为（字号±/快捷键…/关于页签，**30/30**）；`smoke-settings` S13 改为真实页签/实时排版断言并修复单引号模板比较 bug（**27/27**）。
- 证据产物已刷新：`docs/verify/latest.md`、各阶段截图（4b/4c/5）。

## 附加：弹窗遮罩让位标题栏（2026-10-02，维护者指示）

- **维护者报告**：首启引导打开时窗口「不动」（无法拖拽/操作标题栏）。
- **根因**：六处模态遮罩（引导/保存/确认/未保存三态/数据目录/历史面板）均为 `position: fixed; inset: 0` 全屏覆盖，自绘标题栏（无边框窗口的拖拽区与三键）被一并盖住 → 窗口无法拖动、最小化/最大化/关闭不可点。
- **修复**：遮罩统一从标题栏下方开始（`inset: var(--h-titlebar) 0 0 0`），弹窗期间标题栏始终可用；组件顶部以注释说明该约束。
- **验证**：`smoke-titlebar` 新增 T7（引导打开时 `elementFromPoint` 命中拖拽区）与 T8（CDP 真实坐标点击最小化成功，JS `.click()` 会绕过命中测试）→ **8/8**；回归 `smoke-buttons` **30/30**（C3 过期灰态断言更新为真实面板开闭）、`smoke-settings` **27/27**、`smoke-abuse` **41/41**。

## 阶段 7：多标签与历史记录（2026-10-02）

### 7a 多标签增强（提交 e0cb6a4）
- 后端 `app_state.rs`：新增展示顺序 `order: Vec<u64>`（与 BTreeMap 解耦），标签栏顺序与关闭回落同源；`reorder` 方法 + `reorder_tab` 命令（移除后插入语义，越界收敛）。
- 前端：`TabBar.svelte` 重写（指针拖拽排序含落点指示线、中键关闭、右键菜单、滚轮横向滚动）；新增 `TabContextMenu.svelte`（关闭/关闭其他/关闭全部）；`tabs.svelte.ts` 乐观排序 + 失败回读；App 接脏标签保护（关闭其他/全部时保留脏标签并提示）。
- E2E `smoke-tabs.mjs` **7/7**（顺序/中键/拖拽/关闭其他/关闭全部/上限）；`verify-all` 纳入。

### 7b 历史记录（本次提交）
- 后端：`update_history_progress` 命令（按路径回写 lastRow/lastPercent）；时间戳毫秒精度（`now_rfc3339` 毫秒 + `now_unix_millis`）+ `sort_key`/`prune` 毫秒化——修复快速连续打开时排序抖动。
- 前端：`history.svelte.ts`（共享 store：加载/单删/清空/重开恢复进度/关闭与退出路径回写）、`HistoryPanel.svelte`（虚拟列表/搜索/删除/清空二次确认/进度与大小展示）、菜单「最近打开」子菜单（最近 10 条）、工具栏启用、App 各关闭路径（X/退出/重载/关闭其他/全部）即时 flush。
- **修复真实缺陷（滚动恢复竞争）**：`applyInitialScroll` 原在内容高度未建立时逐帧重试把 `scrollTop` 拉回顶部（约 2 秒），与用户随后的滚动竞争并把滚动记忆写成 0 → 历史进度丢失/会话位置被覆盖；修复为「先 `refreshWindow()` 建立占位与首批取行 → 仅对第 >0 行重试 → 用户滚轮/指针/触摸/按键立即中止重试（`scrollEpoch`）」。
- E2E `smoke-history.mjs` **11/11**（面板/搜索/进度回写/重开恢复/删除/清空/最近打开/快捷键）；回归 `smoke-session` **11/11**、`smoke-edit` **12/12**、`smoke-abuse` **41/41**；`verify-all` 纳入。

### 7c 查找替换 v2（本切片，两次提交）
- **用户扩展需求**：双模式（标准 / 用户自写正则）；批量操作二次确认（列出各命中行号与将被替换的句子，可逐条删除）；高亮替换字符；正则引擎选定 Rust `regex`（线性时间）；随后追问要求覆盖其他语言与特殊符号。
- 后端（提交 f1cd802）：`Matcher` 抽象（字面折叠 + regex 引擎；`^`/`$` 按行锚定、零宽跳过、`$1` 展开）；`find/replace_next/replace_all` 增加模式参数；新增 `preview_replace_all`（列表上限 500 条、文本截断 200 字符、`stateId`）、`replace_matches`（剔除后按序号执行、过期拒绝 `StaleSearch`）、`match_window`（可见窗口高亮，越窗即停）；新错误码 `INVALID_REGEX`/`SEARCH_STALE`；`regex =1.13.1` 精确锁定。
- **语言/符号深测（用户问「其他语言呢？特殊符号呢？」）**：修复两个真实缺陷——希腊终结 sigma（ς 搜不到 σ）、土耳其带点大写 İ（"İstanbul" 搜不到 "istanbul"）；新增 8 项边界单测：德语 ß（Unicode 简单折叠语义，ß≠ss）、正则样式字符按字面、ZWJ emoji 序列、零宽/双向控制符、阿拉伯 RTL、NFC/NFD 组合符号精确匹配、全角/半角区分。cargo **214 lib + 15 对抗 + 5 集成**。
- 前端（本次提交）：查找条新增 `.*` 正则开关（Aa 对两种模式通用）；`ReplacePreviewDialog`（行号 + 被替换字符红色高亮 + 替换后文本绿色预览 + 逐条勾选/全选/全不选 + 截断阈值提示）；EditLayer 预览流程（=0 提示 / =1 直执 / ≥2 弹窗）与文档高亮（可见窗口防抖 150ms、渲染上限 800 盒、与引擎同源语义；关闭查找条即清除）。
- E2E `smoke-find.mjs` 扩至 **19/19**（F15 正则命中+高亮、F16 非法正则提示、F17 预览剔除后仅替换所选、F18 单步撤销、F19 关闭清理；F6 覆盖直执路径）；回归 `smoke-i18n` **27/27**（中文替换恰 1 处走直执路径）、`smoke-edit` **12/12**、`smoke-abuse` **41/41**。
- 设计文档 §5.3 与 D18 已同步为 v2 规格。

### 7d 收尾修复与全量自检（本切片）
- **真实缺陷（全量自检暴露）**：快速连点主题按钮时主题会「回跳」——`persistReader` 的异步响应把较旧快照回灌并覆盖更新的本地选择（主题循环又基于本地状态推导下一步，错误被放大）；修复 = 序号守卫（仅采纳最后一次请求的响应），设置窗口 store 同类场景一并加固。
- 测试稳定化：`smoke-buttons` C7 改为按期望值轮询（链式运行较慢时固定延时读取会抖动）；`smoke-history` H3b 延长等待（12s，历史重开后建索引 + 恢复）。
- **全量自检 20/20 通过（191.6s）**：12 项门禁 + 12 套 E2E（含 100MB 长行、多语言、IME、历史、会话、对抗）。

## 阶段 9：启动/内存策略、滚动锚定修复与系统要求（2026-10-02）

### 内存构成分析与策略（维护者拍板：in-process-gpu + 特性裁剪）
- **构成实测（10×100MiB 标签、专用工作集）**：基线 97.7MiB = 浏览器 31.2 + GPU 25.9 + 渲染器 24.7 + utility 7.0/3.2 + **应用（Rust）仅 4.3** + crashpad 1.4；JS 堆 2.8MB、DOM 884 节点 → 我们的代码/数据结构几乎不占，大头是 WebView2 运行时基线。
- 变体对照：`--disable-gpu` 80.1（但图片走软件光栅，不利未来 EPUB 内嵌图片）/ `--in-process-gpu` 88.3 / 堆 + 特性裁剪 95.6 / **in-process-gpu + 裁剪 81.9**。维护者要求按「未来格式 + 内嵌图片」重估后选定**后者**。
- 实施：`main.rs` 启动注入 `--in-process-gpu --disable-features=WinUseBrowserSpellChecker,CalculateNativeWinOcclusion,msWebOOUI,msPdfOOUI --disable-background-networking --disable-component-update --disable-extensions --disable-sync`（外部参数保留；显式 `--in-process-gpu` 时不重复追加）；裁剪项同时强化「完全离线」。
- 复测：**79.2MiB**（无独立 GPU 进程，GPU 服务并入浏览器）；压力后峰值 105.8MiB（<120 兜底，理想线 >100 为 churn 增长，属可回收的渲染进程内部缓存）。

### 启动策略（维护者拍板：立即显示 + 内置占位）
- `main.rs`：读 reader.json 主题 → `set_background_color`（浅 #FAF9F7 / 深 #1E1E1E / 护眼 #F5EFE0；system 读窗口明暗）→ **显示之前**应用会话窗口几何（原前端职责迁移，消除可见跳动）→ 立即 `show()`；占位样式 `index.html` 内置（CSP 下内联 <style> 正常，受 `style-src 'unsafe-inline'` 管控）；`main.ts` 挂载前 `replaceChildren()`；App 只保留归还焦点。
- 度量口径更新（`measure-startup.mjs`）：可见 = 日志「启动→主窗口已显示」；内容就绪 = CDP（`__srt` + 标题）。实测 release 5 次：可见 581–685ms、内容就绪 median 718ms/max 822ms → **PASS**。

### 滚动锚定真实缺陷（压力测试暴露，已修复）
- 现象：100MB 文件冷态首次远跳停在不复位的 694,384（S5 14/15、漂移 594,384px）。
- 排查链（探针证据齐全）：10MB 文件正常（内容高 3.4M<Chrome 33.55M 上限）→ 纯 DOM 静态稳定 → 加「跳转后 DOM 重建」复现（`auto`=199,064、`overflow-anchor:none`=100,000）→ 无任何 JS 写入（setter 钩子）→ 根因 **Chromium 滚动锚定 + 应用内反馈放大 6.9 倍**；`--disable-features=ScrollAnchoring` 实测无效（特性名失效）；CSS 注入被 CSP 拦（对照实验曾因此无效）。
- 修复：`.reader { overflow-anchor: none; }`。验证：首跳精确 100,000；`stress` **11/11**（S5 15/15、漂移全 0、耗时 3078→128ms）。

### 系统要求结论（Win7 问询）
- 官方证据：Rust 1.78 发布说明（`x86_64-pc-windows-gnu` 等目标最低 Windows 10）；Tauri 2.12.1 MSRV=1.90；`time`/`encoding_rs` 需 1.88、`webview2-com` 需 1.82；微软文档「Edge 对 Win7 支持于 109（2023-01）终止」。
- 决策：**不支持 Win7/8.1，明确写系统要求 Windows 10 1803+**；不附无法验证的旧运行时包。已同步 README / Release 说明 / known-issues。

### 符号按钮悬浮提示审计（维护者要求）
- 全量盘点：工具栏 8 键 / 标题栏三键 / 标签关闭 / 查找条（`.*`、`Aa`、`×`）/ 历史面板（关闭、单条删除、清空）/ Toast 关闭 —— 仅**历史面板「关闭」缺失**；已补齐 `title`，查找条关闭提示补注 Esc。
- 顺带修复测试基建：`smoke-buttons` C8b 链条偶发失败根因 = **CDP 目标出现 ≠ Svelte 已挂载**（高负载时关闭按钮尚未渲染，`?.click()` 静默点空 → 设置窗口残留 → C8b/D12 连锁失败）；加固为先轮询关闭按钮存在的再点击；单跑 **30/30**。

### 安装范围与启动按需提权（维护者要求，2026-10-02）【启动权限部分已于同日修订为一次性授权助手，见文末「启动权限修订」】
- **安装器**：`bundle.windows.nsis.installMode: "both"` → 生成模板含 `INSTALLMODE "both"` + `MULTIUSER_EXECUTIONLEVEL Highest` + 多用户选择页（官方简体中文：「为本机所有用户安装 / 只为我自己安装」）；**选「所有用户」才请求 UAC**（MultiUser 宏按选择动态提权）；产物仍 1.87 MiB。
- **启动按需提权**（新增 `src-tauri/src/elevation.rs`；`windows-sys =0.61.2` 精确锁定，features：Foundation/Security/System_Registry/System_Threading/UI_Shell/UI_WindowsAndMessaging）：判据 = 数据目录可写探测（唯一需要权限的操作即写 `data/`）；不可写+非管理员+未尝试 → `ShellExecuteExW("runas")` 重启（子进程继承环境含 `SRT_DATA_DIR`）；防循环 `SRT_ELEVATION_ATTEMPTED`；逃生阀 `SRT_NO_ELEVATION`；已提权仍不可写 → 回落引导；用户取消 → 引导。决策表 5 项单测 → cargo **219 库单测（239 全量）**。
- **集成实测**：不可写目录 + `SRT_NO_ELEVATION=1` → 应用存活、`data_dir_status.writable=false`、数据目录引导弹窗出现（决策链与回落正确）；可写场景零打扰（全量套件回归）。真实 UAC 路径无法自动化（安全桌面不可脚本交互）→ 由维护者在「所有用户」安装后人工验收。
- **测试基建修复**：`smoke-datadir` 显式 `SRT_NO_ELEVATION=1`（不可写场景不再触发 UAC——此前 verify-all 链中该套件失败即此因）；`measure-startup` 支持 `SRT_MEASURE_EXTRA_ARGS`（启动参数 A/B）。
- **冷启动复测**（release，5 次 × 2 组）：默认参数组 可见 median 621ms / 内容就绪 median 758ms；仅 `--in-process-gpu` 组 615ms / 733ms —— 参数组对启动无实义影响（差异在噪声内）；首跑离群（可见 1077 / 就绪 1225ms）= 新构建后杀软扫描 + 全新 WebView2 配置目录，属环境性（稳定态全新 profile 实测 716ms）。结论：中位数较阶段 9 前（就绪 886ms）**反而更快**；离群值原因已写入 known-issues。
- **CI/CD 更新**：CI 卫生作业新增编码与换行自检；CI/Release 的 Rust 作业新增 `cargo fmt --check`；Release 描述「安装说明」重写为 7 条（系统要求 / 安装范围二选一 / 架构 / 未签名 / WebView2 / **启动权限行为** / 离线与数据位置）。

### 启动权限修订：整体提权 → 一次性授权助手（2026-10-02，用户上报的 Program Files 启动故障）
- **故障现象**：按机器安装到 `D:\Program Files\S-Read-TXT` 后首次启动报「Microsoft Edge 无法读取和写入其数据目录（`data\webview\EBWebView`）」；日志仅有「启动」行（无「主窗口已显示」）；目录 ACL 仅 `Users:(RX)`；用户手动「给权限」无效。
- **根因**：整体提权实例设置 `WEBVIEW2_USER_DATA_FOLDER` 后，普通权限的 WebView2/Edge 子进程写不进 `Program Files`（提权与子进程权限模型冲突）；WebView2 初始化失败又被 `main.rs` 的 `.expect()` 静默吞掉（`windows_subsystem` 无控制台）→ 无界面、无提示。
- **修订方案（维护者选定「运行时一次性提权 + 仅当前用户」）**：`elevation.rs` 重写为**授权助手**——`--prepare-data-dir <路径> --grant-sid <SID>` 只创建目录并用 `icacls` 授予当前用户修改权限后立即退出（绝不进入 Tauri/WebView2）；应用本体始终以普通权限运行；已提权启动则就地幂等授权；启动失败补「日志 + 原生错误框」；移除 `SRT_ELEVATION_ATTEMPTED`（不再需要）；`SRT_NO_ELEVATION=1` 语义＝跳过提权初始化。
- **验证**：单测 7 项（计划表 / 参数解析含顺序无关与缺参拒绝 / ACL 真实往返回归）+ 真实 exe 集成测试 2 项（助手建目录并授权；缺参退出码 2）→ cargo **243/243**；`smoke-datadir` **15/15**（新增 D5：只读 ACL 预置 → 助手模式修复 → 当前用户可写）；代码提交 `8dfd846`、测试提交 `3da8298`，文档随本次提交。
- **文档同步**：README（权限行为表与补充说明、环境变量表）、known-issues #3、configuration.md、设计 D36、CHANGELOG、Release 安装说明第 6 条、本台账。

### 设置界面改造与卸载清理（2026-10-02，维护者三项反馈中的第 1、2 项）
- **设置界面**（提案确认后实施）：全部数值项改「滑块 + 实时数值」双控件（拖动经设置窗口 store 140ms 节流落盘实时预览、松开/数字框确认立即落盘）；通用控件（SliderRow/ChoiceRow/ToggleRow/ActionRow）+ 共享样式 `src/windows/settings/settings.css` 按配置表驱动；排版新增「上下边距」（后端 `pagePaddingY` + `--reading-pad-y` 接线；旧配置缺字段自动取默认，向后兼容测试覆盖）；范围放宽（`src-tauri/src/settings/defaults.rs` 常量 + store 钳制断言同步）：字号 8–72 / 行高 1.0–3.2 / 限宽 320–2400 / 边距 0–240 / 标签上限 200 / 历史 100–1,000,000 条、1–36500 天；设置窗口 640×520 → 800×620；控件带 `data-setting` 供自动化定位。
- **卸载清理**（第 2 项）：根因 = Tauri NSIS 主模板的 `RMDir "$INSTDIR"` 不带 /r（便携 `data/` 非空即保留）且自带「删除应用程序数据」仅覆盖 `%APPDATA%`/`%LOCALAPPDATA%`；修复 = 新增 `src-tauri/nsis/installer-hooks.nsh`（POSTUNINSTALL：交互式询问是否删除 `data/`、默认删除；静默卸载 `/SD IDYES` 默认删除；清理注册表安装位置/语言残留；补删空安装目录），`tauri.conf.json` 启用 `installerHooks`。
- **测试新增/加固**：
  - `scripts/smoke-uninstall.mjs`（新增，已加进 verify-all）：预清理 → 静默安装（`/S /currentuser`）→ 造数据 → 静默卸载 → 断言目录（含 data）与注册表全清；**6/6 通过**。
  - 关键解谜（记入脚本头注）：Node `spawn/spawnSync` 启动 `highestAvailable` 清单的 NSIS 安装器/卸载器会得到 **EACCES**（CreateProcess 返回 ERROR_ELEVATION_REQUIRED=740，libuv 映射为 EACCES）——必须经 ShellExecute（PowerShell `Start-Process`）；此前链式运行中所有 “status=null” 均为此因，而非卡死。
  - `scripts/smoke-settings.mjs` 加固：`Page.bringToFront` + `startRecording`（点击后确认进入录制态、首击偶发丢失时重试一次）+ S2a/S2b 输出诊断详情；**27/27 通过**（此前一次 16/27 属首个按键注入的时序抖动，探针证实功能正常）。
- **验证汇总**：cargo 244（222 lib + 15 对抗 + 2 助手 + 5 集成）；svelte-check 0 错 0 警；smoke-settings 27/27；smoke-uninstall 6/6；提交见 git 历史。

## 设置入口扩充与滚动完整性套件（2026-10-03，维护者反馈第 3 轮）
- **设置入口扩充（B 批次，已确认三项全选）**：标题栏齿轮按钮（`TitleBar` 增 `showSettings`/`onSettings`，aria「打开设置」，设置窗口自身不显示）；文件菜单「设置…」；工具栏「设置」按钮改带文字标签（图标 + 文本，`icon-btn.with-text`）。
- **滚动渲染完整性套件**（维护者反馈「滑动多次渲染不出来」）：新增常驻 `scripts/smoke-scroll.mjs` —— A 随机跳转 30 轮 / B 滚轮连发 5×20 / C 上下震荡 3×30（落点稳定性）/ D 滑块连调 13 轮（滑块 input/change 与数字框交替、含 8/72 极值），逐轮断言可视行内容完整、排版变量到位；并入 verify-all（21 步）。当前构建 **4/4 全绿**（既有 30 跳 + 100 滚轮探针亦 0 空白）；该缺陷类别已由 `7c81032`（占位即时重算）与状态（fetch 完成/缓存写入均触发重渲染）覆盖，后续回归由本套件兜底。
- **修复**：设置导航容器 `nav` → `div`（a11y：non-interactive 元素不得承载 tablist 角色，svelte-check 恢复 0/0）。
- **扩展路径记录**：维护者提示「之后可能扩充语言」——文案集中组件层，届时按「语言包 + 设置项」整体抽取（已记入设计文档 § 功能范围）。
- **验证**：`smoke-titlebar` **10/10**（新增 T9 齿轮开设置 / T9b 关闭）、`smoke-buttons` **32/32**（新增 D13a 工具栏文字标签 / D13b 文件菜单入口）；`npm run check` 0 错 0 警。

## 自定义字体与排版/界面扩充（C 批次，2026-10-03）
- **后端**：新模块 `src-tauri/src/fonts.rs`（导入/列表/删除/读取；扩展名白名单 ttf/otf/woff/woff2、单文件 64MB 上限、防目录穿越、重名唯一化、内置 Base64 编码含 RFC 4648 测试向量）；4 个命令 `list_fonts` / `import_font` / `remove_font` / `read_font_data`；新错误码 `FONT_UNSUPPORTED` / `FONT_TOO_LARGE` / `FONT_NOT_FOUND` / `FONT_INVALID_NAME`。
- **模型扩充（全部 serde default 向后兼容）**：`Typography` +`paragraphSpacing` / `firstLineIndent` / `textAlign`（TextAlign 枚举，未知值归一）/ `smoothScroll`；`ReaderSettings` +`statusBar{showFileName,showPercent,showSize,showEncoding}`；`AppSettings` +`startup{restoreSession,restoreWindow}`；store 归一钳制同步；main.rs 窗口几何按 `restoreWindow` 门控（关闭时用默认几何）。
- **前端接线**：App 自定义字体 FontFace 动态加载（`custom:<文件>` 约定、加载缓存、失败可重试、加载完成触发排版变量重算）；CSS 变量 +`--reading-para-spacing` / `--reading-indent` / `--reading-align`（ReaderView `.row` 应用）；PgUp/PgDn 按 `smoothScroll` 使用平滑动画（首尾跳转保持瞬时）；会话恢复按 `restoreSession` 门控；StatusBar 四元素显隐 props。
- **设置 UI**：`TypographyTab` 重写（7 滑块 + 对齐/平滑开关 + 自定义字体导入与删除管理）；`GeneralTab` 新增「界面元素」「启动行为」组。
- **验证**：cargo **251/251**（229 lib + 15 对抗 + 2 助手 + 5 集成，0 告警）；svelte-check 0/0；`smoke-settings` **47/47**（新增 S14a–e 排版扩充、S15a–i 字体导入链、S16a–e 界面/启动开关）；回归 `smoke-scroll` 4/4、`smoke-titlebar` 10/10、`smoke-buttons` 32/32。
- **测试口径说明**：字体删除的原生确认框（TaskDialog）无法自动化点「是」——E2E 覆盖「弹框 + 取消保留」，删除链路经直连命令验证（S15h/i）。

## P0 基础设施与外观（2026-10-03 启动）
- **规划产物**：设计文档并入 §17（D39–D77 摘要）；docs/plan/2026-10-03-p0-plan.md（P0-1 双阈值 / P0-2 设置 v2 / P0-3 i18n / P0-4 设置 UI v2 / P0-5 主题 v2 / P0-6 背景图 / P0-7 欢迎页 / P0-8 资源策略 / P0-9 快捷键框架 / P0-10 配置迁移 / P0-11 验收）；三份提案状态改为「已批准执行」（维护者「完全开始」）。
- **主题令牌**：src-tauri/resources/themes/*.json 6 套落盘（13 令牌/套），scripts/check-theme-contrast.mjs 实测全部 WCAG AA 通过（护眼绿 accent 首轮 4.34 不达标 → 调整为 #3A7449 后 4.53）。
- **设计预览**：docs/design/previews/2026-10-03-themes-and-welcome.html（6 套色板 × 模拟窗口 + 浅/深欢迎页 mockup），待维护者确认后实施主题与欢迎页（其余 P0 任务不受阻）。
- **P0-1 大文件双阈值**（完成）：只读阈值/硬上限分离（默认 100MB / 2048MB，倒挂自动修正）；`TabInfo.readOnly` + 状态栏「只读」标记 + 编辑入口禁用与 Ctrl+E 拦截提示 + `FILE_READ_ONLY` 错误码；打开/重载/另存为全路径按硬上限放开、编辑按只读阈值限制；Rust 253 全绿（新增 3 项：双阈值行为、归一一致性、存储往返）；E2E `smoke-limits` **6/6**（101MiB 只读打开/标记/禁用/Ctrl+E/硬上限拒绝/设置双滑块）；并入 verify-all（22 步）。
- **P0-2 设置 schema v2（后端完成）**：新增 `settings/registry.rs`（27 项注册表：id / 分组 / 标签 / 类型与范围；界面生成、导入校验、重置作用域的唯一元数据源）、`settings/migrate.rs`（迁移链 v1→v2、`apply_chain`、迁移前备份 `<文件>.v<旧>.bak`，未来版本 / 损坏 / 失败一律保留原文件；启动时自动迁移）、`settings/bundle.rs`（导出/导入 JSON 包：bundleVersion 1、严格校验（未知字段 / 类型 / 范围 / 枚举 / 动作白名单 / 8MB 上限）、导入前 `*.import-bak` 备份、写入失败回滚）、`settings/reset.rs`（`ResetScope` 全部 / 分组 / 单项；默认值取自模型 `Default`）；4 个新命令 `export_settings` / `import_settings` / `reset_settings` / `get_settings_registry`（成功后广播 `srt://settings-changed`）；错误码 `SETTINGS_EXPORT` / `SETTINGS_IMPORT` / `SETTINGS_RESET`；会话 schema 版本解耦（`SESSION_SCHEMA_VERSION = 1`，不再随设置版本联动）。验证：cargo **275/275**（253 lib + 15 对抗 + 2 助手 + 5 集成）、`cargo fmt --check` 通过、0 告警。设置 UI（P0-4）接入注册表后 P0-2 全链完成。
- **路径规范（维护者要求）**：去除脚本硬编码目录——`verify-all` 经 `SRT_MSYS_BIN` 或 PATH 自动探测 MSYS2；系统字体路径取 `%WINDIR%`；临时目录用 `os.tmpdir()`；`smoke-limits` / `smoke-scroll` 改为相对脚本定位（原绝对项目路径）；压力报告与设计预览中的机器路径清洁化；README 与 configuration.md 增补 `SRT_MSYS_BIN`；AGENTS.md 新增规则 230「禁止硬编码目录与绝对路径」（两份副本同步）。
- **测试覆盖补测（2026-10-03）**：新建 docs/verify/coverage-matrix.md（43 命令/24 错误码/27 设置项的逐项证据索引）。新增 smoke-settings-io **15/15**（v1 启动迁移端到端 + 导出结构 + 三类篡改拒绝 + 合法导入与 import-bak + 重置三级 + 注册表断言）；smoke-history 长列表虚拟滚动 **13/13**（60 条→渲染 24）；smoke-tabs 溢出滚轮/拖拽取消 **10/10**；smoke-settings **48/48**（含 FontFace 实际加载）；offline-check 并入 verify-all（单步 36.4s）。Rust **283/283**（+7：注册表完备性/边界、bundle 边界 4、locale 变体）。修复测试自身缺陷：S13a 过期断言、H8 种子 encoding 字段类型。
- **P0-3b i18n 核心与外壳抽取**：src/lib/i18n/（zh-CN 类型源 / en Record 完备性 / translate 纯函数 / index.svelte runes 入口）；6 组件文案抽取（TitleBar/ToolBar/StatusBar/EmptyState/DropOverlay/Onboarding）+ App.reloadSettings 接 setLocale；vitest **82/82**（+5）；smoke-settings-io 扩展 L1–L4（重启后英文 UI）→ **19/19**；titlebar 10/10、buttons 32/32 回归通过。
- **P0-3c i18n 收口**：
untime.ts（纯模块，测试/非组件可用）+ index.svelte.ts（响应式）；App.svelte 全量运行时文案、ipc 固定错误、tabs store、ReaderView/EditLayer/Toast、快捷键动作名（SHORTCUT_LABEL_KEYS）与录制提示（RECORD_ERROR_KEYS）抽取；smoke-settings-io **21/21**（+L5 菜单/L6 空状态，重启英文 UI）；回归 edit/find/abuse/buttons 全绿；svelte-check 0/0、vitest 82/82。
- **P0-4 设置 UI v2（完成）**：后端注册表去除自然语言（label 字段删除，前端按 setting.<id> / settingGroup.<group> 解析语言包）；新建 RegistryPage.svelte（分组卡片/折叠/搜索/三级重置）、BackupSection.svelte（导出/导入/全部重置+确认）、parts/FontFamilyRow.svelte（字体行+导入管理）、
egistry-util.ts（路径读写纯函数 + 6 项单测）；SliderRow/ChoiceRow/ToggleRow 增 onReset 行级恢复；旧 GeneralTab/TypographyTab/ActionRow 删除；data-setting 契约迁移为完整设置项 id（smoke-settings/scroll/limits 选择器同步）；SettingsApp 全 i18n + 语言随配置即时切换（→setLocale）。验证：svelte-check 0/0、vitest **88/88**、cargo 261+15+2+5、smoke-settings **48/48**、新增 **smoke-settings-v2 10/10**（搜索/重置×3/导入导出按钮/语言下拉/折叠）、smoke-limits 6/6、smoke-scroll 4/4、smoke-buttons 32/32、smoke-titlebar 10/10、settings-io **21/21**；smoke-settings-io 与 smoke-settings-v2 均已并入 verify-all。
- **P0-8 磁盘占用与缓存清理（完成）**：src-tauri/src/resources.rs（disk_usage 六分项统计 + clear_scope 逐文件容错清理 + 单测 5 项）；命令 get_disk_usage/clear_cache（错误码 INVALID_SCOPE）；设置窗新增 DiskSection.svelte（分项/总量/三清理按钮+确认框，data-disk-item/data-disk-total 自动化契约）；smoke-disk **6/6** 并并入 verify-all（现 22 步）；红线复测：release 重建（NSIS **1.93MiB**），启动 5 次可见 max **631ms** / 就绪 median **763ms** PASS；内存基线抽查留待 P0-11 统一复测。Rust 288 全绿（266 lib+15+2+5）、vitest 88、svelte-check 0/0。
- **P0-9 快捷键扩展框架（完成）**：settings/shortcut_io.rs（导出生效绑定全表；导入强校验：未知动作/空值/超长/非对象/256KB 上限逐一报错；导入前 .import-bak 备份 + 失败回滚；整体替换语义；单测 4 项）；命令 export_shortcuts/import_shortcuts；Settings 快捷键页新增导入/导出按钮（原生对话框，data-setting=exportShortcuts/importShortcuts）+ 页内文案全量 i18n（shortcutRecorder.hint/recording/unset/resetOne/resetAll + shortcutIo.*）；scripts/check-shortcut-parity.mjs（Rust 默认表 ↔ 前端动作列表对齐）并入 verify-all（现 23 步）；smoke-settings-io 扩 E11–E13 → **24/24**；回归 shortcuts 30/30、settings-v2 10/10；cargo 292（270 lib+15+2+5）、svelte-check 0/0、vitest 88。
- **P0-10 数据目录迁移（完成）**：storage/migrate_dir.rs（复制宽容跳过被占用文件→文件数/字节数校验→写指针→原目录尽力清理，失败写 .cleanup.json 延迟到下次启动；失败回滚目标保留原目录；单测 8 项）；storage/paths.rs 指针读写 + DataDirOrigin::Persisted（解析顺序：运行时>SRT_DATA_DIR>指针>便携）；命令 migrate_data_dir/
estart_app + 错误码 MIGRATE_FAILED；设置窗新增 DataSection.svelte（当前目录/指针展示、目录选择→确认→结果→立即重启/稍后）；smoke-migrate **13/13**（副本程序目录便携运行：迁移→重启→persisted→数据完整→旧目录清理→restart_app 自我替换）并入 verify-all（现 **24 步**）；配置文档新增 §2.8（指针文件）；cargo **300**（278 lib+15+2+5）、svelte-check 0/0、vitest 88。
- **P0-5 主题系统 v2（完成）**：settings/theme.rs（6 套内置 include_str! + data/themes 用户主题；强校验；list/get/import/export/remove；单驻留缓存；系统明暗解析（HKCU 表））；
eader.json 主题升级为 id（v3 迁移 eye→paper-cream）+ themeAnimEnabled/themeAnimMs；命令 5 个（THEME_INVALID）；启动背景色改取解析后 base 令牌；前端 lib/theme.ts（令牌→CSS 变量、data-theme-base、过渡动画类、减少动态效果）+ main.ts 挂载前预载（防闪烁）+ ThemeMenu（工具栏）+ 查看菜单动态主题项 + 设置「阅读排版」主题行（导入/导出/删除，用户主题）；smoke-theme **10/10**（默认/六套/系统/重启预载/导入应用/删除/非法拒绝/内置删除拒绝/动画类/截图）并入 verify-all（现 **25 步**）；回归：settings 48/48、settings-v2 10/10、settings-io 24/24、titlebar 10/10、buttons 32/32；cargo **308**（286 lib+15+2+5）。
- **P0-6 背景图（完成）**：src-tauri/src/background.rs（png/jpg/jpeg/webp ≤10MB、单文件驻留、防穿越；6 单测）；
eader.rs BackgroundSettings/BackgroundFill + defaults；store 归一；registry 6 项（含负值范围校验 is_i64 改造）+ 负值范围测试；命令 set/clear/read_background_image（BACKGROUND_INVALID，base64 返回）；前端 App 工作区图层（data URL + 填充/重复/透明度/模糊/亮度，缺失优雅降级）+ 设置窗 BackgroundRow（选择/移除/恢复默认）+ i18n（中英 30+ 键）；**save_settings 命令统一广播设置变更**（后端 emit，设置窗 store 不再自广播）；smoke-bg **10/10** 并入 verify-all（现 **26 步**）；回归 settings 48/48、settings-v2 10/10、settings-io 24/24、theme 10/10；cargo **315**（293 lib+15+2+5）、vitest 88、svelte-check 0/0。
- **P0-7 欢迎页（完成）**：EmptyState 重制（品牌区/主按钮/最近打开≤3 条点击直达/生效快捷键提示行（取 shortcuts 绑定）/拖拽与编辑提示）；「新建文件·载入示例文本」按钮待 P3 功能落地后加入（不展示无效按钮）；App 传入 recentEntries/shortcuts；i18n 键更新（empty.tagline/recent/note，移除旧 title/hint/shortcutHint）；smoke-buttons +B2 → **33/33**；settings-io 24/24、bg 10/10 回归通过；svelte-check 0/0、vitest 88/88。
- **P0-11 验收（完成）**：全量自检 26 步首跑 29/31（fmt + T5 断言两个测试侧问题当场修复并复验，修复后重跑）；红线复测：NSIS **1.97MiB**、启动可见 max 666ms/就绪 median 792ms、内存 10×100MiB 打开后 82.8MiB/压力峰 106.9MiB（<120）；stress 11/11。**P0 全部完成**（P0-1…P0-10）。
- **发布形态补充（维护者要求，完成）**：①审计确认应用本体**零注册表写入、零持久化环境变量写入**（两处 `set_var` 仅进程内存；系统主题仅只读注册表）；②新增 `scripts/make-portable.mjs`（exe+WebView2Loader.dll+LICENSE+便携说明 → `S-Read-TXT_<版本>_<架构>-portable.zip`，实测 2.43MiB、解压即用、数据落解压目录、可整体搬迁）；③release.yml 三架构构建新增便携版打包与产物上传；汇总与发布说明改为「安装版+便携版」双形态（README 新增形态对照表与零注册表说明）；④安装版目录已含 WebView2Loader.dll，可整体复制使用。
- **验收修复（P0-11 续）**：①迁移校验竞态修复（改为「以复制记录为准」，migrate 13/13 两连跑稳定）；②smoke-settings S11c 偶发失败加固（重开设置窗后等待行渲染再点击），48/48。
- **P0-11 验收收官**：修复后最终全量自检 **31/31 通过（678.3s）**；红线：NSIS 1.97MiB、启动可见 max 666ms/就绪 median 792ms、内存打开后 82.8MiB/压力峰 106.9MiB（<120）；stress 11/11；**P0 全部完成，待维护者验收演示与推送授权**。
- **清理与文档（维护者要求）**：临时清理（temp\\opencode 本项目 srt-* 全清、5 个一次性探针删除、测试残留目录清理；`cargo clean --profile dev` 释放 6.1GiB；保留 release 与 portable-dist）；新增 `docs/plan/remaining-work.md`（未完成清单唯一入口：P1–P4/发布验收/技术债/不做清单）；README 补便携版打包命令与 scripts 说明。
- **P1-1 批量插入/序号（完成）**：编辑引擎 batch.rs（10 种格式/行首行尾/5 种范围/跳空行/模板变量/容量预检 20 万行/单撤销步；18 单测）+ IPC（preview/apply，BATCH_INVALID；状态层 2 测试）；前端编辑菜单「批量插入/序号…」+ 弹窗（预览 10 条、错误就地展示）；smoke-batch **16/16** 并入 verify-all（现 **27 步**）；回归 edit 12/12、find 19/19；cargo **334**（312 lib+15+2+5）、vitest 88、svelte-check 0/0。
- **P1-2 行操作套件（完成）**：设置节 v4（编辑器页签 11 项，提交 6cf6be3）；引擎 line_ops.rs 26 种操作（提交 e98a551，15 单测）；IPC（提交 faea1ab）；前端 LineOpsDialog（按操作族显隐参数/预览/错误就地展示）+ 编辑菜单入口（提交待落）；smoke-lineops **16/16** 并入 verify-all（现 **28 步**）；回归 smoke-batch 16/16、smoke-edit 12/12；cargo **328 lib** + 15 + 2 + 5、vitest 88、svelte-check 0/0。
- **P1-3 多光标与矩形选择（完成）**：设置节 v5（提交 fa4193e）；纯逻辑 `edit/multi.ts` + 8 单测（提交待落）；EditLayer 集成（修饰键单击加/移除光标、矩形拖选、多光标键入/退格/前删=单撤销步、矩形复制剪切、Esc/导航收起、上限保护、设置门控）+ ReaderView/App 透传；**修复阶段 4b 遗留缺陷：编辑区鼠标点击定位实际不可用（caretRangeFromPoint 命中交互层）→ elementsFromPoint+字符盒二分**；smoke-multi **18/18** 并入 verify-all（现 **29 步**）；回归 edit 12/12、lineops 16/16；cargo **329 lib** + 15 + 2 + 5、vitest **96**、svelte-check 0/0。简化取舍（已注释）：含换行粘贴退化为单点；多光标删除后落定后端光标（不保留列）。
- **P1-4 过滤视图（完成）**：后端 filter.rs（字面量/正则/大小写/隐藏空行；命中 5 万/扫描 200 万行截断；8 单测；提交 0fa98ad）+ fetch_rows_at 稀疏取行；前端阅读区「筛选」粘性条（入口仅阅读态、切换标签/进编辑自动清空、显示行↔文件行映射的稀疏虚拟化、无匹配提示、非法正则就地报错）；开发中修复 effect 依赖回流缺陷（refreshWindow→viewRowsTotal 读取 filterRows 导致应用后被标签 effect 清空 → 该 effect 体改 untrack 包裹）；smoke-filter **13/13** 并入 verify-all（现 **30 步**）；回归 edit 12/12、lineops 16/16、multi 18/18；svelte-check 0/0、vitest 96。
- **P1-5 剪贴板历史与复制格式（完成）**：设置节 v6（提交 12e33ca；editor.clipboard 两项 historyLimit/persist；registry +2）；后端 `clipboard_history.rs`（同文本去重留最新/上限/截断/持久化或会话内存；8 单测）+ 命令 list/add/remove/clear_clipboard_history；前端 EditLayer 复制/剪切自动记录、`ClipboardHistoryDialog`（插入/删除/清空确认/空态）、编辑菜单「剪贴板历史…」与「复制为」（纯文本/HTML/Markdown；HTML 走 writeHtml + 纯文本兜底）、capabilities +write-html；smoke-clipboard **19/19** 并入 verify-all（现 **31 步**）；回归 smoke-edit 12/12；cargo 346 lib + 15 + 2 + 5、vitest 96、svelte-check 0/0。调试记录：命令名曾误写 `add_clipboard_history`（实为 `add_clipboard_entry`）；`save_settings` 的 shortcuts 必须传 `snapshot.shortcuts.bindings` 扁平表（传整个对象报 "invalid type: map, expected a string"）。
- **P1-6 查找增强（完成）**：设置节 v7（find 9 项 + regex 2 项；注册表新类型 Color/StringList 与强校验；提交 03cc8bd）；引擎 SearchRequest（全词 `\b` 包裹/计数上限 20 万/扫描超时中断/until 严格上界）+ 查找历史存储（find-history.json 去重置顶）+ 命令 count_matches_in_edit 与 list/add/clear_find_history；前端 FindBar v2（W 开关/计数/历史下拉/范围选择+行区间 1 基）、EditLayer（范围边界解析与过滤、循环开关、高亮颜色 CSS 变量、highlightAll/matchCount 开关、历史记录/清空）、设置页 ColorRow/StringListRow 新控件（注册表颜色与字符串列表类目）；smoke-find 扩至 **27/27**（F20–F24：计数/全词/历史/行范围过滤/设置新控件）；回归 smoke-edit 12/12、smoke-settings-v2 10/10（首跑偶发 9/10，重跑 10/10）；cargo **357 lib** + 15 + 2 + 5、vitest 96、svelte-check 0/0。
- **P1-7 辅助编辑（完成）**：设置节 v8（insert/autoPairs/cleanup；SCHEMA_VERSION=8；registry +8 项、注册表总数 70；编辑器页新增 3 分组）；EditLayer：时间戳插入（5 格式）、自动补对（补对/包裹/跳过/空对退格）、回车自动缩进、括号配对高亮（跨显示行扫描限额 400 行 / 20 万字符、防抖 80ms、.bracket-match）、清理单项与一键（复用行操作引擎，逐项单撤销步）；MenuBar 编辑菜单新增「插入日期时间」与「清理」子菜单；E2E smoke-tools **16/16**（T1–T9）并入 verify-all（现 **38 步**）；**修复两处真实缺陷**：自动补对跳过路径 `moveRight` 实参顺序错误（静默抛错导致跳过光标不动、连带空对退格失效）、物理 `Enter` 走 keydown 分支导致自动缩进从未生效；同步陈旧断言：smoke-settings S1b 页签数 5→6、smoke-settings-io 迁移/导出版本断言 3→8（自 P1-2 起未同步）；回归 abuse 41/41、edit 12/12、clipboard 19/19、find 27/27；cargo **358 lib**（+15+2+5）、vitest 96、svelte-check 0/0。
- **P1-8 工作区查找与替换（完成）**：设置 v9（find.multifileEnabled 默认开 + multifileConcurrency 1–16 默认 4；registry 72 项；提交 382803d）；后端 workspace_scan（编辑态复用流式引擎/只读逐行匹配/命中行摘要 + 单文件 200 条与 20 万计数上限/超时保留）+ AppState（受限并发只读扫描/编辑态顺序；replace_workspace 逐文件单撤销步、只读跳过）+ 两命令与 MULTIFILE_DISABLED；前端 WorkspaceFindDialog（搜索/替换/结果树/点击跳转）+ jumpStore（跨组件定位广播：App 切标签 → ReaderView 滚动 → EditLayer 选区与焦点）+ 编辑菜单入口（开关关闭时禁用）；E2E smoke-workspace **18/18** 并入 verify-all（现 **39 步**）；开发中修复：runReplaceAll 的 busy 自锁导致重扫被拦、Dialog 内 describeIpcError 需先 toIpcError、EditLayer 无 tab prop（应用 tabId）、FALLBACK_FIND 缺新字段；cargo **362 lib**（+15+2+5）、vitest 96、svelte-check 0/0。
- **运维教训（P1-7/P1-8 间）**：一个早前用 WMI 分离启动的 verify-all 残留进程一直在后台（日志被缓冲为 0 字节），其 E2E 锁住 exe 导致后续多次构建失败（failed to remove file / 拒绝访问）；处置：按 PID 树杀自家链；**今后所有分离任务必须跟踪到终态（轮询/看门狗），构建前先检查 verify-all/smoke 链进程**。

### P2-1 状态栏 v2（P2-1a–e，完成）
- 后端：统计引擎 stats.rs（字素簇/码点/字节/词数，跨块安全，5000 万上限；decode_stream 修复 encoding_rs 容量语义，不丢字节）+ edit_stats/FileSession::document_stats + 命令 document_stats/selection_stats；EOL 检测 textfile/eol.rs（前 256KB，UTF-16 感知）+ TabInfo.eol；convert_eol 全文档转换（≤32MB，单撤销步；修复 decode_slice 绝对偏移越界崩溃 + 多字节编辑回归测试）；设置 v10（app.status 9 项；registry StringList 增加 allowed 白名单）。
- 前端：StatusBar v2（items 顺序渲染 8 种元素；行列内联跳转；换行菜单编辑态转换；未选择文案；只读徽标；修改标记）；App/ReaderView/EditLayer 接线（文档统计/选区统计 250ms 防抖/光标行列/顶部行）；修复两处 effect 竞态（标签刷新重触发清空、顶部行首帧上报被清）。
- 验证：cargo 378 lib + 15 + 2 + 6 + 5（共 406）；vitest 96；svelte-check 0/0；smoke-status 9/9（物理验证：真实启动/操作/截图 6 张）并入 verify-all（现 40 步）。
- 提交：ae9e1f3（refactor app.status）/ eb7b0e8（fix 换行崩溃）/ f3ee6f0（feat 状态栏 v2）/ 85251da（test smoke-status）/ edfc157（chore 截图）。

### P2-2 显示选项（P2-2a–d，完成）
- 设置 v11：`app.display` 9 项（行号/相对行号/当前行高亮/自动换行/标尺+位置 0–1000/缩进参考线/不可见字符白名单/滚动条标记开关，滚动条标记留 P2-2 后续）；registry 共 85；migrate v11；store 归一测试；设置页「显示选项」分组。
- 渲染（ReaderView）：行号 gutter（ch 宽度、不可选中）、相对行号（阅读参照首帧/编辑参照光标）、当前行高亮、标尺竖线、缩进参考线（每 indentWidth 列）、不可见字符（空格·/制表→/行尾␣/换行¶；均为 1:1 替换保映射，超 4000 字符行跳过）、自动换行关闭（横向滚动，layoutKey 含 wordWrap 触发重排）。
- 兼容修复（开发中实测发现并解决）：① Svelte 空白泄漏——行内元素间换行符进入 textContent 导致文本断言全面失败 → 行模板压为单行零空白；② `.txt` 曾加 position/z-index 盖住 edit-surface → 鼠标定位/修饰键全失效 → 改用 `display:inline`；③ refreshMatches 残留 `node.firstChild` → 搜索高亮丢失 → 改 textAt 跨节点换算；④ smoke-multi 测试侧 pointOf 同步改 `.txt` 文本节点。
- 验证：cargo 407；vitest 96；svelte-check 0/0；smoke-display 15/15 并入 verify-all（现 41 步）；回归 multi 18/18、find 27/27、edit 12/12、status 9/9、clipboard 19/19、abuse 41/41、i18n 27/27；截图 p2-display-{on,nowrap,edit}.png。
- 提交：feat（渲染）+ test（smoke-display/verify-all/multi 适配）+ chore（截图）+ docs（本台账）。
- 备注：V-08 折叠/V-09 大纲/V-10 面包屑归 P2-6；V-11 滚动条标记（搜索/书签/修改标记点）在本阶段末小节实施。
