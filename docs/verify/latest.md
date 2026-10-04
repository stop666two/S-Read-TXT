# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-04T15:12:04.265Z
- 提交：da36866（未提交变更 23 项）
- 结果：**46/47 通过**，总耗时 2049.5s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 0.2 |
| 快捷键动作对齐 | ✅ | 0.1 |
| svelte-check | ✅ | 20.8 |
| vitest 单测 | ✅ | 5.0 |
| cargo fmt 检查 | ✅ | 1.9 |
| cargo test（全目标） | ✅ | 48.6 |
| Tauri 构建（debug） | ✅ | 27.0 |
| E2E 编辑（smoke-edit） | ✅ | 12.0 |
| E2E 基础（smoke） | ✅ | 4.9 |
| E2E 查找（smoke-find） | ✅ | 23.0 |
| E2E 批量序号（smoke-batch） | ✅ | 19.4 |
| E2E 行操作（smoke-lineops） | ✅ | 13.2 |
| E2E 过滤视图（smoke-filter） | ✅ | 10.4 |
| E2E 多光标（smoke-multi） | ✅ | 14.6 |
| E2E 剪贴板历史（smoke-clipboard） | ✅ | 17.2 |
| E2E 辅助编辑（smoke-tools） | ✅ | 29.6 |
| E2E 输入法（smoke-ime） | ✅ | 16.4 |
| E2E 多语言（smoke-i18n） | ✅ | 19.5 |
| E2E 工作区搜索（smoke-workspace） | ✅ | 13.2 |
| E2E 标题栏（smoke-titlebar） | ✅ | 162.4 |
| E2E 全按钮（smoke-buttons） | ✅ | 27.8 |
| E2E 设置窗口（smoke-settings） | ✅ | 39.6 |
| E2E 状态栏（smoke-status） | ✅ | 6.0 |
| E2E 快照（smoke-snapshots） | ✅ | 20.2 |
| E2E 新建/导出/打印（smoke-p32） | ✅ | 5.3 |
| E2E 命令行/单实例（smoke-cli） | ✅ | 2.6 |
| E2E 大纲（smoke-outline） | ✅ | 5.8 |
| E2E 阅读模式（smoke-reading） | ✅ | 20.6 |
| E2E 标注（smoke-annotations） | ✅ | 26.4 |
| E2E 显示选项（smoke-display） | ✅ | 14.4 |
| E2E 设置 I/O（smoke-settings-io） | ✅ | 7.4 |
| E2E 设置 v2（smoke-settings-v2） | ✅ | 7.1 |
| E2E 磁盘占用（smoke-disk） | ✅ | 8.2 |
| E2E 数据目录迁移（smoke-migrate） | ✅ | 18.0 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 23.8 |
| E2E 多标签（smoke-tabs） | ✅ | 4.8 |
| E2E 历史记录（smoke-history） | ✅ | 7.2 |
| E2E 会话恢复（smoke-session） | ✅ | 10.9 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 3.3 |
| E2E 卸载清理（smoke-uninstall） | ❌ | 1020.8 |
| E2E 对抗（smoke-abuse） | ✅ | 21.3 |
| E2E 滚动完整性（smoke-scroll） | ✅（重跑通过） | 128.8 |
| E2E 主题系统（smoke-theme） | ✅ | 11.3 |
| E2E 背景图（smoke-bg） | ✅ | 15.6 |
| E2E 双阈值（smoke-limits） | ✅ | 27.3 |
| 离线核查（offline-check） | ✅ | 38.2 |
| E2E 100MB 长行（smoke-longline） | ✅ | 67.4 |

## 失败详情

### E2E 卸载清理（smoke-uninstall）

```
安装包：D:\administrator\Documents\project\S-Read-TXT\src-tauri\target\release\bundle\nsis\S-Read-TXT_0.0.1-beta_x64-setup.exe
FAIL  U2 安装目录存在  ← 未找到（候选：C:\Users\Administrator\AppData\Local\Programs\S-Read-TXT | C:\Users\Administrator\AppData\Local\S-Read-TXT | C:\Program Files\S-Read-TXT | C:\Program Files (x86)\S-Read-TXT）

卸载清理冒烟：0/1 通过（提前终止）
失败项：U2 安装目录存在
```

