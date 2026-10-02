# S-Read-TXT 设计文档（定稿基线）

- 文档版本：1.0（定稿）
- 日期：2026-10-02
- 应用版本：0.0.1-beta
- 状态：已通过 33 项决策确认（第 1 轮 15 项 + 第 2 轮 7 项 + 第 3 轮 4 项 + 7 项补充确认），进入实施。变更须双方确认并升版本。

---

## 1. 概述与目标

S-Read-TXT 是一个 Windows 桌面 TXT 阅读器（可切换编辑），Tauri v2 + Rust 后端 + Svelte 5 前端，完全离线、便携存储、极简界面。

**硬性红线（不得牺牲）**

| 指标 | 要求 | 验收口径 |
|---|---|---|
| 内存 | <100MB；120MB 兜底（已授权） | 10 个标签只读阅读态；编辑态峰值单独列报 |
| 冷启动 | 完全启动到可阅读 <1s | 进程启动 → 前端 ready 可交互（日志打点） |
| 安装包 | <10MB | NSIS 安装器产物（不捆绑 WebView2） |

**功能范围**：阅读、多标签、历史记录、快捷键 + 已确认补充（排版设置、会话/窗口恢复、首启引导、编辑模式、查找替换、另存为）。除此之外不得新增功能。

---

## 2. 决策基线（D1–D33 摘要）

- D1 前端：Svelte 5 + Vite；D2 样式：UnoCSS（presetWind3）+ CSS 变量主题
- D3 历史入口：独立面板 + 菜单「最近打开」；D4 历史重开恢复上次阅读进度（历史条目记录位置，重建行索引后跳转）
- D5 快捷键：应用内全局（非系统级），可录制/冲突检测/恢复默认
- D6 编码：全量（UTF-8/16LE/16BE、GB18030、Big5、Shift_JIS、EUC-KR、Windows-1252 等）+ BOM 识别
- D7 启动恢复：窗口（尺寸/位置/最大化）+ 标签会话；D8 标签切换恢复滚动位置；D9 会话文件缺失 → 跳过 + Toast
- D10 程序目录不可写：启动弹窗说明 + 选择可写目录（会话级，下次启动再提示）
- D11 首启引导：轻量浮层，可跳过、可「不再显示」
- D12 排版设置：允许（字号/行距/字体/限宽/边距）；默认 = 微软雅黑 16px / 1.8 / 720px / 48px
- D13 注释：全部代码写详细注释（Rust `///`、TS JSDoc）；JSON 配置逐字段说明于 `docs/configuration.md`
- D14 测试报告：`docs/test-report.md`
- D15 内存兜底：120MB 口径已授权，报告须附原因与优化路径
- D16 工具链：windows-gnu（探针已实测通过：tauri 2.12.1 + dialog 插件编译链接成功）
- D17 编辑模式：默认只读，可切换；**自研分块编辑引擎**（见 §5，全项目最大风险项）
- D18 编辑能力：标准编辑集（定位/选区/输入/删除/撤销重做/剪贴板/全选/普通文本查找替换（大小写开关）/另存为）；不做正则、多光标、列选择
- D19 撤销历史上限：50MB 或 1000 步，先到先裁剪
- D20 保存编码：每次询问（默认选项为「保持原编码」）
- D21 未保存关闭：弹窗「保存 / 不保存 / 取消」；无修改直接关闭（微调原「关窗即退」）
- D22 备份：每文件本次运行首次保存前生成 .bak（仅一份，可设置关闭）
- D23 重复打开同一文件：激活已有标签
- D24 外部修改：切回标签/窗口聚焦时检测，Toast 询问「重新加载 / 保留当前」
- D25 菜单：文件 / 编辑 / 查看 / 帮助；D26 工具栏：完整组 9 键（打开｜保存｜编辑切换｜历史｜编码▾｜主题▾｜字号-＋｜全屏｜设置）
- D27 标签栏：紧凑矩形（Notepad++ 风，关闭按钮常显）+ 未保存脏标记；中键关闭、右键菜单、拖拽排序、上限 20（可设）
- D28 状态栏：左 = 文件名 + 阅读百分比；右 = 文件大小 + 编码（点击切换）；编辑态增加「行:列 / 编辑中」
- D29 空状态：引导式（打开按钮 + 拖拽提示 + 常用快捷键）；D30 Toast：右下角堆叠 3s；D31 设置窗口：640×520 五页签
- D32 构建/安装/测试由 AI 执行；测试样本由 AI 生成并明确标记；每阶段交付截图 + 日志 + 摘要
- D33 LICENSE：MIT，`Copyright (c) 2026 stop666two`；仓库地址为「待补充（占位）」

