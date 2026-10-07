@echo off
chcp 936 >nul
setlocal EnableExtensions EnableDelayedExpansion

rem S-Read-TXT 右键菜单与打开方式清理工具（仅当前用户 HKCU，无需管理员）
rem 用途：便携版换位置/异常残留时，一键删除本程序的「打开方式」与右键菜单登记；
rem       与卸载器同款清理逻辑（含默认值备份还原与历史残留）。
rem 用法：双击运行；自动化场景可加参数 /quiet（不等待按键）。

set "QUIET=0"
if /i "%~1"=="/quiet" set "QUIET=1"

echo.
echo ============================================
echo   S-Read-TXT 右键菜单与打开方式清理工具
echo ============================================
echo.

rem —— 1) 本应用所有权键（与设置面板/卸载器对称） ——
reg delete "HKCU\Software\Classes\SReadTXT.txt" /f >nul 2>nul
reg delete "HKCU\Software\Classes\SReadTXT.log" /f >nul 2>nul
reg delete "HKCU\Software\Classes\Applications\s-read-txt.exe" /f >nul 2>nul
reg delete "HKCU\Software\Classes\SystemFileAssociations\.txt\shell\S-Read-TXT" /f >nul 2>nul
reg delete "HKCU\Software\Classes\SystemFileAssociations\.log\shell\S-Read-TXT" /f >nul 2>nul
reg delete "HKCU\Software\S-Read-TXT" /f >nul 2>nul
echo [1/3] 已删除「打开方式」候选与右键菜单登记

rem —— 2) 「打开方式」候选表与备份值 ——
reg delete "HKCU\Software\Classes\.txt\OpenWithProgids" /v SReadTXT.txt /f >nul 2>nul
reg delete "HKCU\Software\Classes\.log\OpenWithProgids" /v SReadTXT.log /f >nul 2>nul
reg delete "HKCU\Software\Classes\.txt" /v SReadTXT.txt_backup /f >nul 2>nul
reg delete "HKCU\Software\Classes\.log" /v SReadTXT.log_backup /f >nul 2>nul
echo [2/3] 已清理扩展名关联表残留值

rem —— 3) 默认值残留（本应用 ProgID / 历史旧 ProgID，按备份还原或清除） ——
call :fix_default ".txt" "SReadTXT.txt" "Text Document"
call :fix_default ".log" "SReadTXT.log" "Log File"
call :drop_old_progid "Text Document"
call :drop_old_progid "Log File"
echo [3/3] 已处理默认值与历史版本残留（按备份还原，否则清除；旧登记仅在指向本程序时删除）

echo.
echo 清理完成：右键菜单、打开方式与历史残留均已处理。
echo 提示：如仍看到「用 S-Read-TXT 打开」，刷新桌面（F5）或注销后即可消失。
echo.
if "%QUIET%"=="0" pause
exit /b 0

rem —— 子过程：扩展名默认值为本应用/旧 ProgID 时，按备份值还原或清除 ——
:fix_default
set "CUR="
for /f "delims=" %%L in ('reg query "HKCU\Software\Classes\%~1" /ve 2^>nul ^| findstr /i "REG_SZ"') do set "CUR=%%L"
if defined CUR set "CUR=!CUR:*REG_SZ=!"
if defined CUR for /f "tokens=*" %%X in ("!CUR!") do set "CUR=%%X"
if not defined CUR goto :eof
if /i "!CUR!"=="%~2" (
  set "BK=%~2_backup"
) else if /i "!CUR!"=="%~3" (
  set "BK=%~3_backup"
) else (
  goto :eof
)
reg query "HKCU\Software\Classes\%~1" /v "!BK!" >nul 2>nul
if errorlevel 1 (
  reg delete "HKCU\Software\Classes\%~1" /ve /f >nul 2>nul
) else (
  set "DATA="
  for /f "delims=" %%L in ('reg query "HKCU\Software\Classes\%~1" /v "!BK!" 2^>nul ^| findstr /i "REG_SZ"') do set "DATA=%%L"
  if defined DATA set "DATA=!DATA:*REG_SZ=!"
  if defined DATA for /f "tokens=*" %%X in ("!DATA!") do set "DATA=%%X"
  if defined DATA reg add "HKCU\Software\Classes\%~1" /ve /d "!DATA!" /f >nul
)
reg delete "HKCU\Software\Classes\%~1" /v "!BK!" /f >nul 2>nul
goto :eof

rem —— 子过程：旧 ProgID 键仅在命令行指向本程序（s-read-txt）时删除 ——
:drop_old_progid
reg query "HKCU\Software\Classes\%~1\shell\open\command" /ve 2>nul | findstr /i /c:"s-read-txt" >nul
if not errorlevel 1 reg delete "HKCU\Software\Classes\%~1" /f >nul 2>nul
goto :eof
