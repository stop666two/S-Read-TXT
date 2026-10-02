// 原生对话框探针（scripts/lib/dialog.mjs）
// 用途：检测目标应用进程内出现的原生对话框窗口（Windows 类名 `#32770`），
//       并可选发送 WM_CLOSE 关闭（用于自动化测试「打开/另存为」等系统对话框）。
// 实现：PowerShell user32 EnumWindows 枚举（参数经环境变量传入——PS 5.1 的
//       -Command 会把尾随参数拼进脚本，直接传参不可用）。

import { spawnSync } from 'node:child_process';

import { delay } from './smoke-cdp.mjs';

/** PowerShell 探针脚本：枚举目标进程的 #32770 窗口；action=close 时 PostMessage WM_CLOSE。 */
const PS_DIALOG = `
$target = [uint32]$env:SRT_DLG_PID
$action = [string]$env:SRT_DLG_ACTION
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public class SrtDlgProbe {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  public static List<string> Lines = new List<string>();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
}
'@
$global:target = $target
$global:action = $action
$cb = [SrtDlgProbe+EnumProc]{
  param($h, $l)
  if ([SrtDlgProbe]::IsWindowVisible($h)) {
    $wpid = [uint32]0
    [SrtDlgProbe]::GetWindowThreadProcessId($h, [ref]$wpid) | Out-Null
    if ($wpid -eq $global:target) {
      $c = New-Object System.Text.StringBuilder 256
      [SrtDlgProbe]::GetClassName($h, $c, 256) | Out-Null
      if ($c.ToString() -eq '#32770') {
        [void][SrtDlgProbe]::Lines.Add($c.ToString())
        if ($global:action -eq 'close') { [SrtDlgProbe]::PostMessage($h, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
      }
    }
  }
  return $true
}
[SrtDlgProbe]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
if ([SrtDlgProbe]::Lines.Count -gt 0) { 'FOUND' } else { 'NONE' }
`;

/**
 * 创建对话框探测/关闭操作器。
 *
 * 参数：`pid` 目标应用进程 id（本应用主进程，不是 WebView2 子进程）。
 * 返回：
 * - `dialogOp(action)`：同步执行一次探测/关闭（`find` | `close`），返回 'FOUND' 或 'NONE'；
 * - `waitDialog(expectFound, timeoutMs)`：轮询直到状态符合预期（默认 6 秒）。
 */
export function createDialogOps(pid) {
  const dialogOp = (action) => {
    const res = spawnSync('powershell', ['-NoProfile', '-Command', PS_DIALOG], {
      encoding: 'utf8',
      timeout: 20000,
      env: { ...process.env, SRT_DLG_PID: String(pid), SRT_DLG_ACTION: action },
    });
    return (res.stdout ?? '').trim();
  };

  const waitDialog = async (expectFound, timeoutMs = 6000) => {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const result = dialogOp('find');
      if ((result === 'FOUND') === expectFound) return true;
      if (Date.now() > deadline) return false;
      await delay(300);
    }
  };

  return { dialogOp, waitDialog };
}