---

## 3. 总体架构

```
┌─────────────────────────── WebView2 (前端 Svelte 5) ───────────────────────────┐
│  App.svelte                                                                     │
│  ├─ MenuBar / ToolBar / TabBar / StatusBar                                      │
│  ├─ ReaderView（虚拟滚动渲染层，行级 DOM 复用 + 高度缓存）                        │
│  ├─ EditLayer（编辑态：光标/选区/IME 锚点输入框/查找替换条）                      │
│  ├─ Panels：HistoryPanel / SettingsWindow(独立窗口) / Onboarding / Dialog / Toast │
│  └─ lib/ipc.ts（invoke 类型化封装）                                              │
└───────────────────────────────┬────────────────────────────────────────────────┘
                     IPC（Tauri commands + events）
┌───────────────────────────────┴────────────────────────────────────────────────┐
│ Rust 核心                                                                        │
│  ├─ textfile：mmap → 编码检测 → 稀疏行索引 → 文档模型（只读索引 + 片表）           │
│  ├─ 编辑引擎：piece table / 撤销栈 / 查找替换 / 保存链（原子写+.bak+编码询问+冲突） │
│  ├─ storage：便携 data/ 目录、原子写、可写检测与目录回退                          │
│  ├─ settings / history / session：模型、读写、清理、剪枝                          │
│  └─ commands / state / window_state / logging / error                            │
└──────────────────────────────────────────────────────────────────────────────────┘
```

**数据流（关键路径）**

1. 打开：前端 `open_file` → 后端校验 → mmap → 后台线程建索引 → 返回 TabInfo → 前端渲染首屏（预取 2 屏）。
2. 滚动：虚拟层算可视行区间 → 批量 `get_rows(tab, start, count)` → 后端按块定位/解码可视窗 → 返回 UTF-8 文本。
3. 编辑：键盘/IME 事件 → 前端合并为批量 `apply_edits` → 后端在片表上应用（并入撤销栈）→ 返回受影响行 → 前端局部重渲染。
4. 保存：`save(tab, encoding?)` → 冲突检测（mtime/size）→ 询问编码 → 首次保存写 .bak → 临时文件 + rename 原子替换 → 清脏标记。

---

## 4. 阅读引擎

### 4.1 内存映射与稀疏索引
- `memmap2` 只读映射；**禁止整读文件**。
- 索引：每 512 行 1 个「行起始字节」检查点；内存 ≈ 行数/512 × 8B（10 万行 ≈ 1.6KB；100 万行 ≈ 16KB）。
- 行定位：检查点二分 + 至多 511 行线性重扫；不做逐行存储。
- 显示行 ≤ 8KB：无换行超长行按字符边界分块（UTF-8 续字节回退、UTF-16 代理项保护、GB18030/Big5/Shift_JIS/EUC-KR 前向宽度走查）。
- 索引在后台线程构建（每标签惰性：仅激活标签立即建索引），完成前前端显示「索引中…」。

### 4.2 编码
- 顺序：BOM → `chardetng` → UTF-8 快速校验 → 兜底 GB18030。
- 解码：`encoding_rs` 按可视窗解码；换行字节（0x0A）在 WHATWG 传统编码中不会落入多字节序列内部，可安全作为行边界。
- UTF-16：专用扫描器处理 0x0A00/0x000A 与 BOM 对齐。
- 手动切换编码 = 重建索引 + 重取可视窗。

### 4.3 显示行与超长行
- 显示行 = 逻辑行；逻辑行字节长度 >8KB 时长行分块（分块点对齐字符边界），防无换行大文件卡死。
- 百分比 = 可视窗结束偏移 ÷ 文件总长（编辑后按文档逻辑长度重算）。

---

## 5. 编辑引擎（最大风险项 D17–D22）

