# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-03T05:33:12.144Z
- 提交：47c3ba0（未提交变更 8 项）
- 结果：**29/31 通过**，总耗时 1001.5s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 0.2 |
| 快捷键动作对齐 | ✅ | 0.1 |
| svelte-check | ✅ | 7.8 |
| vitest 单测 | ✅ | 3.2 |
| cargo fmt 检查 | ❌ | 0.9 |
| cargo test（全目标） | ✅ | 95.5 |
| Tauri 构建（debug） | ✅ | 29.9 |
| E2E 编辑（smoke-edit） | ✅ | 13.5 |
| E2E 查找（smoke-find） | ✅ | 13.3 |
| E2E 输入法（smoke-ime） | ✅ | 12.6 |
| E2E 多语言（smoke-i18n） | ✅ | 18.6 |
| E2E 标题栏（smoke-titlebar） | ✅ | 167.8 |
| E2E 全按钮（smoke-buttons） | ✅ | 49.3 |
| E2E 设置窗口（smoke-settings） | ✅ | 53.2 |
| E2E 设置 I/O（smoke-settings-io） | ✅（重跑通过） | 15.3 |
| E2E 设置 v2（smoke-settings-v2） | ✅ | 23.0 |
| E2E 磁盘占用（smoke-disk） | ✅ | 12.7 |
| E2E 数据目录迁移（smoke-migrate） | ✅（重跑通过） | 82.6 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 29.1 |
| E2E 多标签（smoke-tabs） | ❌ | 28.8 |
| E2E 历史记录（smoke-history） | ✅（重跑通过） | 23.2 |
| E2E 会话恢复（smoke-session） | ✅ | 11.6 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 4.5 |
| E2E 卸载清理（smoke-uninstall） | ✅ | 60.3 |
| E2E 对抗（smoke-abuse） | ✅ | 42.5 |
| E2E 滚动完整性（smoke-scroll） | ✅ | 47.6 |
| E2E 主题系统（smoke-theme） | ✅ | 21.1 |
| E2E 背景图（smoke-bg） | ✅（重跑通过） | 82.3 |
| E2E 双阈值（smoke-limits） | ✅ | 10.3 |
| 离线核查（offline-check） | ✅ | 22.9 |
| E2E 100MB 长行（smoke-longline） | ✅ | 17.8 |

## 失败详情

### cargo fmt 检查

```
Diff in \\?\D:\administrator\Documents\project\S-Read-TXT\src-tauri\src\commands.rs:265:
         settings_store::save_snapshot(&dir, &request)
             .map_err(|err| IpcError::new(CODE_CONFIG_SAVE, format!("保存配置失败：{err}")))?;
         log::info!(target: "sread::ipc", "配置已保存");
[31m-        let _ = app.emit(EVENT_SETTINGS_CHANGED, serde_json::json!({ "kind": "save" }));
[0m[32m+        let _ = app.emit(
[0m[32m+            EVENT_SETTINGS_CHANGED,
[0m[32m+            serde_json::json!({ "kind": "save" }),
[0m[32m+        );
[0m         Ok(settings_store::load_snapshot(&dir))
     })
 }
```

### E2E 多标签（smoke-tabs）

```
PASS  T1 三个标签按打开顺序排列  ← ["za.txt","zb.txt","zc.txt"]
PASS  T2 中键关闭标签  ← ["za.txt","zc.txt"]
PASS  T3 拖拽排序（zc 移至首位）  ← ["zc.txt","za.txt"]
PASS  T4a 右键菜单出现（关闭/关闭其他/关闭全部）
PASS  T4b 「关闭其他」仅保留当前标签  ← ["za.txt"]
FAIL  T5 「关闭全部」后回到空状态
PASS  T6 超出上限提示且不新增标签  ← count=2 toast=标签数量已达上限，请先关闭部分标签（上限可在设置中调整）。
PASS  T7a 标签栏出现横向溢出  ← {"x":1070,"y":124,"sw":1332,"cw":1100}
PASS  T7b 滚轮纵向转横向滚动  ← scrollLeft=232
PASS  T8 拖拽取消：顺序不变且状态清理  ← ["za.txt","zb.txt","extra-0.txt","extra-1.txt","extra-2.txt","extra-3.txt","extra-4.txt","extra-5.txt","extra-6.txt","extra-7.txt","extra-8.txt","extra-9.txt"] {"dragging":false,"drop":false}

多标签冒烟：9/10 通过
失败项：T5 「关闭全部」后回到空状态
```

