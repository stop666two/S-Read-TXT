#!/usr/bin/env node
// 快捷键引擎 E2E（阶段 5a）：默认方案逐项验证（真实 CDP 键盘事件注入）。
// 覆盖：固定键 Ctrl+1~9、标签循环 Ctrl+Tab/Ctrl+Shift+Tab、Ctrl+W 关闭、
//       翻页与首尾（PgDn/PgUp/Home/End）、F11 全屏、Ctrl+E 编辑切换、Ctrl+F 查找条、Ctrl+O 原生对话框。
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-shortcuts.mjs [--exe <路径>] [--port 9227]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDialogOps } from './lib/dialog.mjs';
import { argValue, createClient, delay, findTarget, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const port = Number(argValue('--port', String(9200 + Math.floor(Math.random() * 600))));
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '180000');

const workDir = join(process.env.TEMP ?? '.', 'srt-smoke-shortcuts');
const mainFile = join(workDir, 'keys-main.txt');
const secondFile = join(workDir, 'keys-second.txt');
const thirdFile = join(workDir, 'keys-third.txt');
const runDataDir = join(workDir, `data-${Date.now()}`);

const checks = [];
let currentStep = 'K0 启动';
function check(name, passed, detail = '') {
  checks.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

const watchdog = setTimeout(() => {
  console.error(`看门狗超时（${watchdogMs}ms），最后步骤：${currentStep}`);
  process.exit(4);
}, watchdogMs);

/** 删除目录（含重试：进程句柄释放有延迟） */
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

async function main() {
  if (!existsSync(exePath)) {
    console.error(`可执行文件不存在：${exePath}（先运行 npm run tauri build -- --debug --no-bundle）`);
    process.exit(2);
  }
  mkdirSync(workDir, { recursive: true });
  writeFileSync(mainFile, Array.from({ length: 400 }, (_, i) => `line-${i + 1}`).join('\n') + '\n', 'utf8');
  writeFileSync(secondFile, 'second\n', 'utf8');
  writeFileSync(thirdFile, 'third\n', 'utf8');

  const child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: runDataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });

  /** 原生对话框探测/关闭（共享实现） */
  const { dialogOp, waitDialog } = createDialogOps(child.pid);

  let client;
  try {
    client = await createClient(await findTarget(port));
    const evalJs = async (expression) => {
      const result = await client.send('Runtime.evaluate', {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
      if (result.exceptionDetails) throw new Error(`页面脚本异常：${result.exceptionDetails.text}`);
      return result.result?.value;
    };
    await waitForValue(async () => {
      const ready = await evalJs(
        '(() => !!(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke && window.__srt && window.__srt.openPath))()',
      );
      return ready ? true : null;
    }, 30000);

    /** CDP 键盘注入（rawKeyDown 才会投递到页面；modifiers：Alt=1 Ctrl=2 Shift=8） */
    const press = async (key, code, vk, modifiers = 0) => {
      const base = { key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
      await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...base });
      await client.send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
      await delay(140);
    };

    const tabsInfo = () =>
      evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return { count: v.tabs.length, names: v.tabs.map((t) => t.name) }; })()`,
      );
    const activeTab = () =>
      evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return v.tabs.find((t) => t.tabId === v.activeTabId) ?? null; })()`,
      );
    const scrollTop = () => evalJs(`document.querySelector('.reader')?.scrollTop ?? -1`);
    const scrollMax = () =>
      evalJs(
        `(() => { const el = document.querySelector('.reader'); return el ? el.scrollHeight - el.clientHeight : 0; })()`,
      );

    // K1 打开三个文件
    currentStep = 'K1 打开三个文件';
    for (const file of [mainFile, secondFile, thirdFile]) {
      await evalJs(`window.__srt.openPath(${JSON.stringify(file)})`);
      await delay(450);
    }
    let info = await tabsInfo();
    check('K1 三个标签已打开', info.count === 3, `count=${info.count}`);

    // K2 固定键 Ctrl+2 / Ctrl+3（不参与自定义）
    currentStep = 'K2 固定标签跳转';
    await press('2', 'Digit2', 50, 2);
    let active = await activeTab();
    check('K2a Ctrl+2 跳到第二个标签', active?.name === 'keys-second.txt', `active=${active?.name}`);
    await press('3', 'Digit3', 51, 2);
    active = await activeTab();
    check('K2b Ctrl+3 跳到第三个标签', active?.name === 'keys-third.txt', `active=${active?.name}`);

    // K3 循环切换
    currentStep = 'K3 标签循环';
    await press('Tab', 'Tab', 9, 2);
    active = await activeTab();
    check('K3a Ctrl+Tab 循环到下一个', active?.name === 'keys-main.txt', `active=${active?.name}`);
    await press('Tab', 'Tab', 9, 10);
    active = await activeTab();
    check('K3b Ctrl+Shift+Tab 循环到上一个', active?.name === 'keys-third.txt', `active=${active?.name}`);

    // K4 Ctrl+W 关闭活动标签（第三个）
    currentStep = 'K4 关闭标签';
    await press('w', 'KeyW', 87, 2);
    await delay(450);
    info = await tabsInfo();
    check('K4 Ctrl+W 关闭活动标签', info.count === 2, `count=${info.count}`);

    // K5 翻页与首尾（Ctrl+1 选中 400 行的主文件）
    currentStep = 'K5 翻页与首尾';
    await press('1', 'Digit1', 49, 2);
    await delay(400);
    const before = await scrollTop();
    await press('PageDown', 'PageDown', 34);
    const afterDown = await scrollTop();
    check('K5a PgDn 向下翻页', afterDown > before, `${before}→${afterDown}`);
    await press('End', 'End', 35);
    await delay(400);
    const afterEnd = await scrollTop();
    const max = await scrollMax();
    check('K5b End 跳到结尾', max > 0 && afterEnd > max * 0.7, `top=${afterEnd} max=${max}`);
    await press('Home', 'Home', 36);
    await delay(400);
    check('K5c Home 跳到开头', Math.abs(await scrollTop()) <= 1);

    // K6 F11 全屏开/关
    currentStep = 'K6 全屏';
    await press('F11', 'F11', 122);
    await delay(500);
    const fullOn = await evalJs(
      `(async () => await window.__TAURI_INTERNALS__.invoke('plugin:window|is_fullscreen', { label: 'main' }))()`,
    );
    check('K6a F11 进入全屏', fullOn === true, `isFullscreen=${fullOn}`);
    await press('F11', 'F11', 122);
    await delay(500);
    const fullOff = await evalJs(
      `(async () => await window.__TAURI_INTERNALS__.invoke('plugin:window|is_fullscreen', { label: 'main' }))()`,
    );
    check('K6b F11 退出全屏', fullOff === false, `isFullscreen=${fullOff}`);

    // K7 编辑切换与查找条
    currentStep = 'K7 编辑与查找';
    await press('e', 'KeyE', 69, 2);
    await waitForValue(async () => ((await activeTab())?.editing === true ? true : null), 5000);
    check('K7a Ctrl+E 进入编辑模式', (await activeTab())?.editing === true);
    await press('f', 'KeyF', 70, 2);
    await waitForValue(async () => ((await evalJs(`!!document.querySelector('.find-bar')`)) ? true : null), 5000);
    check('K7b Ctrl+F 打开查找条', await evalJs(`!!document.querySelector('.find-bar')`));
    await press('Escape', 'Escape', 27);
    await delay(350);
    check('K7c Esc 关闭查找条', !(await evalJs(`!!document.querySelector('.find-bar')`)));
    await press('e', 'KeyE', 69, 2);
    await waitForValue(async () => ((await activeTab())?.editing === false ? true : null), 5000);
    check('K7d Ctrl+E 退出编辑模式', (await activeTab())?.editing === false);

    // K8 Ctrl+O 原生打开对话框（探针检测 + WM_CLOSE 关闭）
    currentStep = 'K8 打开对话框';
    await press('o', 'KeyO', 79, 2);
    const dialogShown = await waitDialog(true);
    check('K8a Ctrl+O 弹出打开对话框', dialogShown === true);
    if (dialogShown) dialogOp('close');
    const dialogClosed = dialogShown ? await waitDialog(false) : false;
    check('K8b 对话框已关闭', dialogClosed === true);
    await delay(300);

    // 汇总
    const failed = checks.filter((item) => !item.passed);
    console.log(`\n快捷键冒烟：${checks.length - failed.length}/${checks.length} 通过`);
    if (failed.length > 0) {
      console.error(`失败项：${failed.map((item) => item.name).join('；')}`);
      process.exitCode = 1;
    }
  } catch (error) {
    console.error(`快捷键冒烟异常（步骤：${currentStep}）：${error?.message ?? error}`);
    process.exitCode = 1;
  } finally {
    clearTimeout(watchdog);
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    await delay(500);
    await removeWithRetry(runDataDir);
    await removeWithRetry(workDir);
    client?.close?.();
  }
}

void main();