### 5.1 数据结构
- **原始内容**：保持 mmap 不动（只读，零复制）。
- **片表（piece table）**：`Piece = Original{off,len} | Added{off,len}`；插入/删除 = 拆分 + 缩短/新增。
- **新增缓冲**：仅编辑产生的 UTF-8 文本进内存；删除不回收，直到回收/裁剪。
- **行数元数据**：原始片段的行数由稀疏索引前缀和提供；新增片段插入时统计行数；用 Fenwick 树对片段行数做前缀和，行↔偏移映射 O(log n)。

### 5.2 编辑交互层
- 光标/选区：点击定位、拖选、Shift+方向键、Ctrl+A；光标以「行 + 行内字符偏移」建模。
- 输入：桌面输入法经**隐藏输入框锚点**（composition 事件 → 预览 → 提交 `apply_edits`），确保中文拼音输入可用；这是专项测试重点。
- 撤销/重做：后端操作栈（含选区恢复信息）；双上限 50MB / 1000 步，先到先裁剪。
- 剪贴板：系统剪贴板纯文本；大范围复制（>10MB）前提示确认。

### 5.3 查找与替换
- 普通文本 + 大小写敏感开关；全文件扫描（100MB 扫描为百毫秒级）。
- 查找下一个、替换、全部替换（单个撤销步）；不做正则。

### 5.4 保存链
- 编码：每次询问（默认保持原编码）；同编码保存 = 原始片段字节直拷（快），换编码 = 全量转码。
- 不可表示字符 → 提示「转为 UTF-8 保存 / 取消」。
- 首次保存前生成 `<name>.bak`（可设置关闭）；写入 = 临时文件 + 原子 rename。
- 与磁盘冲突（外部已修改）→ 弹窗「覆盖 / 另存为 / 取消」。
- 另存为 = 指定路径 + 编码选择。

### 5.5 内存模型
- 只读态：mmap + 索引 + 可视窗（微）。
- 编辑态增量：新增缓冲 + 撤销历史（双上限）+ 可视窗；与文件大小基本无关。
- 验收：红线按只读态测量；编辑态峰值单独报告。

---

## 6. IPC 接口（Tauri commands 概览）

文件/标签：`open_file(path)`｜`open_paths(paths)`｜`close_tab(id, force)`｜`get_tab_info(id)`｜`set_encoding(id, enc|null)`
阅读：`get_rows(id, start, count)`｜`get_position(id, row)`｜`reload_tab(id)`
编辑：`toggle_edit(id, on)`｜`apply_edits(id, ops)`｜`undo(id)`｜`redo(id)`｜`find(id, q, opts)`｜`replace(id, …)`｜`replace_all(id, …)`｜`save(id, encoding|null, force)`｜`save_as(id, path, encoding)`｜`is_dirty(id)`
历史：`get_history`｜`remove_history(path)`｜`clear_history`
设置：`get_settings`｜`save_settings(bundle)`｜`reset_shortcuts()`
会话：`get_session`｜`save_session(session)`；窗口状态随会话持久化（自定义实现，不用插件，避免写 AppData）
目录：`data_dir_status()`｜`pick_data_dir()`｜`set_data_dir_override(path)`
应用：`get_app_info()`
事件（后端→前端）：`data-dir-warning`（可选）

---

## 7. 数据与存储（便携模式）

程序目录结构：

```
S-Read-TXT.exe
data/
├─ settings.json    主配置（logLevel、maxFileSizeMB、maxTabs、history 上限与保留期、bak 开关等）
├─ reader.json      阅读排版子配置（theme、typography）
├─ shortcuts.json   快捷键绑定子配置
├─ session.json     会话（窗口状态 + 标签：路径/编码/滚动锚点）
├─ history.jsonl    历史（JSONL 追加；启动压缩去重；剪枝：条数 10000 / 天数 365）
└─ logs/app.log     结构化日志（JSON 行，轮转 5MB×3）
```

- 所有写入原子（临时文件 + rename）；JSON 为 UTF-8 无 BOM、LF。
- 程序目录不可写检测：启动写探测文件；不可写 → 弹窗 + 选择目录（`pick_data_dir`），仅会话生效。
- 环境变量（文档化于 README 与 docs/configuration.md）：`SRT_DATA_DIR`（数据目录覆盖）、`SRT_LOG_LEVEL`（日志级别覆盖）。

---

## 8. 界面规格

