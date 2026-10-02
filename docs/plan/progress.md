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
