#!/usr/bin/env node
// 中文输入法（IME）冒烟（阶段 4c）：真实应用 + CDP 输入法管线模拟。
// 覆盖：组合开始（preedit 悬浮显示）→ 组合更新（候选更新）→ 提交（入库）→
//       取消组合（无副作用）→ 保存后磁盘为 UTF-8 中文。
//
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-ime.mjs [--exe <路径>] [--port 9224] [--screenshot <路径>]
// 说明：Input.imeSetComposition 驱动 Chromium 输入法管线（与真实输入法一致的
//        compositionstart/update 事件序列）；提交经组合中的 Input.insertText 触发
//       compositionend（本 WebView2 协议面不含 imeCommitComposition，已探针实测）。

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget, openPathDone, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const port = Number(argValue('--port', String(9200 + Math.floor(Math.random() * 600))));
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '240000');

const workDir = join(process.env.TEMP ?? '.', 'srt-smoke-ime');
const testFile = join(workDir, 'ime-sample.txt');
const runDataDir = join(workDir, `data-${Date.now()}`);

const checks = [];
let currentStep = 'I0 启动';
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
  rmSync(`${testFile}.bak`, { force: true });
  writeFileSync(testFile, 'alpha\nbeta\n', 'utf8');

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
    const rowText = (row) => evalJs(`document.querySelector('.row[data-row="${row}"]')?.textContent ?? ''`);
    const activeTab = async () =>
      evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return v.tabs.find((t) => t.tabId === v.activeTabId); })()`,
      );

    // 就绪护栏
    let ready = false;
    for (let attempt = 0; attempt < 180; attempt += 1) {
      ready = (await evalJs('!!window.__srt?.openPath && !!window.__TAURI_INTERNALS__')) === true;
      if (ready) break;
      await delay(250);
    }
    if (!ready) throw new Error('前端未就绪（window.__srt 未注入）');
    await dismissOnboarding(evalJs);

    // I1：打开并进入编辑
    currentStep = 'I1 打开并进入编辑';
    await evalJs(openPathDone(testFile));
    const firstRow = await waitForValue(async () => {
      const text = await rowText(0);
      return text === 'alpha' ? text : null;
    }, 8000);
    await evalJs(`(document.querySelector('[aria-label="切换编辑模式"]')?.click(), true)`);
    const editing = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.editing ? tab : null;
    }, 8000);
    check('I1 打开文件并进入编辑', firstRow === 'alpha' && editing !== null);

    // I2：组合开始（拼音候选 "ni"）→ preedit 悬浮显示
    currentStep = 'I2 组合开始';
    await evalJs(`(document.querySelector('textarea.input-proxy')?.focus(), true)`);
    await client.send('Input.imeSetComposition', {
      text: 'ni',
      selectionStart: 2,
      selectionEnd: 2,
    });
    const preedit1 = await waitForValue(async () => {
      const text = await evalJs(`document.querySelector('.preedit')?.textContent ?? ''`);
      return text === 'ni' ? text : null;
    }, 5000);
    check('I2 组合中显示 preedit（ni）', preedit1 === 'ni', preedit1 ?? '(超时)');

    // I3：组合更新（选中候选 "你"）
    currentStep = 'I3 组合更新';
    await client.send('Input.imeSetComposition', {
      text: '你',
      selectionStart: 1,
      selectionEnd: 1,
    });
    const preedit2 = await waitForValue(async () => {
      const text = await evalJs(`document.querySelector('.preedit')?.textContent ?? ''`);
      return text === '你' ? text : null;
    }, 5000);
    check('I3 候选更新显示（你）', preedit2 === '你', preedit2 ?? '(超时)');

    // I4：提交组合 → 文本入库、preedit 消失、变脏
    // 注：本 WebView2 的 CDP 无 Input.imeCommitComposition；组合中的 Input.insertText
    //     会被 Blink 路由为「提交当前组合」并触发 compositionend（探针实测事件序列）。
    currentStep = 'I4 提交组合';
    await client.send('Input.insertText', { text: '你' });
    const committed = await waitForValue(async () => {
      const text = await rowText(0);
      const preeditGone = await evalJs(`document.querySelector('.preedit') === null`);
      const tab = await activeTab();
      return text === '你alpha' && preeditGone === true && tab?.dirty === true ? text : null;
    }, 8000);
    check('I4 提交入库（你alpha，脏态）', committed === '你alpha', committed ?? `row0=${await rowText(0)}`);

    // I5：取消组合（设置组合后清空）→ 无插入、脏态不变
    currentStep = 'I5 取消组合';
    await client.send('Input.imeSetComposition', {
      text: 'hao',
      selectionStart: 3,
      selectionEnd: 3,
    });
    await delay(250);
    await client.send('Input.imeSetComposition', {
      text: '',
      selectionStart: 0,
      selectionEnd: 0,
    });
    await delay(600);
    const afterCancel = await evalJs(
      `(async () => { const row = document.querySelector('.row[data-row="0"]')?.textContent ?? ''; const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return { row, dirty: v.tabs.find((t) => t.tabId === v.activeTabId)?.dirty }; })()`,
    );
    check(
      'I5 取消组合无副作用',
      afterCancel?.row === '你alpha' && afterCancel?.dirty === true,
      JSON.stringify(afterCancel ?? {}),
    );

    // I6：保存 → 磁盘为 UTF-8 中文
    currentStep = 'I6 保存校验磁盘';
    await evalJs(`(document.querySelector('[aria-label="保存"]')?.click(), true)`);
    const dialogSeen = await waitForValue(
      async () => (await evalJs(`document.querySelector('[role="dialog"]') !== null`)) || null,
      5000,
    );
    await evalJs(
      `(() => { const dlg = document.querySelector('[role="dialog"]'); const btn = [...dlg.querySelectorAll('button')].find((b) => b.textContent.trim() === '保存'); btn?.click(); return true; })()`,
    );
    const saved = await waitForValue(async () => {
      const tab = await activeTab();
      if (tab?.dirty) return null;
      const disk = readFileSync(testFile, 'utf8');
      return disk === '你alpha\nbeta\n' ? disk : null;
    }, 15000);
    check(
      'I6 保存后磁盘为 UTF-8 中文',
      dialogSeen === true && saved === '你alpha\nbeta\n',
      `disk=${JSON.stringify(readFileSync(testFile, 'utf8'))}`,
    );

    // I7：截图（编辑态，接收键盘焦点后可人工复核 IME 候选窗跟随）
    try {
      const shot = await client.send('Page.captureScreenshot', { format: 'png' });
      const screenshotPath = argValue('--screenshot', join(root, 'docs', 'screenshots', 'phase4c-ime.png'));
      mkdirSync(dirname(screenshotPath), { recursive: true });
      writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'));
      check('I7 截图已保存', true, screenshotPath);
    } catch (error) {
      check('I7 截图已保存', false, String(error));
    }
  } finally {
    client?.close();
    if (child.pid) {
      // 仅回收本应用进程树；严禁按 msedgewebview2 名称杀进程
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    }
    await removeWithRetry(testFile);
    await removeWithRetry(`${testFile}.bak`);
    await removeWithRetry(runDataDir);
  }

  clearTimeout(watchdog);
  const failed = checks.filter((item) => !item.passed);
  console.log(`\nIME 冒烟结果：${checks.length - failed.length}/${checks.length} 通过`);
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((error) => {
  clearTimeout(watchdog);
  console.error(`IME 冒烟失败（${currentStep}）：${error?.message ?? error}`);
  process.exit(3);
});
