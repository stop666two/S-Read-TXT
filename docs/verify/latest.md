# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-03T09:16:43.714Z
- 提交：e7cbe53（未提交变更 22 项）
- 结果：**35/37 通过**，总耗时 882.3s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 0.2 |
| 快捷键动作对齐 | ✅ | 0.1 |
| svelte-check | ✅ | 20.5 |
| vitest 单测 | ✅ | 9.3 |
| cargo fmt 检查 | ✅ | 1.3 |
| cargo test（全目标） | ✅ | 23.0 |
| Tauri 构建（debug） | ✅ | 59.9 |
| E2E 编辑（smoke-edit） | ✅（重跑通过） | 54.2 |
| E2E 基础（smoke） | ✅（重跑通过） | 19.5 |
| E2E 查找（smoke-find） | ✅ | 22.4 |
| E2E 批量序号（smoke-batch） | ✅ | 17.3 |
| E2E 行操作（smoke-lineops） | ✅ | 13.2 |
| E2E 过滤视图（smoke-filter） | ✅ | 14.8 |
| E2E 多光标（smoke-multi） | ✅ | 21.3 |
| E2E 剪贴板历史（smoke-clipboard） | ✅ | 18.6 |
| E2E 输入法（smoke-ime） | ✅ | 2.8 |
| E2E 多语言（smoke-i18n） | ✅ | 11.3 |
| E2E 标题栏（smoke-titlebar） | ✅ | 165.3 |
| E2E 全按钮（smoke-buttons） | ✅ | 28.4 |
| E2E 设置窗口（smoke-settings） | ❌ | 79.3 |
| E2E 设置 I/O（smoke-settings-io） | ❌ | 16.7 |
| E2E 设置 v2（smoke-settings-v2） | ✅ | 7.6 |
| E2E 磁盘占用（smoke-disk） | ✅ | 4.8 |
| E2E 数据目录迁移（smoke-migrate） | ✅ | 17.9 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 24.2 |
| E2E 多标签（smoke-tabs） | ✅ | 4.4 |
| E2E 历史记录（smoke-history） | ✅ | 7.1 |
| E2E 会话恢复（smoke-session） | ✅ | 9.6 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 3.3 |
| E2E 卸载清理（smoke-uninstall） | ✅ | 20.9 |
| E2E 对抗（smoke-abuse） | ✅ | 21.2 |
| E2E 滚动完整性（smoke-scroll） | ✅ | 55.5 |
| E2E 主题系统（smoke-theme） | ✅ | 17.5 |
| E2E 背景图（smoke-bg） | ✅ | 17.2 |
| E2E 双阈值（smoke-limits） | ✅ | 15.9 |
| 离线核查（offline-check） | ✅ | 33.4 |
| E2E 100MB 长行（smoke-longline） | ✅ | 22.7 |

## 失败详情

### E2E 设置窗口（smoke-settings）

```
PASS  S1a 设置窗口打开且快捷键行完整
FAIL  S1b 设置窗口五个页签  ← count=6
PASS  S2a 录制后组合键更新为 Ctrl+Q  ← text=Ctrl+Q
PASS  S2b 保存提示出现  ← toast=快捷键已保存
PASS  S2c 覆盖表已落盘（仅含 closeTab）  ← {"closeTab":"Ctrl+Q"}
PASS  S3a 冲突提示出现  ← 该组合已被其他动作使用
PASS  S3b 冲突后仍在录制态
PASS  S4 保留键提示出现  ← Ctrl+1~9 是固定的标签跳转键，不可占用
PASS  S5 取消后恢复显示原绑定
PASS  S6a 单条恢复默认后组合键回到 Ctrl+W
PASS  S6b 覆盖表清空  ← {}
PASS  S7a 全部恢复默认生效
PASS  S7b 覆盖表为空  ← {}
PASS  S8a 主窗口两个标签
PASS  S8b 自定义 Ctrl+Q 关闭标签生效
PASS  S8c 旧键 Ctrl+W 已解绑（不再关闭）
PASS  S9 恢复默认后 Ctrl+W 重新生效
PASS  S10a 裸字母被拒绝并提示  ← 该组合会干扰正常输入，请配合 Ctrl / Shift / Alt 或改用功能键
PASS  S10b 拒绝后仍在录制态
PASS  S11a F5 单键录制成功
PASS  S11b 主窗口 F5 打开对话框
FAIL  S11c 打开文件单条恢复默认
PASS  S12a 重启后两个标签
PASS  S12b 重启后自定义 Ctrl+Q 仍生效
PASS  S13a 常规页签显示真实字段
PASS  S13b 切回快捷键页签行完整
PASS  S13c 字号修改实时应用到主窗口
PASS  S13d 下调字号后阅读区无需滚动即重排  ← h30=1753 h16=997
PASS  S14a 段间距实时生效
PASS  S14b 首行缩进实时生效（2 字 ×16px=32px）
PASS  S14c 文字对齐切换生效
PASS  S14d 平滑滚动开关可切换
PASS  S14e 平滑滚动恢复默认
PASS  S15a 导入字体按钮打开原生对话框
PASS  S15b 取消对话框后关闭
PASS  S15c 导入字体链路成功  ← consola.ttf
PASS  S15d 自定义字体出现在字体列表
PASS  S15e 自定义字体加载并应用
PASS  S15e2 FontFace 实际加载（read_font_data 生效）
PASS  S15f 删除字体弹出原生确认框
PASS  S15g 取消删除后字体保留
PASS  S15h 删除字体链路成功
PASS  S15i 删除后自定义字体列表为空  ← count=0
PASS  S16a 状态栏开关切换
PASS  S16b 状态栏开关还原
PASS  S16c 启动恢复会话开关切换
PASS  S16d 启动恢复会话开关还原
PASS  S16e 启动恢复窗口开关存在

设置窗口冒烟：46/48 通过
失败项：S1b 设置窗口五个页签；S11c 打开文件单条恢复默认
```

### E2E 设置 I/O（smoke-settings-io）

```
FAIL  M1 settings.json 迁移到 v3  ← schemaVersion=7
PASS  M2 迁移前备份 .v1.bak（内容为 v1）
PASS  M3 用户值保留 + 新字段补默认  ← 33
FAIL  M4 三文件版本升级 + 阅读/快捷键值保留
PASS  M5 三个文件均生成 .v1.bak
FAIL  E1 导出落盘且结构完整
PASS  E2 范围篡改拒绝（含字段路径）  ← app.maxTabs：数值超出允许范围 1–200（实际 9999）
PASS  E3 未知字段拒绝（含字段名）  ← 未知字段：app.bogus
PASS  E4 类型篡改拒绝（含字段路径）  ← reader.typography.fontSize：应为整数
PASS  E5 合法导入生效 + *.import-bak
PASS  E6 重置单项（app.maxTabs → 20）
PASS  E7 重置分组（字号 16 / 段间距 0）
PASS  E8 重置全部（快捷键覆盖清空、恢复默认）
PASS  E9 注册表完整（≥27 项、含 app.locale 枚举、id 唯一）  ← count=62
PASS  E10 未知设置项拒绝  ← 未知设置项：app.noSuchField
套件异常： Uncaught
```

