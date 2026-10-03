# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-02T15:20:45.191Z
- 提交：f88c214（未提交变更 2 项）
- 结果：**19/21 通过**，总耗时 544.4s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 0.2 |
| svelte-check | ✅ | 7.6 |
| vitest 单测 | ✅ | 2.7 |
| cargo fmt 检查 | ❌ | 0.7 |
| cargo test（全目标） | ✅ | 58.0 |
| Tauri 构建（debug） | ✅ | 77.0 |
| E2E 编辑（smoke-edit） | ✅ | 31.9 |
| E2E 查找（smoke-find） | ✅ | 25.2 |
| E2E 输入法（smoke-ime） | ✅ | 11.1 |
| E2E 多语言（smoke-i18n） | ✅ | 20.1 |
| E2E 标题栏（smoke-titlebar） | ✅ | 11.3 |
| E2E 全按钮（smoke-buttons） | ✅ | 47.2 |
| E2E 设置窗口（smoke-settings） | ✅ | 40.6 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 37.2 |
| E2E 多标签（smoke-tabs） | ❌ | 48.7 |
| E2E 历史记录（smoke-history） | ✅ | 17.8 |
| E2E 会话恢复（smoke-session） | ✅ | 16.7 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 9.2 |
| E2E 卸载清理（smoke-uninstall） | ✅ | 31.9 |
| E2E 对抗（smoke-abuse） | ✅ | 31.7 |
| E2E 100MB 长行（smoke-longline） | ✅ | 17.5 |

## 失败详情

### cargo fmt 检查

```
Diff in \\?\D:\administrator\Documents\project\S-Read-TXT\src-tauri\src\commands.rs:808:
         tauri::WebviewUrl::App("settings.html".into()),
     )
     .title("设置 - S-Read-TXT")
[31m-        .inner_size(800.0, 620.0)
[0m[32m+    .inner_size(800.0, 620.0)
[0m     .resizable(false)
     .maximizable(false)
     .decorations(false)
```

### E2E 多标签（smoke-tabs）

```
node:fs:1484
  return binding.rmSync(getValidatedPath(path), opts.maxRetries, opts.recursive, opts.retryDelay);
                 ^

Error: EPERM, Permission denied: \\?\C:\Users\Administrator\AppData\Local\Temp\srt-smoke-tabs '\\?\C:\Users\Administrator\AppData\Local\Temp\srt-smoke-tabs'
    at rmSync (node:fs:1484:18)
    at main (file:///D:/administrator/Documents/project/S-Read-TXT/scripts/smoke-tabs.mjs:56:3)
    at file:///D:/administrator/Documents/project/S-Read-TXT/scripts/smoke-tabs.mjs:242:7
    at ModuleJob.run (node:internal/modules/esm/module_job:569:25)
    at async node:internal/modules/esm/loader:650:26
    at async asyncRunEntryPointWithESMLoader (node:internal/modules/run_main:101:5) {
  errno: 1,
  code: 'EPERM',
  path: '\\\\?\\C:\\Users\\Administrator\\AppData\\Local\\Temp\\srt-smoke-tabs',
  syscall: 'rm'
}

Node.js v26.7.0
```