```
┌──────────────────────────────────────────────────────┐
│ 文件  编辑  查看  帮助                                 │ 菜单栏 30px
├──────────────────────────────────────────────────────┤
│ [打开][保存][编辑][历史][编码▾][主题▾][A-][A+][全屏][设置] │ 工具栏 36px
├──────────────────────────────────────────────────────┤
│ [书名1]│[书名2 •]│[书名3]│        ← 紧凑矩形标签 + 脏标记  │ 标签栏 32px
├──────────────────────────────────────────────────────┤
│                                                      │
│        阅读区（虚拟化，720px 限宽，16px/1.8）           │
│                                                      │
├──────────────────────────────────────────────────────┤
│ 文件名 · 12.3%                    2.4MB · GB18030 ▾   │ 状态栏 24px
└──────────────────────────────────────────────────────┘
```

**主题色板（暖色系 D）**（浅/深双态均满足 WCAG AA 对比）

| 令牌 | 浅色 | 深色 | 护眼 |
|---|---|---|---|
| 背景 bg | #FAF9F7 | #1E1E1E | #F5EFE0 |
| 表面 surface | #FFFFFF | #252526 | #FAF5E8 |
| 正文 text | #2B2B2B | #D4D4D4 | #33302A |
| 次要 secondary | #6B6B6B | #9A9A9A | #6E675A |
| 边框 border | #E5E1DA | #3A3A3A | #E3DCC9 |
| 强调 accent | #3B6EA5 | #6FA3D8 | #3B6EA5 |

跟随系统 = 监听系统明暗（浅↔深自动）。

**菜单**：文件（打开/最近打开▸/历史记录/退出）｜编辑（撤销/重做/剪切/复制/粘贴/全选/查找/替换/切换编辑模式；只读态置灰）｜查看（主题▸/全屏/字号±/重置字号）｜帮助（快捷键/关于）

**其他**：空状态引导（应用名 + 打开按钮 + 拖拽提示 + 快捷键提示）；Toast 右下堆叠；引导浮层 4 卡片；设置窗口 640×520 五页签（常规/阅读排版/快捷键/历史/关于）；关于页仅版本 + 仓库占位。

**a11y**：键盘全可达、可见焦点环、对比度 AA（WCAG 2.2）、ARIA 标注；无 emoji 图标（全内联 SVG）。

---

## 9. 默认快捷键（全部可改，可恢复默认）

| 动作 | 默认 | 动作 | 默认 |
|---|---|---|---|
| 打开文件 | Ctrl+O | 保存 | Ctrl+S（编辑态） |
| 切换编辑/只读 | Ctrl+E | 关闭标签 | Ctrl+W |
| 下一/上一标签 | Ctrl+Tab / Ctrl+Shift+Tab | 跳转标签（固定键） | Ctrl+1~9 |
| 向下/向上翻页 | PgDn / PgUp | 文首/文末 | Home / End（阅读态） |
| 全屏 | F11 | 查找 / 替换 | Ctrl+F / Ctrl+H（编辑态） |
| 另存为 | Ctrl+Shift+S（编辑态） | 历史面板 | Ctrl+Shift+H |

阅读态备用键：Space / Shift+Space 翻页（固定，不可配置）；编辑态不生效。编辑标准键（Ctrl+Z/Y/X/C/V/A、方向键等）固定不参与配置。

---

## 10. 错误处理与日志

- 统一错误类型 → 前端展示「错误编号 + 中文提示」，编号可回溯日志。
- 边界：空文件、1 字节文件、无换行、CRLF/CR/LF、超限（逐字提示「很抱歉，文件过大无法打开，可以在设置里面调整。」）、不可读、被占用、保存冲突、磁盘满。
- 日志：JSON 行（时间 RFC3339、级别 RFC 5424 命名、模块名、tab/请求 id），轮转 5MB×3；级别由 settings.logLevel / `SRT_LOG_LEVEL` 控制；不含敏感信息。

---

## 11. 性能策略

- 虚拟渲染：仅可视行 + 缓冲区 DOM 复用；行高缓存，测量校正。
- IPC 批量取行（~200 行/次）、预取 2 屏、单标签单请求在途（序号防乱序）。
- 惰性索引（仅激活标签）；打开大文件不阻塞 UI。
- release：`opt-level="s"`、LTO、`panic="abort"`、strip；NSIS 单目标；WebView2 = skip（完全离线）。
- 启动路径：页面极简 + 会话惰性恢复 + 首屏绘制优先。

