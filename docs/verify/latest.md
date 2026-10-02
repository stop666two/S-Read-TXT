# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-02T11:16:49.108Z
- 提交：8230e14（未提交变更 5 项）
- 结果：**19/20 通过**，总耗时 377.3s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 0.1 |
| svelte-check | ✅ | 6.9 |
| vitest 单测 | ✅ | 3.3 |
| cargo fmt 检查 | ✅ | 0.6 |
| cargo test（全目标） | ✅ | 65.0 |
| Tauri 构建（debug） | ✅ | 18.0 |
| E2E 编辑（smoke-edit） | ✅ | 10.5 |
| E2E 查找（smoke-find） | ✅ | 12.3 |
| E2E 输入法（smoke-ime） | ✅ | 8.4 |
| E2E 多语言（smoke-i18n） | ✅ | 18.6 |
| E2E 标题栏（smoke-titlebar） | ✅ | 12.6 |
| E2E 全按钮（smoke-buttons） | ✅ | 30.6 |
| E2E 设置窗口（smoke-settings） | ✅ | 39.7 |
| E2E 快捷键（smoke-shortcuts） | ❌ | 55.0 |
| E2E 多标签（smoke-tabs） | ✅ | 9.5 |
| E2E 历史记录（smoke-history） | ✅ | 12.2 |
| E2E 会话恢复（smoke-session） | ✅ | 12.6 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 3.3 |
| E2E 对抗（smoke-abuse） | ✅ | 25.7 |
| E2E 100MB 长行（smoke-longline） | ✅ | 32.5 |

## 失败详情

### E2E 快捷键（smoke-shortcuts）

```
PASS  K1 三个标签已打开  ← count=3
PASS  K2a Ctrl+2 跳到第二个标签  ← active=keys-second.txt
PASS  K2b Ctrl+3 跳到第三个标签  ← active=keys-third.txt
PASS  K3a Ctrl+Tab 循环到下一个  ← active=keys-main.txt
PASS  K3b Ctrl+Shift+Tab 循环到上一个  ← active=keys-third.txt
PASS  K4 Ctrl+W 关闭活动标签  ← count=2
PASS  K5a PgDn 向下翻页  ← 0→534
PASS  K5a2 PgUp 向上翻页  ← 534→0
PASS  K5b End 跳到结尾  ← top=11055 max=11056
PASS  K5c Home 跳到开头
PASS  K6a F11 进入全屏  ← isFullscreen=true
PASS  K6b F11 退出全屏  ← isFullscreen=false
PASS  K7a Ctrl+E 进入编辑模式
PASS  K7b Ctrl+F 打开查找条
PASS  K7c Esc 关闭查找条
PASS  K7d Ctrl+E 退出编辑模式
PASS  K8a Ctrl+O 弹出打开对话框
PASS  K8b 对话框已关闭
FAIL  K9 Ctrl+Shift+H 提示历史面板待提供
FAIL  K10a 全部标签已关闭  ← count=2
FAIL  K10b 无标签时按键无副作用且应用存活
FAIL  K11a 输入后进入脏态
PASS  K11b Ctrl+S 弹出保存（编码询问）弹窗
FAIL  K11c 取消保存后弹窗关闭且仍为脏态
FAIL  K12a Ctrl+Shift+S 弹出另存为对话框
FAIL  K12b 另存为对话框已关闭
PASS  K13a 脏标签 Ctrl+W 弹出三态弹窗
FAIL  K13b 弹窗打开时快捷键挂起（未关闭/未切换）
FAIL  K13c 取消后标签保留且仍脏
FAIL  K13d 不保存后标签关闭

快捷键冒烟：20/30 通过
失败项：K9 Ctrl+Shift+H 提示历史面板待提供；K10a 全部标签已关闭；K10b 无标签时按键无副作用且应用存活；K11a 输入后进入脏态；K11c 取消保存后弹窗关闭且仍为脏态；K12a Ctrl+Shift+S 弹出另存为对话框；K12b 另存为对话框已关闭；K13b 弹窗打开时快捷键挂起（未关闭/未切换）；K13c 取消后标签保留且仍脏；K13d 不保存后标签关闭
```

