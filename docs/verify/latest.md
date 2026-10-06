# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-06T14:32:32.147Z
- 提交：f98a6c3（未提交变更 15 项）
- 结果：**55/56 通过**（另有 1 项排除），总耗时 1688.7s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 0.3 |
| 快捷键动作对齐 | ✅ | 0.1 |
| svelte-check | ✅ | 28.9 |
| vitest 单测 | ✅ | 6.7 |
| cargo fmt 检查 | ✅ | 1.1 |
| cargo test（全目标） | ✅ | 298.8 |
| Tauri 构建（debug） | ✅ | 50.0 |
| E2E 编辑（smoke-edit） | ✅（重跑通过） | 64.8 |
| E2E 基础（smoke） | ✅ | 14.2 |
| E2E 查找（smoke-find） | ✅ | 38.6 |
| E2E 批量序号（smoke-batch） | ✅ | 27.7 |
| E2E 行操作（smoke-lineops） | ✅ | 9.5 |
| E2E 过滤视图（smoke-filter） | ✅ | 7.8 |
| E2E 多光标（smoke-multi） | ✅ | 12.9 |
| E2E 剪贴板历史（smoke-clipboard） | ✅ | 9.8 |
| E2E 辅助编辑（smoke-tools） | ✅ | 13.1 |
| E2E 输入法（smoke-ime） | ✅ | 6.4 |
| E2E 多语言（smoke-i18n） | ✅ | 25.7 |
| E2E 工作区搜索（smoke-workspace） | ✅ | 20.2 |
| E2E 标题栏（smoke-titlebar） | ✅ | 158.6 |
| E2E 全按钮（smoke-buttons） | ✅ | 66.1 |
| E2E 设置窗口（smoke-settings） | ✅ | 65.4 |
| E2E 状态栏（smoke-status） | ✅ | 9.5 |
| E2E 快照（smoke-snapshots） | ✅ | 23.9 |
| E2E 新建/导出/打印（smoke-p32） | ✅ | 8.7 |
| E2E 命令行/单实例（smoke-cli） | ✅ | 5.8 |
| E2E 大纲（smoke-outline） | ✅ | 9.4 |
| E2E 阅读模式（smoke-reading） | ✅ | 21.1 |
| E2E 标注（smoke-annotations） | ✅ | 22.9 |
| E2E 显示选项（smoke-display） | ✅ | 7.0 |
| E2E 设置 I/O（smoke-settings-io） | ✅ | 11.2 |
| E2E 设置 v2（smoke-settings-v2） | ✅ | 13.7 |
| E2E 磁盘占用（smoke-disk） | ✅ | 11.8 |
| E2E 数据目录迁移（smoke-migrate） | ✅ | 14.5 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 24.7 |
| E2E 多标签（smoke-tabs） | ✅ | 14.5 |
| E2E 多窗口（smoke-windows） | ✅ | 24.0 |
| E2E 分屏（smoke-split） | ✅ | 32.5 |
| E2E 工具功能（smoke-utility） | ✅ | 17.0 |
| E2E 比较与合并（smoke-compare） | ✅ | 11.9 |
| E2E 历史记录（smoke-history） | ✅ | 10.8 |
| E2E 会话恢复（smoke-session） | ✅ | 13.6 |
| E2E 会话恢复内容（smoke-restore） | ✅ | 130.9 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 4.6 |
| E2E 对抗（smoke-abuse） | ✅ | 27.1 |
| E2E 滚动完整性（smoke-scroll） | ✅ | 73.6 |
| E2E 主题系统（smoke-theme） | ✅ | 15.6 |
| E2E 背景图（smoke-bg） | ✅ | 26.7 |
| E2E 双阈值（smoke-limits） | ✅ | 26.6 |
| E2E 命令面板（smoke-palette） | ✅ | 11.5 |
| E2E 限额可调（smoke-caps） | ✅ | 43.5 |
| E2E 可访问性（smoke-a11y） | ✅ | 7.3 |
| E2E 隐私清除（smoke-privacy） | ✅ | 6.5 |
| E2E 更新检查（smoke-update） | ✅ | 12.2 |
| 离线核查（offline-check） | ❌ | 28.4 |
| E2E 100MB 长行（smoke-longline） | ✅ | 38.6 |
| E2E 卸载清理（smoke-uninstall） | ⏭ 排除 | — |

## 排除项

- E2E 卸载清理（smoke-uninstall）：用户指定排除

## 失败详情

### 离线核查（offline-check）

```
FAIL  A1 Windows 目标依赖树无 HTTP 客户端（cargo tree 实测）  ← 发现：ureq
PASS  A1b 锁文件全平台条目（信息性）  ← 锁文件含 reqwest, hyper, ureq（跨平台解析；未编译进 Windows 产物）
PASS  A2 前端依赖无 HTTP 客户端  ← 未发现
PASS  B1 运行时无对外网络连接（进程树 6 次采样）  ← 采样连接数 0（均为本机/无）

离线核查结果：3/4 通过
```

