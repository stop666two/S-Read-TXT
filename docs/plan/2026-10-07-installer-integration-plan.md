# 安装器/卸载器与系统集成 实施计划（2026-10-07）

> 用户已授权的六项裁定（原话见会话记录 m03932）：①用户级安装+按需提权 ②应用内开关、安装器零注册 ③HKCU 每用户 + 可选全局按钮 ④右键仅 .txt/.log ⑤卸载严格清理 ⑥自动化+真实 UAC 验收。

# Goal
使安装器「打开零 UAC、装后零注册表」；关联/右键/打开方式改为应用内可选开关（HKCU 免管理员，全局注册按需 UAC）；卸载彻底清理全部痕迹；便携版权限行为实测验证；文档与自动化证据齐全。

# Architecture
- 安装器：`installMode: currentUser`（模板 `RequestExecutionLevel user`，双击/安装零 UAC，默认装 `%LOCALAPPDATA%\...`）；目录自选受保护路径时由 `installer-hooks.nsh` 的 PREINSTALL 检测可写性并 `ExecShell runas` 自提权重入（权限只作用于该目录）；PREUNINSTALL 做同样的删除权限处理；POSTUNINSTALL 严格清理注册表。
- 集成引擎：新 `src-tauri/src/shell_integration.rs`——键路径构造为纯函数（单测）；HKCU 直写（免管理员）；HKLM（全局按钮）走 `--integration-write machine <flags>` 提权助手 argv 模式（复用 elevation.rs 的 runas+等待+校验套路，助手不进入 Tauri/WebView2）。
- UI：设置→系统页新增「系统集成」分组（`IntegrationSection.svelte`）：三开关（关联 .txt / 关联 .log / 右键菜单）+ 按钮（每用户应用/注销、全局应用/注销、刷新）+ 状态展示；全部带 `data-integration-*` 测试钩子。
- 卸载清理：POSTUNINSTALL 删除我们写过的全部键（含历史版本：旧 ProgID `Text Document`/`Log File` 仅当 shell\open\command 指向 `$INSTDIR` 才删；`*_backup` 值条件恢复后删除；OpenWithProgids 值；语言键；uninstall 键），幂等。
- 测试：Rust 单测（键表/还原决策/参数解析）+ `probe-integration`（真实驱动设置 UI 点击 + PowerShell 注册表断言）+ 扩展 `smoke-uninstall`（安装→CLI 注册→卸载→归零断言；installer.nsi 断言 `RequestExecutionLevel user` 且无 `APP_ASSOCIATE`）+ verify-all 并入 + 文档全套。

# Global Constraints
- 只写我们拥有的键（`SReadTXT.txt` / `SReadTXT.log` / `Applications\S-Read-TXT.exe` / `SystemFileAssociations\.txt|.log\shell\S-Read-TXT`）；不碰用户 UserChoice；默认值仅在指向我们时改写/还原。
- 幂等 + 可逆；拒绝 UAC 不阻断（返回需授权提示）。
- 错误信息中文、经 i18n（新错误码进 ipc_error.rs + ipc-error.ts 双端映射）。
- 默认（无操作时）：注册表零残留（安装器除卸载信息与快捷方式外不写任何键）。
- 每次任务：fmt / cargo test / svelte-check 绿后提交；验收含"真实运行"证据（真实安装/卸载 + 注册表现状输出）。

---
### Task 1 安装器零 UAC（配置 + 钩子）
**Files:** `src-tauri/tauri.conf.json`（installMode→currentUser；删 fileAssociations）、`src-tauri/nsis/installer-hooks.nsh`（新增 PREINSTALL/PREUNINSTALL 提权逻辑）。
**步骤**
1. 配置修改后 `npm run tauri build -- --debug --no-bundle` 快速验证：生成 `target/debug/nsis/x64/installer.nsi` 断言 `RequestExecutionLevel user`、无 `APP_ASSOCIATE`、`!define INSTALLMODE "currentUser"`。
2. 钩子（关键代码，直接落地）：
```nsh
!macro NSIS_HOOK_PREINSTALL
  ClearErrors
  CreateDirectory "$INSTDIR"
  FileOpen $9 "$INSTDIR\.srt-probe" w
  IfErrors srt_probe_failed
  FileClose $9
  Delete "$INSTDIR\.srt-probe"
  Goto srt_probe_done
  srt_probe_failed:
    ClearErrors
    WriteRegStr HKLM "Software\S-Read-TXT" "ElevProbe" "1"
    DeleteRegValue HKLM "Software\S-Read-TXT" "ElevProbe"
    IfErrors srt_need_elev
    ; 已是管理员但仍不可写：直接继续（由文件复制报错）
    Goto srt_probe_done
  srt_need_elev:
    MessageBox MB_YESNO|MB_ICONQUESTION "目标目录 $INSTDIR 需要管理员权限才能写入。是否以管理员身份继续安装？" IDNO srt_abort
    ExecShell "runas" "$EXEPATH" "/SRT_ELEVATED /D=$INSTDIR"
    Quit
  srt_abort:
    MessageBox MB_OK|MB_ICONSTOP "已取消：安装目录需要管理员权限。"
    Abort
  srt_probe_done:
!macroend
```
   （详细实现需处理：`/S` 静默透传、`/SRT_ELEVATED` 标记跳过重复提示、IfErrors 分支的 Goto/Label 命名避免冲突。PREUNINSTALL 同理：以"在 $INSTDIR 创建/删除探针文件"判断删除权限，不足且非管理员→`ExecShell runas "$INSTDIR\uninstall.exe" "/SRT_ELEVATED"` + Quit。）
