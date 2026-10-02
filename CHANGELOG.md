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
- 编辑交互层（阶段 4b）：编辑模式切换与脏标记、光标与选区（点击/拖选/Shift+方向键/Ctrl+A）、输入/删除/回车、中文输入法组合输入（隐藏锚点）、剪切/复制/粘贴、撤销/重做快捷键、保存流（编码询问/冲突弹窗/.bak 备份）、脏标签与窗口关闭三态确认
- 编辑态超长行显示分段：>8KB 逻辑行按 8KB 显示分段虚拟渲染（与只读视觉一致），段表随编辑增量维护、随撤销快照回滚；任意大小单行文件现在可正常进入编辑（原「单行 >64KB 拒绝编辑」限制移除）；`RowText` 载荷增加 `logicalRow` / `baseUtf16` 供前端跨段光标/选区映射，前端已适配（跨段光标/选区/复制、段内坐标↔逻辑坐标换算；编辑落点由后端 `caretRow`/`caretUtf16` 权威返回）
- 窗口与启动体验：消除调试控制台窗口、首帧渲染后显示（防冷启动空白）、窗口与任务栏图标、防白闪背景色、WebView2 用户数据目录便携化（data/webview）
- 查找与替换（编辑态）：Ctrl+F / Ctrl+H 查找条（大小写开关、查找下一个、替换、全部替换单撤销步、未找到提示、文末自动回绕一次）；编辑菜单补全（撤销/重做/剪贴板/全选/查找/替换）；文件菜单新增重新加载（脏态确认后丢弃）；另存为流程（目标路径选择 + 编码询问 + 标签重定向）
- 自定义标题栏：关闭原生窗口装饰，自绘「应用图标 + 文件名 - S-Read-TXT + 拖拽区（双击最大化/还原）+ 最小化/最大化/关闭」，随三套主题配色；任务栏/Alt-Tab 名称随活动文件同步
- 快捷键引擎与默认方案：应用内全局（15 个动作可自定义）；Ctrl+1~9 固定标签跳转；编辑态翻页/首尾让位给编辑器；弹窗打开时挂起；绑定热刷新（事件+聚焦双通道，无需重启）
- 设置窗口（独立、按需创建，不占用常驻内存）：五个页签框架；快捷键页签完整可用（15 个动作查看/录制改键/冲突与保留键检测/Esc 取消/单条与全部恢复默认/修改即存）
- 工具栏「设置」启用（打开设置窗口）
- 全量自检脚本 `scripts/verify-all.mjs`：一条命令串行跑完质量门禁（编码 / 类型检查 / 单测 / fmt / Rust 全测 / 构建 / 11 套 E2E）并落盘 `docs/verify/latest.md`
- 多语言测试覆盖：8 种语言内容与 4 种传统编码（Shift_JIS / EUC-KR / Big5 / windows-1252）的单元 + 端到端用例

### 变更

- 工具栏「历史记录」「设置」在对应功能落地前保持禁用（此前为“可点但无反应”的死按钮）
- 状态栏「编码」由装饰标签改为可点击切换菜单（对齐设计 D28）；编码下拉抽取为共享组件（工具栏/状态栏同一实现）
- 查看菜单「全屏」启用（点击切换；F11 同款）

### 修复

- Windows GNU 工具链构建失败：PATH 中旧版 `libgcc_s_seh-1.dll`（Tesseract-OCR）遮蔽 MSYS2 运行库，导致 `cc1.exe` 启动失败（`0xC0000139 STATUS_ENTRYPOINT_NOT_FOUND`）而 `windres` 报 `preprocessing failed.`；修复方式见 README「Windows 构建环境注意」
- Windows GNU 工具链下，链接 GUI 依赖的测试目标因缺少 Common-Controls v6 清单而加载旧版 comctl32（导入 `TaskDialogIndirect` 失败，`0xC0000139`）；通过 lib+bin 拆分（库 = 纯逻辑，测试不链接 GUI）规避，应用二进制不受影响
- 冷启动窗口可能在内容就绪前显示为空白：窗口默认隐藏，前端在真实首帧（双 rAF）后显示，并加 8 秒兜底；补齐 `core:window:allow-show`/`allow-set-focus` 权限（缺失时显示调用被静默拒绝）
- 编辑态光标/选区不渲染、Ctrl+A 后输入/删除失效：修复阅读区 DOM 注册表失步（改 data-row 实时查询）与全选哨兵越界（前端钳制 + 后端防御测试）
- 安全教训：清理残留进程时严禁按名称结束 `msedgewebview2`（系统共享运行时，影响其他应用）
- 只读模式 8KB 行内分块在 UTF-8 多字节字符处曾切进字符中间（长中文行在分块边界出现替换符）：`snap_row_boundary` 对齐判定修正为检查当前位置字节（而非前一字节），新增多字节超长行拼接无损回归测试；该缺陷对编辑态分段同样生效，已一并修复
- 打开无换行的超长行文件时卡死（界面冻结数分钟）：行索引每个 8KB 块都把「找下一个换行」扫到文件尾，退化为 O(n²)（100MB 单行 ≈ 数百 GB 扫描量）；改为在上限窗口 `[start, start+8KB+2)` 内有界查找（CRLF 跨窗语义保持不变），新增 16MB 单行线性构建防回归测试与窗口边界测试；100MB 单行文件全链路（打开/滚动/编辑/保存）实测秒级通过
- 「关闭」按钮失效（真实缺陷，全按钮审计发现）：Tauri JS 在 `onCloseRequested` 未被阻止时会调用内部销毁命令，而最小权限集缺少 `core:window:allow-destroy`，导致标题栏 ×、菜单「退出→不保存」、三态「不保存」均无法关闭窗口；补齐权限后恢复
- 菜单「粘贴」触发 WebView 剪贴板权限弹窗且读取失败：复制/剪切/粘贴改经官方 `tauri-plugin-clipboard-manager`（Rust 侧读写，无浏览器弹窗；依赖精确锁定）
- 「localhost 拒绝连接」空白页（已根治）：Tauri 以 `custom-protocol` feature 区分打包（内嵌资源）与开发（连 `http://localhost:1420`）模式，裸 `cargo build`/`cargo test` 未启用该 feature，产物被编译为开发语义；已在 `Cargo.toml` 常开该 feature，任何方式编译的 exe 均可独立运行
- 替换/全部替换后键盘焦点停留在查找条，`Ctrl+Z` 到不了编辑器：两个动作成功后归还编辑器焦点