---

## 12. 安全与隐私

- 无网络代码路径（不引入 HTTP 客户端；依赖树审查 + 运行时 netstat 采样入报告）。
- Tauri capabilities 最小权限；CSP 收敛（`default-src 'self'`；样式允许内联以满足动态 CSS 变量）。
- 数据不加密（已确认）；仅存本机程序目录；日志无敏感数据。
- 编辑写盘：原子写 + 首次 .bak；路径不做任何联网用途。

---

## 13. 测试策略（详见实施计划阶段 9）

- 单元：Rust（索引/编码/片表/撤销/存储/历史/设置）、TS（虚拟滚动数学/快捷键引擎/位置映射）。
- 集成：命令层（不开窗口测试文档模型与保存链）。
- E2E：通过 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port` 注入 CDP 驱动真实 UI。
- 压力：10×100MB 标签、100MB 翻页/滚动、反复开关标签、编码反复切换、大文件编辑（IME/撤销/查找替换）。
- 指标：内存（私有工作集合计：应用 + WebView2 子进程，多采样）、冷启动（日志打点）、安装包体积。
- 冒烟 / 功能 / 探测（空文件、超限文案逐字、编码清单、脏关闭、目录不可写流程等）。
- 报告：`docs/test-report.md`（测试项/方法/预期/实际/结论/问题与建议）。

---

## 14. 风险与预案

| # | 风险 | 等级 | 预案 |
|---|---|---|---|
| R1 | 自研编辑引擎（IME/选区/撤销） | 高 | 阶段 3/4 前置；标准集限界；专项测试；问题不过夜 |
| R2 | GNU 工具链兼容 | 中 | 探针已过；异常则局部修复，不可解则启用 MSVC 备选（用户安装） |
| R3 | WebView2 内存基线 | 中 | 120MB 兜底已授权；按需关闭 GPU 加速等优化路径入报告 |
| R4 | 无换行超长行 | 中 | 8KB 显示行分块，测试覆盖 |
| R5 | 10×100MB 启动索引压力 | 中 | 惰性索引 + 后台线程 |
| R6 | 编码误判 | 中 | 手动切换 + 保存每次询问 + 报告记录误判样本 |
| R7 | 保存编码不可表示字符 | 低 | 提示转 UTF-8 / 取消 |
| R8 | 首编时长（~12min） | 低 | 后台构建 + 轮询，增量构建秒级 |

---

## 15. 目录结构（项目）

```
S-Read-TXT/
├─ .gitattributes  .gitignore  LICENSE  README.md  CHANGELOG.md
├─ package.json  vite.config.ts  uno.config.ts  svelte.config.js  tsconfig.json  vitest.config.ts
├─ index.html
├─ docs/ (design/ plan/ configuration.md test-report.md screenshots/)
├─ scripts/ (make-icons.mjs check-encoding.mjs measure-memory.mjs git-hooks/pre-commit)
├─ src/ (Svelte 前端：lib/ipc.ts、lib/reader/、lib/state/、lib/shortcuts/、lib/components/、windows/settings/)
└─ src-tauri/ (Cargo.toml build.rs tauri.conf.json capabilities/ icons/ src/)
```

## 16. 依赖（锁定精确版本）

- Rust：tauri `2.12.1`、tauri-build `2.7.1`、tauri-plugin-dialog `2.8.1`、serde/serde_json、memmap2、memchr、encoding_rs、chardetng、thiserror、time、log（版本以阶段 0 查询结果写入 `Cargo.toml`，全部 `=x.y.z`）；dev：tempfile
- 前端：svelte `5.57.1`、vite `8.3.2`、@sveltejs/vite-plugin-svelte `7.3.1`、unocss `66.10.5`、typescript `5.9.3`、svelte-check `4.7.6`、vitest `5.0.3`（dev）、@tauri-apps/cli `2.12.1`（dev）；运行时仅 @tauri-apps/api `2.12.1`、@tauri-apps/plugin-dialog `2.8.1`
- 禁止：Electron、任何 HTTP/遥测依赖、图标库、UI 组件库
