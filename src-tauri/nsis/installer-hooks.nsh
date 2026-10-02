; S-Read-TXT 卸载钩子（Tauri NSIS installerHooks）
; 位置：src-tauri/nsis/installer-hooks.nsh（tauri.conf.json → bundle.windows.nsis.installerHooks）
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
