# 测试覆盖矩阵（证据索引）

- 维护规则：**任何新增命令 / 设置项 / 错误码 / 功能域必须在本表登记测试引用**；`node scripts/verify-all.mjs` 全绿为放行前提。
- 测试层级：Rust 单测（`cargo test`）｜前端单测（`vitest`）｜E2E（`scripts/smoke-*.mjs`，真实应用 + CDP）｜专项脚本（`stress` / `measure-startup` / `offline-check`）｜CI 门禁（`.github/workflows/ci.yml`）。
- 计数口径：Rust 300（278 lib + 15 对抗 + 2 助手 + 5 集成）；vitest 88；E2E 21 套 ≈353 项；verify-all 24 步。

## 1. E2E 套件清单（verify-all 串行执行）

| 套件 | 项数 | 覆盖域 |
|---|---|---|
| smoke-edit | 12 | 编辑基础：输入/撤销/保存/脏标记/编码弹窗 |
| smoke-find | 19 | 查找替换：大小写/正则/预览剔除/撤销/高亮/菜单 |
| smoke-ime | 7 | 输入法组合：preedit/候选/提交/取消/保存 |
| smoke-i18n | 27 | 8 语言渲染/查找/替换/编码往返/切换 |
| smoke-titlebar | 10 | 标题栏：拖拽/三键/双击/最小化/主题/齿轮入口 |
| smoke-buttons | 32 | 全按钮审计：工具栏/菜单/标签栏/状态栏/退出流 |
| smoke-settings | 48 | 设置窗口全控件：常规/排版/字体导入/开关/页签 |
| smoke-shortcuts | 30 | 快捷键：固定键/循环/翻页/全屏/录制/持久化/模态挂起 |
| smoke-tabs | 10 | 多标签：顺序/中键/拖拽/菜单/上限/溢出滚轮/拖拽取消 |
| smoke-history | 13 | 历史：面板/搜索/进度/删除/清空/最近打开/**长列表虚拟滚动** |
| smoke-session | 11 | 会话：窗口几何/标签/编码/滚动恢复/缺失跳过 |
| smoke-datadir | 15 | 数据目录：弹窗/重定向/日志落位/只读 ACL 修复助手 |
| smoke-uninstall | 6 | 卸载：静默安装/卸载/注册表/数据清理 |
| smoke-abuse | 41 | 对抗：空文件/换行族/BOM/连打/撤销狂按/冲突/长行 |
| smoke-scroll | 4 | 滚动完整性：跳转/滚轮/震荡/滑块联动 |
| smoke-limits | 6 | 双阈值：只读标记/编辑禁用/硬上限拒绝/设置滑块 |
| smoke-settings-io | 24 | 设置 I/O：迁移/导出/篡改拒绝/导入/重置/注册表/**语言持久化（重启英文 UI：工具栏/状态栏/标题栏/菜单/空状态）**/**快捷键导出·导入·未知动作拒绝** |
| smoke-settings-v2 | 10 | 设置 v2（P0-4）：搜索过滤/无匹配/清空恢复/单项重置/分组重置/全部重置/导入导出按钮原生对话框/语言下拉即时切换/分组折叠 |
| smoke-disk | 6 | 磁盘占用（P0-8）：分项与总量、设置卡片、清理日志/备份/WebView（UI+确认框）、未知范围拒绝 |
| smoke-migrate | 13 | 数据目录迁移（P0-10）：便携副本运行、迁移报告/指针、重启 persisted、数据完整、旧目录清理、restart_app 自我替换 |
| smoke-longline | 9 | 100MB 无换行：分段/滚动/编辑/保存字节级 |
| offline-check | 4 | 离线核查：依赖树（静态）+ 运行时零外联（动态） |

## 2. IPC 命令 × 证据（43 个）

