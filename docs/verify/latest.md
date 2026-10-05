# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-05T10:57:34.940Z
- 提交：86a4b97（未提交变更 2 项）
- 结果：**48/50 通过**（另有 1 项排除），总耗时 3180.2s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 1.1 |
| 快捷键动作对齐 | ✅ | 0.1 |
| svelte-check | ✅ | 44.6 |
| vitest 单测 | ✅ | 7.7 |
| cargo fmt 检查 | ✅ | 1.9 |
| cargo test（全目标） | ✅ | 64.7 |
| Tauri 构建（debug） | ✅ | 77.4 |
| E2E 编辑（smoke-edit） | ✅（重跑通过） | 77.4 |
| E2E 基础（smoke） | ✅ | 9.1 |
| E2E 查找（smoke-find） | ✅ | 28.8 |
| E2E 批量序号（smoke-batch） | ✅ | 20.3 |
| E2E 行操作（smoke-lineops） | ✅（重跑通过） | 213.7 |
| E2E 过滤视图（smoke-filter） | ✅ | 16.1 |
| E2E 多光标（smoke-multi） | ✅ | 24.1 |
| E2E 剪贴板历史（smoke-clipboard） | ✅ | 18.8 |
| E2E 辅助编辑（smoke-tools） | ✅ | 21.8 |
| E2E 输入法（smoke-ime） | ✅ | 15.8 |
| E2E 多语言（smoke-i18n） | ✅ | 28.3 |
| E2E 工作区搜索（smoke-workspace） | ✅ | 14.8 |
| E2E 标题栏（smoke-titlebar） | ✅ | 175.7 |
| E2E 全按钮（smoke-buttons） | ✅ | 32.4 |
| E2E 设置窗口（smoke-settings） | ✅（重跑通过） | 164.1 |
| E2E 状态栏（smoke-status） | ✅（重跑通过） | 42.2 |
| E2E 快照（smoke-snapshots） | ✅ | 28.0 |
| E2E 新建/导出/打印（smoke-p32） | ✅ | 23.9 |
| E2E 命令行/单实例（smoke-cli） | ✅ | 25.2 |
| E2E 大纲（smoke-outline） | ✅ | 27.3 |
| E2E 阅读模式（smoke-reading） | ✅（重跑通过） | 136.0 |
| E2E 标注（smoke-annotations） | ✅（重跑通过） | 274.5 |
| E2E 显示选项（smoke-display） | ❌ | 143.1 |
| E2E 设置 I/O（smoke-settings-io） | ❌ | 247.5 |
| E2E 设置 v2（smoke-settings-v2） | ✅ | 78.9 |
| E2E 磁盘占用（smoke-disk） | ✅ | 28.9 |
| E2E 数据目录迁移（smoke-migrate） | ✅ | 67.9 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 33.8 |
| E2E 多标签（smoke-tabs） | ✅ | 22.4 |
| E2E 多窗口（smoke-windows） | ✅（重跑通过） | 68.8 |
| E2E 分屏（smoke-split） | ✅（重跑通过） | 223.4 |
| E2E 工具功能（smoke-utility） | ✅ | 29.8 |
| E2E 比较与合并（smoke-compare） | ✅ | 34.6 |
| E2E 历史记录（smoke-history） | ✅ | 24.2 |
| E2E 会话恢复（smoke-session） | ✅（重跑通过） | 102.3 |
| E2E 数据目录引导（smoke-datadir） | ✅（重跑通过） | 69.6 |
| E2E 对抗（smoke-abuse） | ✅（重跑通过） | 126.7 |
| E2E 滚动完整性（smoke-scroll） | ✅ | 79.1 |
| E2E 主题系统（smoke-theme） | ✅ | 26.5 |
| E2E 背景图（smoke-bg） | ✅ | 22.8 |
| E2E 双阈值（smoke-limits） | ✅ | 35.8 |
| 离线核查（offline-check） | ✅ | 40.4 |
| E2E 100MB 长行（smoke-longline） | ✅ | 57.4 |
| E2E 卸载清理（smoke-uninstall） | ⏭ 排除 | — |

## 排除项

- E2E 卸载清理（smoke-uninstall）：需提权运行安装包（UAC）；同意提权时可不带 --exclude 运行

## 失败详情

### E2E 显示选项（smoke-display）

```
[PASS] D0 样本打开且行已渲染
[PASS] D1 行号显示（首行 1） — 1
[PASS] D2 每个渲染行都有行号
[PASS] D3 相对行号（阅读态参照首行：1,1,2） — 1,1,2
[PASS] D4 阅读态当前行高亮（首行）
[PASS] D5 标尺出现且位置 200px — left: calc(var(--reading-pad-left) + 200px);
[PASS] D6 缩进参考线（6 空格 → 1 条 @4 列） — 1
[PASS] D7 空格·/制表→ 标记（长度不变） — "a·b→c··"
[PASS] D8 行尾空白单独着色片段 — "··"
[PASS] D9 换行标记 ¶（每个渲染行）
[PASS] D10 关闭换行（white-space: pre） — pre
[PASS] D11 进入编辑模式
[PASS] D12 编辑态当前行跟随光标（行 1） — 1
[PASS] D13 相对行号参照光标（1,2,1） — 1,2,1
[PASS] D14 全部关闭后清理干净
node:fs:1484
  return binding.rmSync(getValidatedPath(path), opts.maxRetries, opts.recursive, opts.retryDelay);
                 ^

Error: EPERM, Permission denied: \\?\C:\Users\Administrator\AppData\Local\Temp\srt-smoke-display-1791196361777 '\\?\C:\Users\Administrator\AppData\Local\Temp\srt-smoke-display-1791196361777'
    at rmSync (node:fs:1484:18)
    at file:///D:/administrator/Documents/project/S-Read-TXT/scripts/smoke-display.mjs:248:3 {
  errno: 1,
  code: 'EPERM',
  path: '\\\\?\\C:\\Users\\Administrator\\AppData\\Local\\Temp\\srt-smoke-display-1791196361777',
  syscall: 'rm'
}

Node.js v26.7.0
```

### E2E 设置 I/O（smoke-settings-io）

```
PASS  M1 settings.json 迁移到 v15  ← schemaVersion=15
PASS  M2 迁移前备份 .v1.bak（内容为 v1）
PASS  M3 用户值保留 + 新字段补默认  ← 33
PASS  M4 三文件版本升级 + 阅读/快捷键值保留
PASS  M5 三个文件均生成 .v1.bak
PASS  E1 导出落盘且结构完整
PASS  E2 范围篡改拒绝（含字段路径）  ← app.maxTabs：数值超出允许范围 1–2000（实际 9999）
PASS  E3 未知字段拒绝（含字段名）  ← 未知字段：app.bogus
PASS  E4 类型篡改拒绝（含字段路径）  ← reader.typography.fontSize：应为整数
PASS  E5 合法导入生效 + *.import-bak
PASS  E6 重置单项（app.maxTabs → 20）
PASS  E7 重置分组（字号 16 / 段间距 0）
PASS  E8 重置全部（快捷键覆盖清空、恢复默认）
PASS  E9 注册表完整（≥27 项、含 app.locale 枚举、id 唯一）  ← count=113
PASS  E10 未知设置项拒绝  ← 未知设置项：app.noSuchField
套件异常： 未发现 CDP 页面目标（应用未启动或调试端口未开）
```

