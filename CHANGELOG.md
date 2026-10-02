# 变更日志

本文件遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 与 [语义化版本 2.0.0](https://semver.org/lang/zh-CN/)。

## [未发布]

### 新增

- 工程初始化：Tauri v2 + Svelte 5 脚手架、便携数据目录保护（`.gitignore`/pre-commit 钩子）、MIT 许可证、设计文档与实施计划
- 构建与验证工具链：编码自检（UTF-8 无 BOM / LF）、图标生成、CDP 冒烟与截图脚本
- Rust 核心模块与 IPC（阶段 1）：便携存储（原子写/JSON 容错）、三类配置（settings/reader/shortcuts）、日志（JSON 行 + 5MB×3 轮转 + 请求链路上下文）、历史（JSONL 去重/剪枝）、会话（窗口 + 标签锚点）、文本读取引擎（mmap + 编码检测 + 稀疏行索引 + 按需解码，禁止整读）；对应命令 get_app_info / data_dir_status / get_settings / save_settings / get_history / remove_history / clear_history / get_session / save_session
- 阅读界面（阶段 2）：打开文件（对话框多选 / 拖拽到窗口）、多标签打开/切换/关闭（重复打开复用标签、数量上限）、编码自动检测与手动切换（8 种编码，工具栏下拉）、虚拟滚动阅读（仅渲染可视行 + 实测行高缓存 + 滚动锚定，2 万行文件渲染 53 行）、状态栏（文件名 / 阅读百分比 / 大小 / 编码）、四主题（浅色/深色/护眼/跟随系统）、空状态与 Toast 提示、退出；新增命令 open_file / get_rows / set_encoding / list_encodings / list_tabs / close_tab
- 编辑引擎后端（阶段 3，为编辑模式打底）：片表（原文零复制 + 只增新增缓冲）、双 Fenwick 行映射（512 行检查点外的新增路径，行↔位置 O(log n)）、跨片 CRLF 安全计数、编辑应用（插入/删除/替换，UTF-16 位置解析与代理对处理，批次 = 单撤销步）、撤销/重做（50MB/1000 步双上限交换式快照）、保存链（外部冲突检测与强制覆盖、`.bak` 首存备份、BOM 策略、同编码字节直拷与异编码流式转码、不可表示字符原子失败、临时文件 + rename 原子写）；编辑守卫：单行 >64KB 拒绝进入编辑（读取不受影响）
- 持续集成与发布流水线：CI（任意分支 push / PR：仓库卫生检查含 AGENTS.md 防泄露、前端检查测试构建、Rust 全量测试与完整构建、32 位与 ARM64 兼容检查）；Release（仅 tag 触发：三架构 x64/x86/ARM64 NSIS 构建，描述含测试摘要/构建环境/变更明细/SHA256/已知问题与备份提醒，Releases 仅保留最新一个、旧 Release 自动清理、tag 永久保留）
- 编辑交互后端接线（阶段 4a）：标签编辑文档生命周期（`toggle_edit` 首次进入创建并保留，脏态跨模式持续）、编辑命令（`apply_edits` / `undo_edit` / `redo_edit` / `save_tab` / `save_tab_as` / `reload_tab`）、另存为标签重定向（路径/会话/编辑文档重建 + 写入历史）、`get_rows` 编辑态供数切换、编辑态阅读百分比、脏态阻止编码切换与重载、新增错误码（EDIT_LINE_TOO_LONG / INVALID_POSITION / FILE_CONFLICT / ENCODING_UNREPRESENTABLE / NOT_EDITING / EDIT_DIRTY）

### 修复

- Windows GNU 工具链构建失败：PATH 中旧版 `libgcc_s_seh-1.dll`（Tesseract-OCR）遮蔽 MSYS2 运行库，导致 `cc1.exe` 启动失败（`0xC0000139 STATUS_ENTRYPOINT_NOT_FOUND`）而 `windres` 报 `preprocessing failed.`；修复方式见 README「Windows 构建环境注意」
- Windows GNU 工具链下，链接 GUI 依赖的测试目标因缺少 Common-Controls v6 清单而加载旧版 comctl32（导入 `TaskDialogIndirect` 失败，`0xC0000139`）；通过 lib+bin 拆分（库 = 纯逻辑，测试不链接 GUI）规避，应用二进制不受影响