| 命令 | 证据 |
|---|---|
| get_app_info / data_dir_status / set_data_dir | 所有套件启动链路；smoke-datadir D1–D4 |
| get_settings / save_settings | smoke-settings S1–S16；smoke-tabs T6；smoke-shortcuts K11/K13 |
| get_default_shortcuts | smoke-settings S2；快捷键恢复默认流程 |
| get_history / remove_history / clear_history / update_history_progress | smoke-history H1–H9；Rust history::store 8 项 |
| get_session / save_session | smoke-session T2–T5；Rust session::store 4 项 |
| open_file / get_rows / set_encoding / list_encodings | smoke-edit/find/ime/i18n/abuse/limits/scroll 全员；Rust textfile 全套 |
| list_tabs / close_tab / set_active_tab / reorder_tab | smoke-tabs T1–T8；smoke-shortcuts K 系列；Rust app_state 19 项 |
| open_settings / take_settings_tab | smoke-buttons D12；smoke-settings S9/S13；smoke-limits L6 |
| list_fonts / import_font / remove_font / read_font_data | smoke-settings S15a–S15g（含 FontFace 实际加载断言）；Rust fonts 7 项 |
| toggle_edit / apply_edits / undo_edit / redo_edit | smoke-edit；smoke-abuse（撤销/重做狂按）；smoke-find；Rust editing 84 项 |
| save_tab / save_tab_as / reload_tab | smoke-edit；smoke-find F12/F14；smoke-shortcuts K12 |
| find_in_edit / replace_in_edit / replace_all_in_edit | smoke-find F1–F14；smoke-i18n I5；Rust search 测试 |
| preview_replace_all_in_edit / apply_replace_all_in_edit / match_window_in_edit | smoke-find F15–F19；smoke-i18n 高亮 |
| export_settings / import_settings / reset_settings / get_settings_registry | **smoke-settings-io E1–E10 + L1–L4（语言持久化）**；**smoke-settings-v2 V1–V10（搜索/单项·分组·全部重置/导入导出按钮/语言下拉/折叠）**；Rust bundle 11 项 / reset 5 项 / registry 7 项 |

## 3. 错误码 × 证据（24 个）

| 错误码 | 证据（★=E2E 真实链路，其余为 Rust 单测） |
|---|---|
| FILE_NOT_FOUND | Rust textfile::session |
| FILE_TOO_LARGE | ★ smoke-limits L5（逐字提示）；Rust |
| MAX_TABS | ★ smoke-tabs T6；Rust app_state |
| TAB_NOT_FOUND | ★ smoke-shortcuts K10（无标签安全）；Rust |
| INVALID_ENCODING | Rust settings/ipc_error |
| IO / INTERNAL / CONFIG_SAVE / HISTORY_SAVE / SESSION_SAVE | Rust ipc_error 5 项（映射表全覆盖） |
| INVALID_POSITION | Rust editing（越界原子性） |
| FILE_CONFLICT | ★ smoke-abuse P（冲突弹窗三路径）；Rust save 冲突测试 |
| ENCODING_UNREPRESENTABLE | Rust save（Big5 原子失败无残留） |
| NOT_EDITING | Rust app_state |
| EDIT_DIRTY | ★ smoke-abuse M（三态关闭） |
| FILE_READ_ONLY | ★ smoke-limits L3/L4；Rust app_state |
| QUERY_TOO_BROAD | Rust editing::search（超限拒绝） |
| INVALID_REGEX | ★ smoke-find F16；Rust search |
| SEARCH_STALE | Rust search（预览过期拒绝） |
| FONT_UNSUPPORTED / FONT_TOO_LARGE / FONT_NOT_FOUND / FONT_INVALID_NAME | Rust fonts 7 项（四错误码全覆盖） |
| SETTINGS_EXPORT | Rust bundle（写入失败路径） |
| SETTINGS_IMPORT | ★ smoke-settings-io E2–E4（三类篡改具体报错） |
| SETTINGS_RESET | ★ smoke-settings-io E10（未知项拒绝） |

## 4. 设置面 × 证据（注册表 27 项）

| 分组 | 项数 | 证据 |
|---|---|---|
| app.basic（含 locale） | 7 | smoke-settings S1/S2/S13/S14；双阈值 ★ smoke-limits L6；locale → Rust 归一/变体测试 + smoke-settings-io E9 |
| app.history | 2 | smoke-settings S12（历史页字段） |
| app.startup | 2 | smoke-settings S16a–e |
| reader.basic（theme） | 1 | smoke-buttons C7；smoke-settings 主题切换 |
| reader.typography | 10 | smoke-settings S3–S8/S13/S14（字号/行高/限宽/边距/段距/缩进/对齐/平滑） |
| reader.statusBar | 4 | smoke-settings S16 |
| shortcuts.bindings | 1 | smoke-shortcuts K1–K13 |
| **完备性保障** | — | Rust `registry_covers_every_default_field`（字段↔注册表双向防漂移）+ `validate_value_boundary_cases`（整数拒浮点/文本超长/动作白名单/空值/枚举/类型） |

