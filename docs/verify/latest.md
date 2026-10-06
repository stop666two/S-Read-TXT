# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-06T04:10:01.376Z
- 提交：be6954e（未提交变更 20 项）
- 结果：**51/51 通过**（另有 1 项排除），总耗时 1448.8s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 1.0 |
| 快捷键动作对齐 | ✅ | 0.1 |
| svelte-check | ✅ | 22.9 |
| vitest 单测 | ✅ | 5.1 |
| cargo fmt 检查 | ✅ | 1.2 |
| cargo test（全目标） | ✅ | 76.8 |
| Tauri 构建（debug） | ✅ | 34.4 |
| E2E 编辑（smoke-edit） | ✅（重跑通过） | 66.5 |
| E2E 基础（smoke） | ✅ | 15.9 |
| E2E 查找（smoke-find） | ✅ | 26.8 |
| E2E 批量序号（smoke-batch） | ✅ | 17.1 |
| E2E 行操作（smoke-lineops） | ✅ | 5.9 |
| E2E 过滤视图（smoke-filter） | ✅ | 4.2 |
| E2E 多光标（smoke-multi） | ✅ | 28.1 |
| E2E 剪贴板历史（smoke-clipboard） | ✅ | 23.6 |
| E2E 辅助编辑（smoke-tools） | ✅ | 28.6 |
| E2E 输入法（smoke-ime） | ✅ | 9.4 |
| E2E 多语言（smoke-i18n） | ✅ | 11.3 |
| E2E 工作区搜索（smoke-workspace） | ✅ | 4.9 |
| E2E 标题栏（smoke-titlebar） | ✅ | 146.3 |
| E2E 全按钮（smoke-buttons） | ✅ | 81.1 |
| E2E 设置窗口（smoke-settings） | ✅ | 40.1 |
| E2E 状态栏（smoke-status） | ✅ | 6.1 |
| E2E 快照（smoke-snapshots） | ✅（重跑通过） | 52.3 |
| E2E 新建/导出/打印（smoke-p32） | ✅ | 5.2 |
| E2E 命令行/单实例（smoke-cli） | ✅ | 2.3 |
| E2E 大纲（smoke-outline） | ✅ | 5.9 |
| E2E 阅读模式（smoke-reading） | ✅ | 17.6 |
| E2E 标注（smoke-annotations） | ✅ | 23.2 |
| E2E 显示选项（smoke-display） | ✅ | 12.0 |
| E2E 设置 I/O（smoke-settings-io） | ✅ | 16.0 |
| E2E 设置 v2（smoke-settings-v2） | ✅ | 14.6 |
| E2E 磁盘占用（smoke-disk） | ✅ | 9.2 |
| E2E 数据目录迁移（smoke-migrate） | ✅ | 33.5 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 34.5 |
| E2E 多标签（smoke-tabs） | ✅ | 15.0 |
| E2E 多窗口（smoke-windows） | ✅ | 29.0 |
| E2E 分屏（smoke-split） | ✅ | 49.5 |
| E2E 工具功能（smoke-utility） | ✅ | 27.0 |
| E2E 比较与合并（smoke-compare） | ✅ | 5.3 |
| E2E 历史记录（smoke-history） | ✅ | 7.2 |
| E2E 会话恢复（smoke-session） | ✅ | 23.0 |
| E2E 会话恢复内容（smoke-restore） | ✅ | 141.5 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 5.9 |
| E2E 对抗（smoke-abuse） | ✅ | 26.2 |
| E2E 滚动完整性（smoke-scroll） | ✅ | 77.8 |
| E2E 主题系统（smoke-theme） | ✅ | 30.0 |
| E2E 背景图（smoke-bg） | ✅ | 29.0 |
| E2E 双阈值（smoke-limits） | ✅ | 19.5 |
| 离线核查（offline-check） | ✅ | 46.2 |
| E2E 100MB 长行（smoke-longline） | ✅ | 33.4 |
| E2E 卸载清理（smoke-uninstall） | ⏭ 排除 | — |

## 排除项

- E2E 卸载清理（smoke-uninstall）：用户指定排除

