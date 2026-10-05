#!/usr/bin/env node
// 会话与窗口恢复 E2E（smoke-session）
// 场景：① 打开两个文件、滚动、改编码、移动窗口（user32 真实移动）→ X 关闭（应触发会话保存）
//       ② 同数据目录重启 → 标签/活动标签/编码覆盖/滚动位置/窗口几何全部恢复
//       ③ 删除其中一个文件 → 再启动 → 缺失文件被跳过（仅恢复存在的标签）
//
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-session.mjs [--exe <路径>] [--port 9230]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, findTarget, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const port = Number(argValue('--port', String(9200 + Math.floor(Math.random() * 600))));
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '240000');

const workDir = join(process.env.TEMP ?? '.', 'srt-smoke-session');
const fileA = join(workDir, 'session-a.txt');
const fileB = join(workDir, 'session-b.txt');
const runDataDir = join(workDir, `data-${Date.now()}`);

const checks = [];
let currentStep = 'T0 启动';
function check(name, passed, detail = '') {
  checks.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

const watchdog = setTimeout(() => {
  console.error(`看门狗超时（${watchdogMs}ms），最后步骤：${currentStep}`);
  process.exit(4);
}, watchdogMs);

/** user32 窗口操作（move=移动并改尺寸 / get=读取矩形 / close=WM_CLOSE）：
 *  按类名 `Tauri Window` + 目标进程 + 可见性定位主窗口；参数经环境变量传递。 */
const PS_WINDOW = `
$target = [uint32]$env:SRT_WIN_PID
$action = [string]$env:SRT_WIN_ACTION
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public class SrtWinOps {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder sb, int max);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int ht, bool repaint);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
'@
$found = $null
$cb = [SrtWinOps+EnumProc]{
  param($h, $l)
  if ([SrtWinOps]::IsWindowVisible($h)) {
    $wpid = [uint32]0
    [SrtWinOps]::GetWindowThreadProcessId($h, [ref]$wpid) | Out-Null
    if ($wpid -eq $target) {
      $c = New-Object System.Text.StringBuilder 256
      [SrtWinOps]::GetClassName($h, $c, 256) | Out-Null
      if ($c.ToString() -eq 'Tauri Window') { $script:found = $h }
    }
  }
  return $true
}
[SrtWinOps]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
if ($script:found -eq $null) { 'NONE'; exit 0 }
if ($action -eq 'move') { [void][SrtWinOps]::MoveWindow($script:found, 100, 100, 900, 600, $true); 'MOVED' }
elseif ($action -eq 'close') { [void][SrtWinOps]::PostMessage($script:found, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero); 'CLOSED' }
else {
  $r = New-Object SrtWinOps+RECT
  [void][SrtWinOps]::GetWindowRect($script:found, [ref]$r)
  '' + $r.Left + ',' + $r.Top + ',' + ($r.Right - $r.Left) + ',' + ($r.Bottom - $r.Top)
}
`;

/** 执行窗口操作（返回 stdout 文本） */
function winOp(pid, action) {
  const result = spawnSync('powershell', ['-NoProfile', '-Command', PS_WINDOW], {
    encoding: 'utf8',
    timeout: 20000,
    env: { ...process.env, SRT_WIN_PID: String(pid), SRT_WIN_ACTION: action },
  });
  return (result.stdout ?? '').trim();
}

/** 等待进程退出 */
function waitGone(pid, timeoutMs = 20000) {
  return waitForValue(async () => {
    const probe = spawnSync('powershell', [
      '-NoProfile',
      '-Command',
      `if (Get-Process -Id ${pid} -ErrorAction SilentlyContinue) { 'ALIVE' } else { 'GONE' }`,
    ], { encoding: 'utf8', timeout: 15000 });
    return (probe.stdout ?? '').trim() === 'GONE' ? true : null;
  }, timeoutMs);
}

async function removeWithRetry(path, attempts = 12) {
  for (let i = 0; i < attempts; i += 1) {
    try {
      rmSync(path, { recursive: true, force: true });
    } catch {
      // 忽略并重试
    }
    if (!existsSync(path)) return;
    await delay(250);
  }
}

/** 启动应用并建立 CDP 客户端（等待就绪） */
async function launchAndConnect() {
  const child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: runDataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });
  let client = await createClient(await findTarget(port));
  const evalJs = async (expression) => {
    let result;
    try {
      result = await client.send('Runtime.evaluate', {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
    } catch (error) {
      throw new Error(`求值失败（${expression.slice(0, 90)}…）：${error?.message ?? error}`);
    }
    if (result.exceptionDetails) throw new Error(`页面脚本异常：${result.exceptionDetails.text}`);
    return result.result?.value;
  };
  await waitForValue(async () => {
    const ready = await evalJs(
      '(() => !!(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke && window.__srt && window.__srt.openPath))()',
    );
    return ready ? true : null;
  }, 30000);
  return { child, client, evalJs };
}

async function main() {
  if (!existsSync(exePath)) {
    console.error(`可执行文件不存在：${exePath}（先运行 npm run tauri build -- --debug --no-bundle）`);
    process.exit(2);
  }
  mkdirSync(workDir, { recursive: true });
  writeFileSync(fileA, Array.from({ length: 60 }, (_, i) => `A-${i + 1}`).join('\n') + '\n', 'utf8');
  writeFileSync(fileB, Array.from({ length: 800 }, (_, i) => `B-${i + 1}`).join('\n') + '\n', 'utf8');

  const tabsState = (evalJs) =>
    evalJs(
      `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return { count: v.tabs.length, names: v.tabs.map((t) => t.name), active: v.tabs.find((t) => t.tabId === v.activeTabId)?.name ?? null, aOverride: v.tabs.find((t) => t.name === 'session-a.txt')?.encodingOverride ?? null }; })()`,
    );

  let first = null;
  let second = null;
  let third = null;
  try {
    // ---- 步骤 1：建立现场（两个标签、滚动、编码覆盖、移动窗口）----
    currentStep = 'T1 步骤一：建立现场';
    first = await launchAndConnect();
    currentStep = 'T1.1 打开 a';
    // 注意：openPath 的原生 Promise 不交给 CDP await（WebView2 下会偶发 “Promise was collected”），
    // 改为页面内触发 + 轮询后端状态确认。
    await first.evalJs(`(void window.__srt.openPath(${JSON.stringify(fileA)}), true)`);
    await waitForValue(async () => ((await tabsState(first.evalJs)).count === 1 ? true : null), 8000);
    currentStep = 'T1.2 设置编码覆盖';
    await first.evalJs(
      `(() => { window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => window.__TAURI_INTERNALS__.invoke('set_encoding', { tabId: v.activeTabId, encoding: 'GB18030' })); return true; })()`,
    );
    await waitForValue(async () => ((await tabsState(first.evalJs)).aOverride === 'GB18030' ? true : null), 8000);
    currentStep = 'T1.3 打开 b';
    await first.evalJs(`(void window.__srt.openPath(${JSON.stringify(fileB)}), true)`);
    await waitForValue(async () => ((await tabsState(first.evalJs)).count === 2 ? true : null), 8000);
    currentStep = 'T1.4 核对标签';
    let state = await tabsState(first.evalJs);
    check('T1a 两个标签已打开（活动 = b）', state.count === 2 && state.active === 'session-b.txt', JSON.stringify(state));

    // 先等待阅读区完成首屏渲染（存在行元素）再滚动，避免与挂载竞态导致 scrollTop 不生效
    await waitForValue(async () => {
      const ready = await first.evalJs(`document.querySelectorAll('.reader .row').length > 0`);
      return ready ? true : null;
    }, 8000);
    await first.evalJs(`(document.querySelector('.reader').scrollTop = 1500, true)`);
    const scrolled = await waitForValue(async () => {
      const top = await first.evalJs(`document.querySelector('.reader').scrollTop`);
      return top > 500 ? top : null;
    }, 8000);
    check('T1b 已滚动到中部', scrolled !== null, `scrollTop=${scrolled}`);
    // 留出时间让滚动记忆（rAF 内实时写入）落地
    await delay(400);

    const moved = winOp(first.child.pid, 'move');
    check('T1c 窗口已移动到 100,100 / 900×600', moved === 'MOVED', moved);
    await delay(600);

    currentStep = 'T2 X 关闭（保存会话）';
    winOp(first.child.pid, 'close');
    const exited1 = await waitGone(first.child.pid);
    check('T2 应用已退出（X 关闭触发会话保存）', exited1 === true);
    first.client?.close?.();

    // ---- 步骤 2：同数据目录重启 → 全部恢复 ----
    currentStep = 'T3 步骤二：会话恢复';
    second = await launchAndConnect();
    await delay(500);
    state = await tabsState(second.evalJs);
    check('T3a 两个标签已恢复', state.count === 2, JSON.stringify(state));
    check('T3b 活动标签恢复为 b', state.active === 'session-b.txt', `active=${state.active}`);
    check('T3c 编码覆盖恢复（a = GB18030）', state.aOverride === 'GB18030', `aOverride=${state.aOverride}`);

    const rect = waitForValue(async () => {
      const value = winOp(second.child.pid, 'get');
      return value && value !== 'NONE' ? value : null;
    }, 8000);
    const parsed = (await rect)?.split(',').map(Number) ?? [];
    // 说明：无边框窗口经 user32 GetWindowRect 读取的是「含不可见缩放边框」的外部矩形，
    // 与 Tauri 内部尺寸存在系统级边框差（宽约 16px、高约 31px），故尺寸容许 ±45、
    // 位置容许 ±3；核心验证点是「恢复到保存时的几何」。
    check(
      'T3d 窗口几何恢复（位置 ±3，尺寸 ±45）',
      parsed.length === 4 &&
        Math.abs(parsed[0] - 100) <= 3 &&
        Math.abs(parsed[1] - 100) <= 3 &&
        Math.abs(parsed[2] - 900) <= 45 &&
        Math.abs(parsed[3] - 600) <= 45,
      `rect=${await rect}`,
    );

    const restoredScroll = await waitForValue(async () => {
      const top = await second.evalJs(`document.querySelector('.reader')?.scrollTop ?? 0`);
      return top > 500 ? top : null;
    }, 8000);
    check('T3e 滚动位置恢复（b 标签）', restoredScroll !== null, `scrollTop=${restoredScroll}`);

    currentStep = 'T4 干净退出';
    winOp(second.child.pid, 'close');
    const exited2 = await waitGone(second.child.pid);
    check('T4 第二阶段应用已退出', exited2 === true);
    second.client?.close?.();

    // ---- 步骤 3：删除 b → 缺失文件跳过 ----
    currentStep = 'T5 缺失文件跳过';
    rmSync(fileB, { force: true });
    third = await launchAndConnect();
    const recovered = await waitForValue(async () => {
      const s = await tabsState(third.evalJs);
      return s.count >= 1 ? s : null;
    }, 10000);
    const finalState = recovered ?? (await tabsState(third.evalJs));
    check(
      'T5a 仅恢复存在的标签（b 缺失被跳过）',
      finalState.count === 1 && finalState.names[0] === 'session-a.txt',
      JSON.stringify(finalState),
    );

    // 汇总
    const failed = checks.filter((item) => !item.passed);
    console.log(`\n会话恢复冒烟：${checks.length - failed.length}/${checks.length} 通过`);
    if (failed.length > 0) {
      console.error(`失败项：${failed.map((item) => item.name).join('；')}`);
      process.exitCode = 1;
    }
  } catch (error) {
    console.error(`会话恢复冒烟异常（步骤：${currentStep}）：${error?.message ?? error}`);
    process.exitCode = 1;
  } finally {
    clearTimeout(watchdog);
    for (const instance of [first, second, third]) {
      if (instance) {
        spawnSync('taskkill', ['/PID', String(instance.child.pid), '/T', '/F'], { stdio: 'ignore' });
        instance.client?.close?.();
      }
    }
    await delay(600);
    await removeWithRetry(runDataDir);
    await removeWithRetry(workDir);
  }
}

void main();