## 5. 已知缺口与后续

| 缺口 | 处置 |
|---|---|
| P0-4 设置 UI v2（**完成**：注册表驱动动态生成、分组卡片+折叠、全局搜索、单项/分组/全部恢复默认、导入/导出界面、语言下拉即时切换；data-setting 契约 = 完整设置项 id） | smoke-settings-v2 V1–V10；smoke-settings 48/48；smoke-limits L6 / smoke-scroll D 选择器迁移后全绿 |
| P0-3 i18n 前端（**已收口**：设置窗口经 P0-4 重写为 i18n 原生 + 语言下拉） | 见 P0-4 |
| 性能套件（stress / measure-startup / memory-report） | 手动执行（时长与负载原因不入 verify-all），结论入 docs/test-report.md |
| 覆盖率度量（llvm-cov / vitest coverage） | 待评估 GNU 工具链可行性；当前以「表面覆盖 + 本矩阵」为准 |
| 32 位 / ARM64 兼容 | CI `cargo check`（i686 / aarch64）覆盖 |

## 6. 变更记录

- 2026-10-03 P0-10：数据目录迁移（复制校验→写指针→延迟清理；`config.json` 指针优先级、`migrate_data_dir`/`restart_app` 命令、设置「数据位置」卡片；smoke-migrate 13/13 并入 verify-all）。
- 2026-10-03 P0-9：快捷键扩展框架（导出「生效绑定」JSON / 导入强校验（未知动作·空值·超长·超大）·导入前备份回滚·整体替换语义；设置窗快捷键页导入导出入口 + 录制页文案全量 i18n；`check-shortcut-parity.mjs` 双端动作对齐检查并入 verify-all；smoke-settings-io 扩至 24/24）。
- 2026-10-03 P0-8：磁盘占用与缓存清理（resources 模块：分项统计/范围清理/逐文件容错；`get_disk_usage`/`clear_cache` 命令 + INVALID_SCOPE；设置「常规」页磁盘卡片 + 三清理按钮；smoke-disk 6/6 并入 verify-all）；release 复测：NSIS 1.93MiB、启动可见 max 631ms/就绪 median 763ms PASS。
- 2026-10-03 P0-4：设置 UI v2（注册表驱动：分组卡片+折叠/全局搜索/三级恢复默认/导入导出界面/语言下拉；旧 GeneralTab/TypographyTab/ActionRow 删除；`data-setting` 契约迁移为完整 id）；注册表去除自然语言标签（前端按 `setting.<id>` 解析语言包）；新增 smoke-settings-v2（10 项）并纳入 verify-all；Rust 261 lib 全绿。
- 2026-10-03 P0-3c：文案抽取收口（App.svelte 全部运行时文案、ipc 固定错误、tabs/session 相关、EditLayer、Toast、快捷键动作名与录制提示）；新增 i18n 运行时纯模块（tests 环境可用）；smoke-settings-io 扩至 **21/21**（+L5 菜单、L6 空状态）；回归 edit/find/abuse/buttons 全绿。
- 2026-10-03 P0-3b：i18n 核心（`zh-CN` 类型源 + `en` 完备性约束 + `t()` 插值 + runes 即时切换）与外壳文案抽取（标题栏/工具栏/状态栏/空状态/拖拽遮罩/首启引导）；`setLocale` 于 `reloadSettings` 接线；vitest 82；smoke-settings-io 扩展 L1–L4（重启后英文 UI）。
- 2026-10-03 覆盖补测批次：新增 smoke-settings-io（15 项）、smoke-history 长列表（+2）、smoke-tabs 溢出/拖拽取消（+3）、smoke-settings 字体加载断言（+1）、offline-check 并入 verify-all；Rust 新增 7 项（注册表完备性/边界、bundle 边界 4 项、locale 变体）。
- 修复随本轮：S13a 过期断言（标签改名）、H8 种子数据 `encoding` 字段类型。
