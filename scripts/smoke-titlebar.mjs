#!/usr/bin/env node
// 自定义标题栏冒烟：真实应用 + CDP 驱动。
// 覆盖：标题（文件名 - 应用名）、拖拽区与三按钮存在、按钮最大化/还原、双击最大化/还原、
//       浅色/深色主题截图、最小化（最后一步，document.hidden 断言）。
//
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-titlebar.mjs [--exe <路径>] [--port 9225]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, findTarget, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const port = Number(argValue('--port', String(9200 + Math.floor(Math.random() * 600))));
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '240000');

const workDir = join(process.env.TEMP ?? '.', 'srt-smoke-titlebar');
const testFile = join(workDir, 'title-sample.txt');
const runDataDir = join(workDir, `data-${Date.now()}`);

const checks = [];
let currentStep = 'T0 启动';
function check(name, passed, detail = '') {
  checks.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
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

const watchdog = setTimeout(() => {
  console.error(`看门狗超时（${watchdogMs}ms），最后步骤：${currentStep}`);
  process.exit(4);
}, watchdogMs);

async function main() {
  if (!existsSync(exePath)) {
    console.error(`可执行文件不存在：${exePath}（先运行 npm run tauri build -- --debug --no-bundle）`);
    process.exit(2);
  }
  mkdirSync(workDir, { recursive: true });
  rmSync(testFile, { force: true });
  writeFileSync(testFile, '标题栏测试\n第二行\n', 'utf8');

  const child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: runDataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });

  let client;
  try {
    const wsUrl = await findTarget(port);
    client = await createClient(wsUrl);
    const evalJs = async (expression) => {
      const result = await client.send('Runtime.evaluate', {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
      if (result.exceptionDetails) {
        throw new Error(`页面执行异常：${result.exceptionDetails.text}`);
      }
      return result.result?.value;
    };

    let ready = false;
    for (let attempt = 0; attempt < 180; attempt += 1) {
      ready = (await evalJs('!!window.__srt?.openPath && !!window.__TAURI_INTERNALS__')) === true;
      if (ready) break;
      await delay(250);
    }
    if (!ready) throw new Error('前端未就绪（window.__srt 未注入）');

    // T1：标题栏随打开文件更新（自定义标题栏 + document.title）
    currentStep = 'T1 标题显示';
    await evalJs(`window.__srt.openPath(${JSON.stringify(testFile)})`);
    const title = await waitForValue(async () => {
      const bar = await evalJs(`document.querySelector('.app-title')?.textContent ?? ''`);
      const doc = await evalJs(`document.title`);
      return bar === 'title-sample.txt - S-Read-TXT' && doc === bar ? bar : null;
    }, 8000);
    check('T1 标题显示「文件名 - S-Read-TXT」', title !== null, title ?? '(超时)');

    // T2：拖拽区与三个窗口按钮存在
    currentStep = 'T2 结构断言';
    const structure = await evalJs(
      `(() => ({
         drag: document.querySelector('[data-tauri-drag-region]') !== null,
         min: document.querySelector('[aria-label="最小化"]') !== null,
         max: document.querySelector('[aria-label="最大化"]') !== null,
         close: document.querySelector('[aria-label="关闭"]') !== null,
       }))()`,
    );
    check(
      'T2 拖拽区与最小化/最大化/关闭按钮齐备',
      structure?.drag && structure?.min && structure?.max && structure?.close,
      JSON.stringify(structure ?? {}),
    );

    // T3：按钮最大化（宽度接近屏幕可用宽度，按钮语义翻转）
    currentStep = 'T3 按钮最大化';
    const before = await evalJs(`window.outerWidth`);
    await evalJs(`(document.querySelector('[aria-label="最大化"]')?.click(), true)`);
    const maximized = await waitForValue(async () => {
      const width = await evalJs(`window.outerWidth`);
      const avail = await evalJs(`screen.availWidth`);
      const restoreBtn = await evalJs(
        `document.querySelector('[aria-label="还原"]') !== null`,
      );
      return width >= avail - 30 && restoreBtn === true ? width : null;
    }, 6000);
    check('T3 按钮最大化（按钮变「还原」）', maximized !== null, `outerWidth=${maximized} (初始 ${before})`);

    // T4：按钮还原
    currentStep = 'T4 按钮还原';
    await evalJs(`(document.querySelector('[aria-label="还原"]')?.click(), true)`);
    const restored = await waitForValue(async () => {
      const width = await evalJs(`window.outerWidth`);
      const maxBtn = await evalJs(`document.querySelector('[aria-label="最大化"]') !== null`);
      return Math.abs(width - before) <= 30 && maxBtn === true ? width : null;
    }, 6000);
    check('T4 按钮还原（宽度复原）', restored !== null, `outerWidth=${restored} (初始 ${before})`);

    // T5：双击拖拽区最大化 / 还原
    currentStep = 'T5 双击切换';
    await evalJs(
      `(document.querySelector('[data-tauri-drag-region]')?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })), true)`,
    );
    const dblMax = await waitForValue(async () => {
      const avail = await evalJs(`screen.availWidth`);
      const width = await evalJs(`window.outerWidth`);
      return width >= avail - 30 ? width : null;
    }, 6000);
    await evalJs(
      `(document.querySelector('[data-tauri-drag-region]')?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })), true)`,
    );
    const dblRestore = await waitForValue(async () => {
      const width = await evalJs(`window.outerWidth`);
      return Math.abs(width - before) <= 30 ? width : null;
    }, 6000);
    check('T5 双击拖拽区最大化/还原', dblMax !== null && dblRestore !== null);

    // T6：浅色/深色主题截图（人工核验配色）
    currentStep = 'T6 主题截图';
    const shots = [];
    for (const theme of ['light', 'dark']) {
      await evalJs(`(document.documentElement.dataset.theme = '${theme}', true)`);
      await delay(400);
      const shot = await client.send('Page.captureScreenshot', { format: 'png' });
      const path = join(root, 'docs', 'screenshots', `phase5-titlebar-${theme}.png`);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, Buffer.from(shot.data, 'base64'));
      shots.push(path);
    }
    check('T6 主题截图已保存', shots.length === 2, shots.join(' / '));

    // T7：最小化（最后一步；WebView2 最小化不改变 visibilityState，经 is_minimized 断言）
    currentStep = 'T7 最小化';
    await evalJs(`(document.querySelector('[aria-label="最小化"]')?.click(), true)`);
    const minimized = await waitForValue(async () => {
      const hidden = await evalJs(`document.visibilityState === 'hidden'`);
      if (hidden === true) return true;
      const result = await evalJs(
        `window.__TAURI_INTERNALS__.invoke('plugin:window|is_minimized', { label: 'main' }).then((v) => v === true).catch(() => false)`,
      );
      return result === true ? true : null;
    }, 6000);
    check('T7 最小化（is_minimized 为真）', minimized === true);
  } finally {
    client?.close();
    if (child.pid) {
      // 仅回收本应用进程树；严禁按 msedgewebview2 名称杀进程
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    }
    await removeWithRetry(testFile);
    await removeWithRetry(runDataDir);
  }

  clearTimeout(watchdog);
  const failed = checks.filter((item) => !item.passed);
  console.log(`\n标题栏冒烟结果：${checks.length - failed.length}/${checks.length} 通过`);
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((error) => {
  clearTimeout(watchdog);
  console.error(`标题栏冒烟失败（${currentStep}）：${error?.message ?? error}`);
  process.exit(3);
});
