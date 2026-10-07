# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-07T01:04:21.139Z
- 提交：8bb83ce（未提交变更 14 项）
- 结果：**56/56 通过**（另有 1 项排除），总耗时 1324.6s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 1.5 |
| 快捷键动作对齐 | ✅ | 0.1 |
| svelte-check | ✅ | 16.7 |
| vitest 单测 | ✅ | 5.8 |
| cargo fmt 检查 | ✅ | 1.4 |
| cargo test（全目标） | ✅ | 94.6 |
| Tauri 构建（debug） | ✅ | 33.9 |
| E2E 编辑（smoke-edit） | ✅（重跑通过） | 61.2 |
| E2E 基础（smoke） | ✅ | 25.0 |
| E2E 查找（smoke-find） | ✅ | 29.0 |
| E2E 批量序号（smoke-batch） | ✅ | 19.1 |
| E2E 行操作（smoke-lineops） | ✅ | 12.4 |
| E2E 过滤视图（smoke-filter） | ✅ | 9.7 |
| E2E 多光标（smoke-multi） | ✅ | 17.7 |
| E2E 剪贴板历史（smoke-clipboard） | ✅ | 15.4 |
| E2E 辅助编辑（smoke-tools） | ✅ | 17.4 |
| E2E 输入法（smoke-ime） | ✅ | 14.3 |
| E2E 多语言（smoke-i18n） | ✅ | 19.2 |
| E2E 工作区搜索（smoke-workspace） | ✅ | 9.6 |
| E2E 标题栏（smoke-titlebar） | ✅ | 153.5 |
| E2E 全按钮（smoke-buttons） | ✅ | 32.4 |
| E2E 设置窗口（smoke-settings） | ✅ | 43.5 |
| E2E 状态栏（smoke-status） | ✅ | 9.5 |
| E2E 快照（smoke-snapshots） | ✅ | 23.9 |
| E2E 新建/导出/打印（smoke-p32） | ✅ | 8.7 |
| E2E 命令行/单实例（smoke-cli） | ✅ | 5.8 |
| E2E 大纲（smoke-outline） | ✅ | 9.4 |
| E2E 阅读模式（smoke-reading） | ✅ | 21.1 |
| E2E 标注（smoke-annotations） | ✅ | 23.0 |
| E2E 显示选项（smoke-display） | ✅ | 7.0 |
| E2E 设置 I/O（smoke-settings-io） | ✅ | 11.1 |
| E2E 设置 v2（smoke-settings-v2） | ✅ | 13.7 |
| E2E 磁盘占用（smoke-disk） | ✅ | 11.8 |
| E2E 数据目录迁移（smoke-migrate） | ✅ | 14.5 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 27.4 |
| E2E 多标签（smoke-tabs） | ✅ | 14.4 |
| E2E 多窗口（smoke-windows） | ✅ | 24.0 |
| E2E 分屏（smoke-split） | ✅ | 32.4 |
| E2E 工具功能（smoke-utility） | ✅ | 17.1 |
| E2E 比较与合并（smoke-compare） | ✅ | 8.5 |
| E2E 历史记录（smoke-history） | ✅ | 10.7 |
| E2E 会话恢复（smoke-session） | ✅ | 13.5 |
| E2E 会话恢复内容（smoke-restore） | ✅ | 124.4 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 8.1 |
| E2E 对抗（smoke-abuse） | ✅ | 28.6 |
| E2E 滚动完整性（smoke-scroll） | ✅ | 67.7 |
| E2E 主题系统（smoke-theme） | ✅ | 17.2 |
| E2E 背景图（smoke-bg） | ✅ | 15.6 |
| E2E 双阈值（smoke-limits） | ✅ | 12.6 |
| E2E 命令面板（smoke-palette） | ✅ | 5.8 |
| E2E 限额可调（smoke-caps） | ✅ | 24.0 |
| E2E 可访问性（smoke-a11y） | ✅ | 7.2 |
| E2E 隐私清除（smoke-privacy） | ✅ | 6.6 |
| E2E 更新检查（smoke-update） | ✅ | 12.4 |
| 离线核查（offline-check） | ✅ | 21.3 |
| E2E 100MB 长行（smoke-longline） | ✅ | 32.0 |
| E2E 卸载清理（smoke-uninstall） | ⏭ 排除 | — |

## 排除项

- E2E 卸载清理（smoke-uninstall）：用户指定排除

