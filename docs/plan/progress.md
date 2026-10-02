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
  - **运行冒烟必须使用 `npm run tauri build -- --debug --no-bundle` 产物**：普通 `cargo build` 的 debug 产物按 Tauri dev 语义指向 `http://localhost:1420`（Vite 未启动则为空白页）。
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
