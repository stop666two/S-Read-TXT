# 全量自检报告（最新一次）

- 时间（UTC）：2026-10-04T05:29:27.028Z
- 提交：0a1fc76（未提交变更 12 项）
- 结果：**36/44 通过**，总耗时 2400.4s

| 步骤 | 结果 | 耗时(s) |
| --- | --- | --- |
| 编码自检 | ✅ | 0.2 |
| 快捷键动作对齐 | ✅ | 0.1 |
| svelte-check | ✅ | 9.7 |
| vitest 单测 | ✅ | 3.7 |
| cargo fmt 检查 | ✅ | 2.4 |
| cargo test（全目标） | ✅ | 145.7 |
| Tauri 构建（debug） | ✅ | 40.6 |
| E2E 编辑（smoke-edit） | ✅（重跑通过） | 52.8 |
| E2E 基础（smoke） | ✅ | 4.6 |
| E2E 查找（smoke-find） | ❌ | 47.1 |
| E2E 批量序号（smoke-batch） | ✅ | 15.7 |
| E2E 行操作（smoke-lineops） | ✅ | 5.8 |
| E2E 过滤视图（smoke-filter） | ✅ | 4.2 |
| E2E 多光标（smoke-multi） | ✅ | 9.3 |
| E2E 剪贴板历史（smoke-clipboard） | ❌ | 26.5 |
| E2E 辅助编辑（smoke-tools） | ✅ | 9.4 |
| E2E 输入法（smoke-ime） | ✅ | 2.8 |
| E2E 多语言（smoke-i18n） | ✅ | 20.3 |
| E2E 工作区搜索（smoke-workspace） | ✅ | 21.5 |
| E2E 标题栏（smoke-titlebar） | ✅ | 168.1 |
| E2E 全按钮（smoke-buttons） | ✅ | 28.3 |
| E2E 设置窗口（smoke-settings） | ✅ | 40.0 |
| E2E 状态栏（smoke-status） | ✅ | 5.9 |
| E2E 大纲（smoke-outline） | ✅ | 5.8 |
| E2E 阅读模式（smoke-reading） | ✅ | 17.4 |
| E2E 标注（smoke-annotations） | ✅ | 19.1 |
| E2E 显示选项（smoke-display） | ✅ | 3.6 |
| E2E 设置 I/O（smoke-settings-io） | ❌ | 14.8 |
| E2E 设置 v2（smoke-settings-v2） | ✅ | 7.0 |
| E2E 磁盘占用（smoke-disk） | ❌ | 95.3 |
| E2E 数据目录迁移（smoke-migrate） | ✅ | 32.2 |
| E2E 快捷键（smoke-shortcuts） | ✅ | 35.9 |
| E2E 多标签（smoke-tabs） | ✅ | 12.2 |
| E2E 历史记录（smoke-history） | ❌ | 54.8 |
| E2E 会话恢复（smoke-session） | ✅ | 23.7 |
| E2E 数据目录引导（smoke-datadir） | ✅ | 4.2 |
| E2E 卸载清理（smoke-uninstall） | ❌ | 1022.0 |
| E2E 对抗（smoke-abuse） | ✅ | 21.4 |
| E2E 滚动完整性（smoke-scroll） | ❌ | 190.4 |
| E2E 主题系统（smoke-theme） | ✅ | 11.2 |
| E2E 背景图（smoke-bg） | ✅ | 15.6 |
| E2E 双阈值（smoke-limits） | ✅ | 25.5 |
| 离线核查（offline-check） | ✅ | 21.5 |
| E2E 100MB 长行（smoke-longline） | ❌ | 101.9 |

## 失败详情

### E2E 查找（smoke-find）