3. debug 构建 + 人工/自动实测：①双击（或 Start-Process）装默认目录 ≤ 全程无 UAC；②指定 `C:\Program Files\...` → 点安装时弹"需要管理员"→选是 → UAC → 完成；③拒绝 → 中止无残留。
4. 提交 `feat(ai): 安装器改用户级（零 UAC 启动）+ 目录按需提权钩子`。

### Task 2 集成引擎 `shell_integration.rs`
**Files:** 新建 `src-tauri/src/shell_integration.rs`、`lib.rs` 导出、`commands.rs`（命令注册）、`main.rs`（注册 + 助手 argv 前置处理）、`ipc_error.rs`（`CODE_INTEGRATION`）。
**接口（供 UI/测试）**
```rust
pub enum IntegScope { User, Machine }
#[derive(Clone, Copy, Serialize)] pub struct IntegOptions { pub txt: bool, pub log: bool, pub context_menu: bool }
pub fn user_status() -> IntegOptions;            // 读 HKCU 判断各键是否存在且 command 指向当前 exe
pub fn machine_status() -> IntegOptions;         // 读 HKLM
pub fn apply(scope, opts, register: bool) -> Result<(), String>; // 直写（调用方保证权限）
pub fn maybe_run_helper(argv: &[String]) -> Option<i32>;         // --integration-write <user|machine> <txt> <log> <menu> <register>
```
- 键表（纯函数 `planned_keys(scope, opts, register) -> Vec<KeyOp>`，单测断言全部路径与值；exe 路径经 `current_exe`）。
- 每用户：`.txt`/`.log` default=ProgID + `OpenWithProgids` 值、ProgID 键（DefaultIcon/shell\open\command=`"exe" "%1"`）、`Applications\S-Read-TXT.exe`（FriendlyAppName、SupportedTypes、shell\open\command）、`SystemFileAssociations\.<ext>\shell\S-Read-TXT`（MUIVerb/Icon/command）。
- 注销：仅删我们键 + OpenWithProgids 值；`.txt/.log` default 若==我们的 ProgID → 置回备份或删除；`<ext>_backup` 值按其值恢复后删除（兼容旧安装器残留）。
- 命令：`integration_status()`、`integration_apply(scope, options, register, elevate: bool)`——machine 且未提权且 elevate=true → `ShellExecute runas` 自进程 `--integration-write machine ...`（等待+校验）；否则返回 `NEEDS_ELEVATION` 错误码供前端确认。
**测试**：键表单测（user/machine × register/unregister × 全组合）、default 还原决策单测、helper 参数解析单测；`cargo test` 全绿。提交 `feat(ai): 系统集成引擎（HKCU 免管理员 + 全局提权助手）`。

### Task 3 设置 UI「系统集成」
**Files:** 新建 `src/windows/settings/IntegrationSection.svelte`；`SettingsApp.svelte`（系统页挂载）；`ipc.ts`（类型+调用）；i18n zh/en（~16 键）。
**UI**：说明文字；三复选框（`data-integration-txt/log/menu`）；状态行（每用户/全局各自的注册情况，`data-integration-status`）；按钮：`data-integration-apply-user`、`data-integration-remove-user`、`data-integration-apply-machine`、`data-integration-remove-machine`、刷新；操作后 toast 结果；需授权时 ConfirmDialog「需要管理员权限，是否现在授权？（会弹出 UAC）」。
**测试**：svelte-check 0/0；vitest（如有组件测试惯例则加，无则跳过）；`probe-integration`（tmp，CDP 点击开关→应用→PowerShell 断言 HKCU 键→注销→断言归零）。提交 `feat(ai): 设置新增系统集成面板`。

### Task 4 卸载严格清理
**Files:** `src-tauri/nsis/installer-hooks.nsh`（POSTUNINSTALL 扩展）。
**步骤**：删除 HKCU+HKLM：`Software\Classes\SReadTXT.txt|log`、`Applications\S-Read-TXT.exe`、`SystemFileAssociations\.txt|.log\shell\S-Read-TXT`（先删子键再删空壳）；OpenWithProgids 值；`_backup` 值条件恢复；旧 ProgID `Text Document`/`Log File` 条件删除（ReadRegStr shell\open\command 含 `$INSTDIR` 才删）；语言键/uninstall 键沿用；保留数据询问与 RMDir。**验证**：debug NSIS 装→用 helper 注册（CLI）→卸载→PowerShell 逐键断言归零（脚本化）。提交 `fix(ai): 卸载器严格清理系统集成键与历史残留`。

### Task 5 release 重建 + E2E + 文档
1. `npm run tauri build`（release，含 NSIS）；2. 扩展 `scripts/smoke-uninstall.mjs`：断言 installer.nsi 的 `RequestExecutionLevel user` 与无 `APP_ASSOCIATE`；静默安装（零 UAC 行为）→ `--integration-write user` 注册 → 断言键存在 → 静默卸载 → 断言归零 + 数据询问路径不变；3. 新增 `scripts/smoke-integration.mjs`（真实 UI 点击路径）并入 verify-all；4. 真实打开验证（CLI 打开 + .log 默认关联场景 → 双击行为）；5. 文档：README（安装/集成/便携版权限说明改写）、CHANGELOG、configuration.md、progress.md、handoff、coverage-matrix；6. verify-all 全量（含 smoke-uninstall，UAC 已授权）；证据提交。
