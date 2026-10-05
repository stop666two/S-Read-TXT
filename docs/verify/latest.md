# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-05T03:32:54.331Z
- 提交：05c46a0（未提交变更 34 项）
- 结果：**47/47 通过**（另有 1 项排除），总耗时 1063.1s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 0.3 |
| 快捷键动作对齐 | ✅ | 0.1 |
| svelte-check | ✅ | 11.5 |
| vitest 单测 | ✅ | 4.0 |
| cargo fmt 检查 | ✅ | 1.0 |
| cargo test（全目标） | ✅ | 113.9 |
| Tauri 构建（debug） | ✅ | 44.5 |
| E2E 编辑（smoke-edit） | ✅（重跑通过） | 56.0 |
| E2E 基础（smoke） | ✅ | 6.8 |
| E2E 查找（smoke-find） | ✅ | 27.2 |
| E2E 批量序号（smoke-batch） | ✅ | 23.5 |
| E2E 行操作（smoke-lineops） | ✅ | 25.1 |
| E2E 过滤视图（smoke-filter） | ✅ | 17.1 |
| E2E 多光标（smoke-multi） | ✅ | 23.8 |
| E2E 剪贴板历史（smoke-clipboard） | ✅ | 17.5 |
| E2E 辅助编辑（smoke-tools） | ✅ | 25.6 |
| E2E 输入法（smoke-ime） | ✅ | 6.0 |
| E2E 多语言（smoke-i18n） | ✅ | 7.9 |
| E2E 工作区搜索（smoke-workspace） | ✅ | 5.0 |
| E2E 标题栏（smoke-titlebar） | ✅ | 151.6 |
| E2E 全按钮（smoke-buttons） | ✅ | 29.1 |
| E2E 设置窗口（smoke-settings） | ✅ | 39.7 |
| E2E 状态栏（smoke-status） | ✅ | 6.0 |
| E2E 快照（smoke-snapshots） | ✅ | 23.8 |
| E2E 新建/导出/打印（smoke-p32） | ✅ | 5.1 |
| E2E 命令行/单实例（smoke-cli） | ✅ | 2.3 |
| E2E 大纲（smoke-outline） | ✅ | 5.8 |
| E2E 阅读模式（smoke-reading） | ✅ | 17.6 |
| E2E 标注（smoke-annotations） | ✅ | 19.3 |
| E2E 显示选项（smoke-display） | ✅ | 3.4 |
| E2E 设置 I/O（smoke-settings-io） | ✅ | 7.5 |
| E2E 设置 v2（smoke-settings-v2） | ✅ | 7.2 |
| E2E 磁盘占用（smoke-disk） | ✅ | 8.2 |
| E2E 数据目录迁移（smoke-migrate） | ✅ | 18.1 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 24.1 |
| E2E 多标签（smoke-tabs） | ✅ | 10.7 |
| E2E 多窗口（smoke-windows） | ✅ | 22.8 |
| E2E 历史记录（smoke-history） | ✅ | 7.3 |
| E2E 会话恢复（smoke-session） | ✅ | 9.1 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 3.4 |
| E2E 对抗（smoke-abuse） | ✅ | 21.5 |
| E2E 滚动完整性（smoke-scroll） | ✅ | 61.4 |
| E2E 主题系统（smoke-theme） | ✅ | 11.3 |
| E2E 背景图（smoke-bg） | ✅ | 15.5 |
| E2E 双阈值（smoke-limits） | ✅ | 27.5 |
| 离线核查（offline-check） | ✅ | 21.7 |
| E2E 100MB 长行（smoke-longline） | ✅ | 65.3 |
| E2E 卸载清理（smoke-uninstall） | ⏭ 排除 | — |

## 排除项

- E2E 卸载清理（smoke-uninstall）：需提权运行安装包（UAC）；同意提权时可不带 --exclude 运行

