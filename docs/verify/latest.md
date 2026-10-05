# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-05T06:01:07.407Z
- 提交：3ac8811（未提交变更 29 项）
- 结果：**48/48 通过**（另有 1 项排除），总耗时 1458.4s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 2.4 |
| 快捷键动作对齐 | ✅ | 0.1 |
| svelte-check | ✅ | 23.1 |
| vitest 单测 | ✅ | 5.4 |
| cargo fmt 检查 | ✅ | 2.1 |
| cargo test（全目标） | ✅ | 66.5 |
| Tauri 构建（debug） | ✅ | 90.3 |
| E2E 编辑（smoke-edit） | ✅ | 22.8 |
| E2E 基础（smoke） | ✅ | 8.2 |
| E2E 查找（smoke-find） | ✅ | 26.3 |
| E2E 批量序号（smoke-batch） | ✅ | 27.0 |
| E2E 行操作（smoke-lineops） | ✅ | 29.9 |
| E2E 过滤视图（smoke-filter） | ✅ | 20.9 |
| E2E 多光标（smoke-multi） | ✅ | 32.6 |
| E2E 剪贴板历史（smoke-clipboard） | ✅（重跑通过） | 50.4 |
| E2E 辅助编辑（smoke-tools） | ✅ | 17.7 |
| E2E 输入法（smoke-ime） | ✅ | 13.1 |
| E2E 多语言（smoke-i18n） | ✅ | 16.8 |
| E2E 工作区搜索（smoke-workspace） | ✅ | 21.2 |
| E2E 标题栏（smoke-titlebar） | ✅ | 167.8 |
| E2E 全按钮（smoke-buttons） | ✅（重跑通过） | 104.7 |
| E2E 设置窗口（smoke-settings） | ✅ | 39.7 |
| E2E 状态栏（smoke-status） | ✅ | 6.0 |
| E2E 快照（smoke-snapshots） | ✅ | 20.4 |
| E2E 新建/导出/打印（smoke-p32） | ✅ | 16.4 |
| E2E 命令行/单实例（smoke-cli） | ✅ | 9.9 |
| E2E 大纲（smoke-outline） | ✅ | 15.3 |
| E2E 阅读模式（smoke-reading） | ✅ | 28.5 |
| E2E 标注（smoke-annotations） | ✅ | 25.9 |
| E2E 显示选项（smoke-display） | ✅ | 3.6 |
| E2E 设置 I/O（smoke-settings-io） | ✅ | 7.6 |
| E2E 设置 v2（smoke-settings-v2） | ✅ | 7.4 |
| E2E 磁盘占用（smoke-disk） | ✅ | 4.9 |
| E2E 数据目录迁移（smoke-migrate） | ✅ | 41.5 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 39.2 |
| E2E 多标签（smoke-tabs） | ✅ | 19.0 |
| E2E 多窗口（smoke-windows） | ✅ | 32.3 |
| E2E 分屏（smoke-split） | ✅ | 43.9 |
| E2E 历史记录（smoke-history） | ✅ | 11.4 |
| E2E 会话恢复（smoke-session） | ✅ | 16.5 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 6.7 |
| E2E 对抗（smoke-abuse） | ✅ | 30.0 |
| E2E 滚动完整性（smoke-scroll） | ✅ | 79.8 |
| E2E 主题系统（smoke-theme） | ✅ | 25.4 |
| E2E 背景图（smoke-bg） | ✅ | 26.5 |
| E2E 双阈值（smoke-limits） | ✅ | 41.8 |
| 离线核查（offline-check） | ✅ | 41.4 |
| E2E 100MB 长行（smoke-longline） | ✅ | 67.9 |
| E2E 卸载清理（smoke-uninstall） | ⏭ 排除 | — |

## 排除项

- E2E 卸载清理（smoke-uninstall）：需提权运行安装包（UAC）；同意提权时可不带 --exclude 运行