```
PASS  F1 打开文件并进入编辑
PASS  F2 Ctrl+F 打开查找条（输入框聚焦）
PASS  F3 菜单打开替换行（两个输入框）
PASS  F4 替换命中 row0（脏态）  ← alpha REPLACED beta
PASS  F5 大小写不敏感命中并替换 row1  ← REPLACED here
PASS  F6 全部替换（1 处 + 提示）  ← REPLACED again
PASS  F7 截图已保存  ← D:\administrator\Documents\project\S-Read-TXT\docs\screenshots\phase4c-find.png
PASS  F8 三次撤销全部还原且干净
PASS  F9 大小写敏感：未找到提示  ←  已替换 1 处 | 未找到「NEEDLE」 
PASS  F10 关闭大小写后命中并选中  ← selection=1
PASS  F11 Esc 关闭查找条
PASS  F12 菜单撤销生效  ← q=alpha Q beta menu=true undone=true
PASS  F13 另存为（新文件 + 标签重定向）  ← {"ok":true,"name":"renamed.txt","path":"\\\\?\\C:\\Users\\Administrator\\AppData\\Local\\Temp\\srt-smoke-find\\renamed.txt"}
PASS  F14 脏态重新加载（确认丢弃）
PASS  F15 正则模式（ne+dle 命中 + 文档高亮）  ← match=3 sel=1
PASS  F16 无效正则提示  ←  已替换 1 处 | 未找到「NEEDLE」 | 已重新加载 | 正则表达式无效：regex parse error:
    (
    ^
error: unclosed group 
PASS  F17 预览剔除后仅替换勾选项  ← marks=6 label=替换已选 2 处
PASS  F18 单步撤销还原全部替换
PASS  F20 匹配计数显示 3 处  ← 3 处匹配
PASS  F21 全词开关生效（关闭后命中选中）  ← toast= 未找到「NEEDLE」 | 已重新加载 | 正则表达式无效：regex parse error:
    (
    ^
error: unclosed group | 已替换 2 处 
PASS  F22a 历史记录包含已查词  ← ["need","needle","ne+dle","NEEDLE"]
PASS  F22b 选取历史回填查询  ← need
FAIL  F22c 清空历史显示空态
PASS  F23a 行范围计数过滤为 1 处  ← 1 处匹配
PASS  F23b 范围内命中选中
PASS  F24 设置页颜色行与列表行可见  ← color=true list=true
PASS  F19 关闭查找条并清除高亮

查找/替换冒烟结果：26/27 通过
```

### E2E 剪贴板历史（smoke-clipboard）

```
✓ C1a 复制后历史非空
  ✓ C1b 历史文本与选区一致
  ✓ C2 历史写入 data/clipboard-history.json
  ✓ C2b 文件包含条目文本
  ✓ C3 弹窗条目可见
  ✓ C4a 弹窗已关闭
  ✓ C4b 插入后字节数增加
  ✓ C5 删除后空态
  ✓ C6a 直连添加两条
  ✓ C6 步骤：点击编辑菜单
  ✓ C6 步骤：点击剪贴板历史项
  ✓ C6 步骤：弹窗打开
  ✓ C7a 上限 0 时不记录
  ✓ C8a 复制为子菜单存在
  ✓ C8b 点击「HTML」成功
  ✓ C8c 剪贴板纯文本兜底与记录一致

剪贴板历史 E2E：16 通过 / 3 失败
  ✗ C6b 清空确认弹窗出现：null
  ✗ C6c 清空后后端为空：2
  ✗ C8d HTML 复制记录历史：3
失败项：
  - C6b 清空确认弹窗出现：null
  - C6c 清空后后端为空：2
  - C8d HTML 复制记录历史：3
```

### E2E 设置 I/O（smoke-settings-io）

```
FAIL  M1 settings.json 迁移到 v12  ← schemaVersion=14
PASS  M2 迁移前备份 .v1.bak（内容为 v1）
PASS  M3 用户值保留 + 新字段补默认  ← 33
FAIL  M4 三文件版本升级 + 阅读/快捷键值保留
PASS  M5 三个文件均生成 .v1.bak
FAIL  E1 导出落盘且结构完整
PASS  E2 范围篡改拒绝（含字段路径）  ← app.maxTabs：数值超出允许范围 1–2000（实际 9999）
PASS  E3 未知字段拒绝（含字段名）  ← 未知字段：app.bogus
PASS  E4 类型篡改拒绝（含字段路径）  ← reader.typography.fontSize：应为整数
PASS  E5 合法导入生效 + *.import-bak
PASS  E6 重置单项（app.maxTabs → 20）
PASS  E7 重置分组（字号 16 / 段间距 0）
PASS  E8 重置全部（快捷键覆盖清空、恢复默认）
PASS  E9 注册表完整（≥27 项、含 app.locale 枚举、id 唯一）  ← count=104
PASS  E10 未知设置项拒绝  ← 未知设置项：app.noSuchField
PASS  L1 重启后 <html lang> 随语言设置更新（en）
PASS  L2 英文界面（工具栏提示 Open file）  ← Open file (Ctrl+O)
PASS  L3 英文状态栏（No file open）
PASS  L4 英文标题栏（Close）
PASS  L5 英文菜单（含 Open，且无「打开」）  ← File Edit View Help Open file…Ctrl+O Reload Open recent ▸ History Ctrl+Shift+H S
PASS  L6 英文空状态按钮（Open file）
PASS  E11 快捷键导出（格式字段 + 15 项）  ← bytes=506 keys=15
PASS  E12 快捷键导入生效（openFile→Ctrl+Shift+O）  ← result=OK openFile=Ctrl+Shift+O
PASS  E13 未知动作拒绝且提示动作名  ← ERR:{"code":"SETTINGS_IMPORT","message":"未知快捷键动作：bogus"}

设置导入/导出/重置/迁移套件：通过 21/24
```

### E2E 磁盘占用（smoke-disk）

