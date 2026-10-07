; S-Read-TXT NSIS 钩子（Tauri NSIS installerHooks）
; 位置：src-tauri/nsis/installer-hooks.nsh（tauri.conf.json → bundle.windows.nsis.installerHooks）
;
; 设计（2026-10-07 修订）：
; - 安装器/卸载器以普通权限运行（installMode=currentUser → RequestExecutionLevel user），
;   双击打开与常规安装全程不请求 UAC；
; - 系统集成（.txt/.log 关联、右键菜单、打开方式条目）不再由安装器注册（安装零注册表写入），
;   由应用内「设置 → 系统集成」开关管理（用户级 HKCU，无需管理员；全局 HKLM 经应用提权助手）；
; - 仅当目标需要更高权限时（安装目录不可写 / 卸载目录不可删 / 存在全局注册项待清理），
;   才即时以管理员身份重入（UAC 只发生在这一刻，权限只作用于所需目标）；
;   拒绝或静默失败则以非零码/中止收场（静默模式自动尝试提权一次）。
; - 卸载时严格清理本应用写入的全部注册表痕迹（含历史版本残留），并询问是否删除 data/。
;
; 变量说明：$EXEPATH 为当前安装器/卸载器自身路径；$9 为探针文件句柄（局部临时使用）。

; StrFunc 声明：卸载清理需要字符串包含判断（UnStrStr 为卸载上下文变体）
${Using:StrFunc} UnStrStr

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
  MessageBox MB_OK|MB_ICONSTOP "安装已取消：目标目录需要管理员权限。可改用默认目录（无需管理员）后重试。" /SD IDOK
  Abort

  srt_pre_done:
!macroend

; ---- 卸载前：删除权限检测（目录不可删 / 存在全局注册项）+ 按需提权重入 ----
!macro NSIS_HOOK_PREUNINSTALL
  ; 提权重入时恢复安装目录：卸载器克隆体运行于 %TEMP%\~nsuN.tmp，其 $INSTDIR 默认指向
  ; 临时目录；原始实例在重入前把真实安装目录写入 HKCU 中转值（规避命令行引号/空格/中文问题）。
  ClearErrors
  ReadRegStr $0 HKCU "Software\S-Read-TXT" "UninstallDir"
  IfErrors srt_un_dir_ready
  StrCmp $0 "" srt_un_dir_ready
  StrCpy $INSTDIR $0
  DeleteRegValue HKCU "Software\S-Read-TXT" "UninstallDir"
  srt_un_dir_ready:

  ClearErrors
  FileOpen $9 "$INSTDIR\.srt-del-probe" w
  IfErrors srt_un_no_write
  FileClose $9
  Delete "$INSTDIR\.srt-del-probe"
  IfFileExists "$INSTDIR\.srt-del-probe" srt_un_no_write
  ; 目录可删：继续检查是否存在全局（HKLM）集成项（清理它们需要管理员）
  ClearErrors
  ReadRegStr $0 HKLM "Software\Classes\SReadTXT.txt\shell\open\command" ""
  IfErrors 0 srt_un_maybe_admin
  ReadRegStr $0 HKLM "Software\Classes\SReadTXT.log\shell\open\command" ""
  IfErrors 0 srt_un_maybe_admin
  Goto srt_un_done

  srt_un_no_write:
  srt_un_maybe_admin:
  ; 判断是否已是管理员：HKLM 写探针成功=管理员
  ClearErrors
  WriteRegStr HKLM "Software\S-Read-TXT" "ElevationProbe" "1"
  DeleteRegValue HKLM "Software\S-Read-TXT" "ElevationProbe"
  IfErrors srt_un_request_elev
  Goto srt_un_done

  srt_un_request_elev:
  ; 提权重入目标：优先启动安装目录中的原版卸载器（UAC 显示 S-Read-TXT），
  ; 而非当前运行中的临时克隆体（Un.exe，名称不可辨）。
  StrCpy $R9 "$EXEPATH"
  IfFileExists "$INSTDIR\卸载 S-Read-TXT.exe" 0 srt_un_elev_legacy
  StrCpy $R9 "$INSTDIR\卸载 S-Read-TXT.exe"
  Goto srt_un_elev_ready
  srt_un_elev_legacy:
  IfFileExists "$INSTDIR\uninstall.exe" 0 srt_un_elev_ready
  StrCpy $R9 "$INSTDIR\uninstall.exe"
  srt_un_elev_ready:
  IfSilent srt_un_silent_elev
  MessageBox MB_YESNO|MB_ICONQUESTION "卸载需要管理员权限（安装目录受保护，或存在待清理的全局注册项）。$\r$\n是否以管理员身份继续卸载？" IDNO srt_un_abort
  WriteRegStr HKCU "Software\S-Read-TXT" "UninstallDir" "$INSTDIR"
  ExecShell "runas" "$R9" "/SRT_ELEVATED"
  Quit

  srt_un_silent_elev:
  WriteRegStr HKCU "Software\S-Read-TXT" "UninstallDir" "$INSTDIR"
  ExecShell "runas" "$R9" "/SRT_ELEVATED /S"
  Quit

  srt_un_abort:
  MessageBox MB_OK|MB_ICONSTOP "卸载已取消：需要管理员权限。" /SD IDOK
  Abort

  srt_un_done:
!macroend

; ---- 安装后：卸载器更名（直观可辨）并同步登记表指向 ----
!macro NSIS_HOOK_POSTINSTALL
  Delete "$INSTDIR\卸载 S-Read-TXT.exe"
  IfFileExists "$INSTDIR\uninstall.exe" 0 srt_post_rename_done
  Rename "$INSTDIR\uninstall.exe" "$INSTDIR\卸载 S-Read-TXT.exe"
  WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\卸载 S-Read-TXT.exe$\""
  srt_post_rename_done:
!macroend

; ---- 卸载后：数据询问 + 注册表严格清理 + 目录收尾 ----
;
; 背景：Tauri 主模板卸载时只执行 `RMDir "$INSTDIR"`（不带 /r）——应用运行时产生的
;       data/ 目录会让安装目录保留；模板自带的「删除应用程序数据」只清理
;       %APPDATA%/%LOCALAPPDATA% 下的目录，不含便携模式的 data/。
;
; 本钩子（POSTUNINSTALL，此时主模板已完成「应用是否在运行」检查）：
;   1. 询问是否删除 data/（历史记录、设置、阅读进度、WebView2 缓存与崩溃转储）；
;      静默卸载（/S）经 /SD IDYES 默认删除；
;   2. 清理安装位置与语言注册表残留（与安装时写入一致，幂等）；
;   3. 严格清理系统集成痕迹：本应用所有权的键/值（HKCU+HKLM），
;      以及历史安装器（FileAssociation.nsh）遗留的 ProgID 与备份值；
;   4. 最后再尝试移除安装目录（数据删除后通常已为空）。
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
  ; 提权中转值与权限探针键（本应用自有）
  DeleteRegValue HKCU "Software\S-Read-TXT" "UninstallDir"
  DeleteRegKey /ifempty HKCU "Software\S-Read-TXT"
  DeleteRegKey HKLM "Software\S-Read-TXT"

  ; 4) 系统集成清理 —— 4a) 本应用所有权键（对称于应用内注册布局）
  DeleteRegKey HKCU "Software\Classes\SReadTXT.txt"
  DeleteRegKey HKCU "Software\Classes\SReadTXT.log"
  DeleteRegKey HKCU "Software\Classes\Applications\s-read-txt.exe"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.txt\shell\S-Read-TXT"
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.log\shell\S-Read-TXT"
  DeleteRegValue HKCU "Software\Classes\.txt\OpenWithProgids" "SReadTXT.txt"
  DeleteRegValue HKCU "Software\Classes\.log\OpenWithProgids" "SReadTXT.log"
  DeleteRegValue HKCU "Software\Classes\.txt" "SReadTXT.txt_backup"
  DeleteRegValue HKCU "Software\Classes\.log" "SReadTXT.log_backup"
  DeleteRegKey HKLM "Software\Classes\SReadTXT.txt"
  DeleteRegKey HKLM "Software\Classes\SReadTXT.log"
  DeleteRegKey HKLM "Software\Classes\Applications\s-read-txt.exe"
  DeleteRegKey HKLM "Software\Classes\SystemFileAssociations\.txt\shell\S-Read-TXT"
  DeleteRegKey HKLM "Software\Classes\SystemFileAssociations\.log\shell\S-Read-TXT"
  DeleteRegValue HKLM "Software\Classes\.txt\OpenWithProgids" "SReadTXT.txt"
  DeleteRegValue HKLM "Software\Classes\.log\OpenWithProgids" "SReadTXT.log"
  DeleteRegValue HKLM "Software\Classes\.txt" "SReadTXT.txt_backup"
  DeleteRegValue HKLM "Software\Classes\.log" "SReadTXT.log_backup"

  ; 4b) 历史安装器（FileAssociation.nsh，ProgID「Text Document」）残留清理：
  ;     默认值指向旧 ProgID 时优先按备份值还原，否则清除；备份值随后删除。
  ClearErrors
  ReadRegStr $0 HKCU "Software\Classes\.txt" ""
  IfErrors srt_lg_txt_done_hkcu
  StrCmp $0 "Text Document" 0 srt_lg_txt_done_hkcu
  ClearErrors
  ReadRegStr $1 HKCU "Software\Classes\.txt" "Text Document_backup"
  IfErrors 0 srt_lg_txt_restore_hkcu
  DeleteRegValue HKCU "Software\Classes\.txt" ""
  Goto srt_lg_txt_cleanup_hkcu
  srt_lg_txt_restore_hkcu:
  WriteRegStr HKCU "Software\Classes\.txt" "" "$1"
  srt_lg_txt_cleanup_hkcu:
  DeleteRegValue HKCU "Software\Classes\.txt" "Text Document_backup"
  srt_lg_txt_done_hkcu:

  ClearErrors
  ReadRegStr $0 HKCU "Software\Classes\.log" ""
  IfErrors srt_lg_log_done_hkcu
  StrCmp $0 "Log File" 0 srt_lg_log_done_hkcu
  ClearErrors
  ReadRegStr $1 HKCU "Software\Classes\.log" "Log File_backup"
  IfErrors 0 srt_lg_log_restore_hkcu
  DeleteRegValue HKCU "Software\Classes\.log" ""
  Goto srt_lg_log_cleanup_hkcu
  srt_lg_log_restore_hkcu:
  WriteRegStr HKCU "Software\Classes\.log" "" "$1"
  srt_lg_log_cleanup_hkcu:
  DeleteRegValue HKCU "Software\Classes\.log" "Log File_backup"
  srt_lg_log_done_hkcu:

  ; 4c) 历史 ProgID 键仅在命令行确实指向本安装目录时删除（避免误删同名键）
  ClearErrors
  ReadRegStr $2 HKCU "Software\Classes\Text Document\shell\open\command" ""
  IfErrors srt_lg_progid_txt_done_hkcu
  ${UnStrStr} $3 "$2" "$INSTDIR"
  StrCmp $3 "" srt_lg_progid_txt_done_hkcu
  DeleteRegKey HKCU "Software\Classes\Text Document"
  srt_lg_progid_txt_done_hkcu:

  ClearErrors
  ReadRegStr $2 HKCU "Software\Classes\Log File\shell\open\command" ""
  IfErrors srt_lg_progid_log_done_hkcu
  ${UnStrStr} $3 "$2" "$INSTDIR"
  StrCmp $3 "" srt_lg_progid_log_done_hkcu
  DeleteRegKey HKCU "Software\Classes\Log File"
  srt_lg_progid_log_done_hkcu:

  ; 4d) 历史残留的全局（HKLM）副本：不可写时保留备份值（避免数据丢失）
  ClearErrors
  ReadRegStr $0 HKLM "Software\Classes\.txt" ""
  IfErrors srt_lg_txt_done_hklm
  StrCmp $0 "Text Document" 0 srt_lg_txt_done_hklm
  ClearErrors
  ReadRegStr $1 HKLM "Software\Classes\.txt" "Text Document_backup"
  IfErrors 0 srt_lg_txt_restore_hklm
  ClearErrors
  DeleteRegValue HKLM "Software\Classes\.txt" ""
  Goto srt_lg_txt_done_hklm
  srt_lg_txt_restore_hklm:
  ClearErrors
  WriteRegStr HKLM "Software\Classes\.txt" "" "$1"
  IfErrors srt_lg_txt_done_hklm
  DeleteRegValue HKLM "Software\Classes\.txt" "Text Document_backup"
  srt_lg_txt_done_hklm:

  ClearErrors
  ReadRegStr $0 HKLM "Software\Classes\.log" ""
  IfErrors srt_lg_log_done_hklm
  StrCmp $0 "Log File" 0 srt_lg_log_done_hklm
  ClearErrors
  ReadRegStr $1 HKLM "Software\Classes\.log" "Log File_backup"
  IfErrors 0 srt_lg_log_restore_hklm
  ClearErrors
  DeleteRegValue HKLM "Software\Classes\.log" ""
  Goto srt_lg_log_done_hklm
  srt_lg_log_restore_hklm:
  ClearErrors
  WriteRegStr HKLM "Software\Classes\.log" "" "$1"
  IfErrors srt_lg_log_done_hklm
  DeleteRegValue HKLM "Software\Classes\.log" "Log File_backup"
  srt_lg_log_done_hklm:

  ClearErrors
  ReadRegStr $2 HKLM "Software\Classes\Text Document\shell\open\command" ""
  IfErrors srt_lg_progid_txt_done_hklm
  ${UnStrStr} $3 "$2" "$INSTDIR"
  StrCmp $3 "" srt_lg_progid_txt_done_hklm
  DeleteRegKey HKLM "Software\Classes\Text Document"
  srt_lg_progid_txt_done_hklm:

  ClearErrors
  ReadRegStr $2 HKLM "Software\Classes\Log File\shell\open\command" ""
  IfErrors srt_lg_progid_log_done_hklm
  ${UnStrStr} $3 "$2" "$INSTDIR"
  StrCmp $3 "" srt_lg_progid_log_done_hklm
  DeleteRegKey HKLM "Software\Classes\Log File"
  srt_lg_progid_log_done_hklm:

  ; 5) 安装目录收尾：删除卸载器自身（更名后与旧名并存场景）与目录
  Delete "$INSTDIR\卸载 S-Read-TXT.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
!macroend
