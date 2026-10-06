# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-06T15:03:27.559Z
- 提交：2615ef0（未提交变更 9 项）
- 结果：**56/56 通过**（另有 1 项排除），总耗时 1472.0s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 0.4 |
| 快捷键动作对齐 | ✅ | 0.1 |
| svelte-check | ✅ | 22.2 |
| vitest 单测 | ✅ | 5.0 |
| cargo fmt 检查 | ✅ | 1.2 |
| cargo test（全目标） | ✅ | 81.9 |
| Tauri 构建（debug） | ✅ | 35.6 |
| E2E 编辑（smoke-edit） | ✅ | 29.6 |
| E2E 基础（smoke） | ✅ | 14.4 |
| E2E 查找（smoke-find） | ✅ | 18.9 |
| E2E 批量序号（smoke-batch） | ✅ | 31.6 |
| E2E 行操作（smoke-lineops） | ✅ | 17.1 |
| E2E 过滤视图（smoke-filter） | ✅ | 21.5 |
| E2E 多光标（smoke-multi） | ✅ | 19.4 |
| E2E 剪贴板历史（smoke-clipboard） | ✅ | 13.2 |
| E2E 辅助编辑（smoke-tools） | ✅ | 15.6 |
| E2E 输入法（smoke-ime） | ✅ | 7.6 |
| E2E 多语言（smoke-i18n） | ✅ | 11.7 |
| E2E 工作区搜索（smoke-workspace） | ✅ | 18.1 |
| E2E 标题栏（smoke-titlebar） | ✅ | 176.5 |
| E2E 全按钮（smoke-buttons） | ✅ | 32.3 |
| E2E 设置窗口（smoke-settings） | ✅ | 43.7 |
| E2E 状态栏（smoke-status） | ✅ | 9.5 |
| E2E 快照（smoke-snapshots） | ✅ | 23.9 |
| E2E 新建/导出/打印（smoke-p32） | ✅ | 8.7 |
| E2E 命令行/单实例（smoke-cli） | ✅ | 5.8 |
| E2E 大纲（smoke-outline） | ✅ | 9.3 |
| E2E 阅读模式（smoke-reading） | ✅ | 21.2 |
| E2E 标注（smoke-annotations） | ✅ | 22.9 |
| E2E 显示选项（smoke-display） | ✅ | 8.4 |
| E2E 设置 I/O（smoke-settings-io） | ✅ | 16.4 |
| E2E 设置 v2（smoke-settings-v2） | ✅ | 20.7 |
| E2E 磁盘占用（smoke-disk） | ✅ | 12.5 |
| E2E 数据目录迁移（smoke-migrate） | ✅ | 31.7 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 33.2 |
| E2E 多标签（smoke-tabs） | ✅ | 23.8 |
| E2E 多窗口（smoke-windows） | ✅ | 30.0 |
| E2E 分屏（smoke-split） | ✅ | 50.8 |
| E2E 工具功能（smoke-utility） | ✅ | 31.4 |
| E2E 比较与合并（smoke-compare） | ✅ | 26.7 |
| E2E 历史记录（smoke-history） | ✅ | 7.3 |
| E2E 会话恢复（smoke-session） | ✅ | 13.6 |
| E2E 会话恢复内容（smoke-restore） | ✅ | 126.3 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 4.4 |
| E2E 对抗（smoke-abuse） | ✅ | 30.0 |
| E2E 滚动完整性（smoke-scroll） | ✅ | 73.1 |
| E2E 主题系统（smoke-theme） | ✅ | 19.0 |
| E2E 背景图（smoke-bg） | ✅ | 12.2 |
| E2E 双阈值（smoke-limits） | ✅ | 16.0 |
| E2E 命令面板（smoke-palette） | ✅ | 5.9 |
| E2E 限额可调（smoke-caps） | ✅ | 37.1 |
| E2E 可访问性（smoke-a11y） | ✅ | 17.3 |
| E2E 隐私清除（smoke-privacy） | ✅ | 10.2 |
| E2E 更新检查（smoke-update） | ✅ | 21.6 |
| 离线核查（offline-check） | ✅ | 40.5 |
| E2E 100MB 长行（smoke-longline） | ✅ | 33.1 |
| E2E 卸载清理（smoke-uninstall） | ⏭ 排除 | — |

## 排除项

- E2E 卸载清理（smoke-uninstall）：用户指定排除