```
file:///D:/administrator/Documents/project/S-Read-TXT/scripts/lib/smoke-cdp.mjs:44
  throw new Error('未发现 CDP 页面目标（应用未启动或调试端口未开）');
        ^

Error: 未发现 CDP 页面目标（应用未启动或调试端口未开）
    at findTarget (file:///D:/administrator/Documents/project/S-Read-TXT/scripts/lib/smoke-cdp.mjs:44:9)
    at async file:///D:/administrator/Documents/project/S-Read-TXT/scripts/smoke-disk.mjs:87:35

Node.js v26.7.0
```

### E2E 历史记录（smoke-history）

```
PASS  H1a 工具栏「历史记录」打开面板
PASS  H1b 历史按最近打开倒序（zc 在前）  ← ["zc.txt","zb.txt","za.txt"]
PASS  H2 搜索过滤（仅显示匹配项）  ← ["zb.txt"]
H3_DIAG=[{"n":"zc.txt","row":0},{"n":"zb.txt","row":0},{"n":"za.txt","row":101}]
PASS  H3a 关闭标签回写阅读进度  ← {"lastRow":101,"lastPercent":16.833333333333332}
FAIL  H3b 从历史重开恢复阅读进度  ← scrollTop=null
PASS  H3c 打开后面板自动关闭且活动标签正确  ← active=za.txt
PASS  H4 单条删除生效  ← ["za.txt","zb.txt"]
PASS  H5 清空历史（二次确认）后为空  ← 暂无历史记录
PASS  H6a 「最近打开」子菜单显示最近条目  ← count=1
PASS  H6b 点击最近条目打开新标签
PASS  H7 Ctrl+Shift+H 打开历史面板
PASS  H8 长列表虚拟滚动（渲染数远小于总数）  ← total=60 rendered=24
PASS  H9 滚动后渲染窗口移动（首条变化）  ← seed-000.txt → seed-042.txt

历史记录冒烟：12/13 通过
失败项：H3b 从历史重开恢复阅读进度
```

### E2E 卸载清理（smoke-uninstall）

```
安装包：D:\administrator\Documents\project\S-Read-TXT\src-tauri\target\release\bundle\nsis\S-Read-TXT_0.0.1-beta_x64-setup.exe
FAIL  U2 安装目录存在  ← 未找到（候选：C:\Users\Administrator\AppData\Local\Programs\S-Read-TXT | C:\Users\Administrator\AppData\Local\S-Read-TXT | C:\Program Files\S-Read-TXT | C:\Program Files (x86)\S-Read-TXT）

卸载清理冒烟：0/1 通过（提前终止）
失败项：U2 安装目录存在
```

### E2E 滚动完整性（smoke-scroll）

```
! A 首败 ROUND 0: {"st":194262,"sh":1450122,"visible":23,"blank":23,"blanks":["6695","6696","6697","6698","6699"]}
FAIL  A 随机跳转  ← 失败 20/30
  ! B 首败 burst 0: {"st":18000,"sh":1450122,"visible":23,"blank":23,"blanks":["617","618","619","620","621"]}
FAIL  B 滚轮连发  ← 失败 5/5
  ! C 失败 round 0: landed=0.100 {"st":144950,"sh":1450122,"visible":23,"blank":23,"blanks":["4995","4996","4997","4998","4999"]}
  ! C 失败 round 1: landed=0.100 {"st":144950,"sh":1450122,"visible":23,"blank":23,"blanks":["4995","4996","4997","4998","4999"]}
  ! C 失败 round 2: landed=0.100 {"st":144950,"sh":1450122,"visible":23,"blank":23,"blanks":["4995","4996","4997","4998","4999"]}
FAIL  C 上下震荡  ← 失败 3/3
  ! D 失败 size=16: blank=21 {"rowReal":28.8,"sh":1450123}
FAIL  D 滑块连调  ← 失败 1/13

滚动完整性套件：通过 0/4
```

### E2E 100MB 长行（smoke-longline）

```
生成单行文件：104857600 字节（约 100MB，无换行）…
PASS  C1 打开 100MB 单行（显示分段行数精确）  ← rows=12800 期望=12800 编码=UTF-8
PASS  C2 首段文本恰为 8KB  ← len=8192
FAIL  C3 滚动中部渲染正常  ← {}
PASS  C4 状态栏显示编码  ← single-line.txt   第 5657 行· · 阅读 44%· 100.0 MB· UTF-8 · —·  
PASS  C5 进入编辑模式
PASS  C6 文档末输入变脏  ← byteLen=104857601
PASS  C7a 保存弹窗打开
PASS  C7b 保存后磁盘 = 原大小 + 1 且尾部 aX  ← {"size":104857601,"tail":"aaaX"}
PASS  C8 截图已保存  ← D:\administrator\Documents\project\S-Read-TXT\docs\screenshots\phase4c-longline.png

超长行验收结果：8/9 通过
```

