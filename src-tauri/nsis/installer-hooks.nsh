; S-Read-TXT NSIS 钩子（Tauri NSIS installerHooks）
; 位置：src-tauri/nsis/installer-hooks.nsh（tauri.conf.json → bundle.windows.nsis.installerHooks）
;
; 设计（2026-10-07 修订）：
; - 安装器/卸载器以普通权限运行（installMode=currentUser → RequestExecutionLevel user），
;   双击打开与常规安装全程不请求 UAC；
; - 系统集成（.txt/.log 关联、右键菜单、打开方式条目）不再由安装器注册（安装零注册表写入），
;   由应用内「设置 → 系统集成」开关管理（用户级 HKCU，无需管理员）；
; - 仅当用户把安装目录改到受保护位置（如 Program Files）时，PREINSTALL/PREUNINSTALL
;   探测写/删权限，不足则即时以管理员身份重入（UAC 只发生在这一刻，权限只作用于该目录）；
;   拒绝或静默失败则以非零码/中止收场（静默模式自动尝试提权一次）。
;
; 变量说明：$EXEPATH 为当前安装器/卸载器自身路径；$9 为探针文件句柄（局部临时使用）。

; ---- 安装前：目录可写性检测 + 按需提权重入 ----
!macro NSIS_HOOK_PREINSTALL
  ClearErrors
  CreateDirectory "$INSTDIR"
  FileOpen $9 "$INSTDIR\.srt-write-probe" w
  IfErrors srt_pre_no_write
  FileClose $9
  Delete "$INSTDIR\.srt-write-probe"
  Goto srt_pre_done

  srt_pre_no_write:
  ; 判断是否已是管理员：HKLM 写探针成功=管理员
  ClearErrors
  WriteRegStr HKLM "Software\S-Read-TXT" "ElevationProbe" "1"
  DeleteRegValue HKLM "Software\S-Read-TXT" "ElevationProbe"
  IfErrors srt_pre_request_elev
  ; 已是管理员仍不可写（罕见）：继续，交由文件复制给出系统错误
  Goto srt_pre_done

  srt_pre_request_elev:
  IfSilent srt_pre_silent_elev
  MessageBox MB_YESNO|MB_ICONQUESTION "目标目录：$\r$\n$INSTDIR$\r$\n需要管理员权限才能写入。$\r$\n是否以管理员身份继续安装？（仅授权写入该目录）" IDNO srt_pre_abort
  ExecShell "runas" "$EXEPATH" "/SRT_ELEVATED /D=$INSTDIR"
  Quit

  srt_pre_silent_elev:
  ExecShell "runas" "$EXEPATH" "/SRT_ELEVATED /S /D=$INSTDIR"
  Quit

  srt_pre_abort:
  MessageBox MB_OK|MB_ICONSTOP "安装已取消：目标目录需要管理员权限。可改用默认目录（无需管理员）后重试。"
  Abort

  srt_pre_done:
!macroend

; ---- 卸载前：目录删除权限检测 + 按需提权重入 ----
!macro NSIS_HOOK_PREUNINSTALL
  ClearErrors
  FileOpen $9 "$INSTDIR\.srt-del-probe" w
  IfErrors srt_un_no_write
  FileClose $9
  Delete "$INSTDIR\.srt-del-probe"
  IfFileExists "$INSTDIR\.srt-del-probe" srt_un_no_write
  Goto srt_un_done

  srt_un_no_write:
  ; 判断是否已是管理员：HKLM 写探针成功=管理员
  ClearErrors
  WriteRegStr HKLM "Software\S-Read-TXT" "ElevationProbe" "1"
  DeleteRegValue HKLM "Software\S-Read-TXT" "ElevationProbe"
  IfErrors srt_un_request_elev
  Goto srt_un_done

  srt_un_request_elev:
  IfSilent srt_un_silent_elev
  MessageBox MB_YESNO|MB_ICONQUESTION "卸载需要管理员权限（安装目录受保护）：$\r$\n$INSTDIR$\r$\n是否以管理员身份继续卸载？" IDNO srt_un_abort
  ExecShell "runas" "$EXEPATH" "/SRT_ELEVATED"
  Quit

  srt_un_silent_elev:
  ExecShell "runas" "$EXEPATH" "/SRT_ELEVATED /S"
  Quit

  srt_un_abort:
  MessageBox MB_OK|MB_ICONSTOP "卸载已取消：需要管理员权限。"
  Abort

  srt_un_done:
!macroend

; ---- 卸载后：数据询问 + 注册表残留清理 + 目录收尾 ----
;
; 背景：Tauri 主模板卸载时只执行 `RMDir "$INSTDIR"`（不带 /r）——应用运行时产生的
;       data/ 目录会让安装目录保留；模板自带的「删除应用程序数据」只清理
;       %APPDATA%/%LOCALAPPDATA% 下的目录，不含便携模式的 data/。
;
; 本钩子（POSTUNINSTALL，此时主模板已完成「应用是否在运行」检查）：
;   1. 询问是否删除 data/（历史记录、设置、阅读进度、WebView2 缓存与崩溃转储）；
;      静默卸载（/S）经 /SD IDYES 默认删除；
;   2. 清理安装位置与语言注册表残留（与安装时写入一致，幂等）；
;   3. 最后再尝试移除安装目录（数据删除后通常已为空）。
;
; 注意：`$(^Name)` 在卸载上下文可用；消息框正文使用 $\r$\n 换行。

!macro NSIS_HOOK_POSTUNINSTALL
  ; 1) 询问（静默模式自动选「是」）
  MessageBox MB_YESNO|MB_ICONQUESTION "是否删除阅读数据（data 目录：历史记录、设置、阅读进度与缓存）？$\r$\n选择「否」将保留数据，可在稍后手动删除。" /SD IDYES IDNO srt_keep_data

  ; 2) 删除便携数据目录（含 WebView2 用户数据与崩溃转储 .dmp）
  RMDir /r "$INSTDIR\data"

  srt_keep_data:

  ; 3) 注册表残留清理（安装位置标记与语言；与主模板勾选逻辑一致，幂等）
  DeleteRegKey SHCTX "${MANUPRODUCTKEY}"
  DeleteRegKey /ifempty SHCTX "${MANUKEY}"
  DeleteRegValue HKCU "${MANUPRODUCTKEY}" "Installer Language"
  DeleteRegKey /ifempty HKCU "${MANUPRODUCTKEY}"
  DeleteRegKey /ifempty HKCU "${MANUKEY}"

  ; 4) 尝试彻底移除安装目录
  RMDir "$INSTDIR"
!macroend
