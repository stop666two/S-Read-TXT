#!/usr/bin/env node
// P1-3 多光标与矩形选择 E2E（常驻套件）。
// 场景（M1–M13）：修饰键单击加/移除光标 / 多光标连续键入（单撤销步）/ 单步撤销还原 /
//   矩形拖选替换与列键入 / Esc 收起 / 多光标退格（单撤销步）/ 设置开关门控 / 截图。
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-multi.mjs [--exe <路径>] [--screenshot <路径>]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const screenshotPath = argValue(
  '--screenshot',
  join(root, 'docs', 'screenshots', 'phase-p1-multi.png'),
);
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '300000');

const workDir = join(tmpdir(), `srt-multi-${Date.now()}`);
const dataDir = join(workDir, 'data');
const testFile = join(workDir, 'multi-sample.txt');
const port = 9200 + Math.floor(Math.random() * 250);

const checks = [];
let currentStep = 'M0 启动';
function check(name, passed, detail = '') {
  checks.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

/** 带重试的删除（进程句柄释放/杀软扫描可能短暂锁定）。 */
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
  mkdirSync(dataDir, { recursive: true });
  writeFileSync(testFile, 'aaa\nbbb\nccc\nddd\neee', 'utf8');

  const child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });

  let client;
  try {
    currentStep = 'M0 等待 CDP';
    const wsUrl = await (async () => {
      const started = Date.now();
      for (;;) {
        try {
          return await findTarget(port);
        } catch {
          if (Date.now() - started > 20_000) throw new Error('CDP 目标未出现');
          await delay(250);
        }
      }
    })();
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

    const waitFor = async (fn, timeoutMs, label) => {
      const started = Date.now();
      for (;;) {
        const value = await fn();
        if (value) return value;
        if (Date.now() - started > timeoutMs) throw new Error(`等待超时：${label}`);
        await delay(120);
      }
    };

    const activeTab = async () =>
      evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return v.tabs.find((t) => t.tabId === v.activeTabId); })()`,
      );

    const rows = async (startRow, count) =>
      evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('get_rows', { tabId: (await window.__TAURI_INTERNALS__.invoke('list_tabs')).activeTabId, startRow: ${startRow}, count: ${count} }); return v.rows.map((r) => r.text); })()`,
      );

    const extraCount = () => evalJs(`document.querySelectorAll('.caret.extra').length`);
    const rectCount = () => evalJs(`document.querySelectorAll('.selection.rect').length`);

    /** 指定位置的屏幕坐标（用 Range 量出字符盒；offset==len 取行尾右侧）。 */
    const pointOf = (row, utf16) =>
      evalJs(
        `(() => {
          const node = document.querySelector('.row[data-row="${row}"]');
          if (!node) return null;
          const rect = node.getBoundingClientRect();
          // .txt 内可能以 Svelte 锚点注释/hl span 开头；用 TreeWalker 取首个文本节点
          const txtEl = node.querySelector('.txt') ?? node;
          const text = document.createTreeWalker(txtEl, NodeFilter.SHOW_TEXT).nextNode();
          if (!(text instanceof Text)) return { x: rect.left + 2, y: rect.top + rect.height / 2 };
          const offset = Math.min(${utf16}, text.length);
          const range = document.createRange();
          if (offset < text.length) {
            range.setStart(text, offset);
            range.setEnd(text, offset + 1);
          } else {
            range.setStart(text, offset);
            range.collapse(true);
          }
          const r = range.getBoundingClientRect();
          return {
            x: offset < text.length ? r.left + 1 : Math.max(r.left + 1, r.right - 1),
            y: rect.top + rect.height / 2,
          };
        })()`,
      );

    /** 修饰键（Alt=1）单击：加/移除附加光标。 */
    const altClick = async (row, utf16) => {
      const point = await pointOf(row, utf16);
      if (!point) throw new Error(`列点不存在：row=${row}`);
      await client.send('Input.dispatchMouseEvent', {
        type: 'mousePressed',
        x: point.x,
        y: point.y,
        button: 'left',
        clickCount: 1,
        modifiers: 1,
      });
      await client.send('Input.dispatchMouseEvent', {
        type: 'mouseReleased',
        x: point.x,
        y: point.y,
        button: 'left',
        clickCount: 1,
        modifiers: 1,
      });
      await delay(160);
    };

    /** 修饰键拖拽：矩形选择。 */
    const altDrag = async (fromRow, fromUtf16, toRow, toUtf16) => {
      const from = await pointOf(fromRow, fromUtf16);
      const to = await pointOf(toRow, toUtf16);
      if (!from || !to) throw new Error('矩形拖拽端点不存在');
      await client.send('Input.dispatchMouseEvent', {
        type: 'mousePressed',
        x: from.x,
        y: from.y,
        button: 'left',
        clickCount: 1,
        modifiers: 1,
      });
      for (let i = 1; i <= 5; i += 1) {
        const t = i / 5;
        await client.send('Input.dispatchMouseEvent', {
          type: 'mouseMoved',
          x: from.x + (to.x - from.x) * t,
          y: from.y + (to.y - from.y) * t,
          button: 'left',
          modifiers: 1,
        });
        await delay(30);
      }
      await client.send('Input.dispatchMouseEvent', {
        type: 'mouseReleased',
        x: to.x,
        y: to.y,
        button: 'left',
        clickCount: 1,
        modifiers: 1,
      });
      await delay(160);
    };

    const insertText = (text) => client.send('Input.insertText', { text });
    const pressKey = async (key, code, virtualKeyCode) => {
      await client.send('Input.dispatchKeyEvent', {
        type: 'keyDown',
        key,
        code,
        windowsVirtualKeyCode: virtualKeyCode,
      });
      await client.send('Input.dispatchKeyEvent', {
        type: 'keyUp',
        key,
        code,
        windowsVirtualKeyCode: virtualKeyCode,
      });
      await delay(160);
    };

    const plainClick = async (row, utf16) => {
      const point = await pointOf(row, utf16);
      await client.send('Input.dispatchMouseEvent', {
        type: 'mousePressed',
        x: point.x,
        y: point.y,
        button: 'left',
        clickCount: 1,
      });
      await client.send('Input.dispatchMouseEvent', {
        type: 'mouseReleased',
        x: point.x,
        y: point.y,
        button: 'left',
        clickCount: 1,
      });
      await delay(120);
    };

    currentStep = 'M0 就绪等待';
    await waitFor(() => evalJs(`!!(window.__srt && window.__TAURI_INTERNALS__)`), 20_000, '前端桥接');
    // 首启引导：等挂载后走真实用户路径关闭（不关会拦截全部鼠标事件）
    await dismissOnboarding(evalJs);
    await waitFor(() => evalJs(`!!document.querySelector('.empty .open-btn')`), 8_000, '欢迎页');

    currentStep = 'M1 打开并进入编辑';
    await evalJs(
      `(async () => { await window.__srt.openPath(${JSON.stringify(testFile)}); return true; })()`,
    );
    await waitFor(activeTab, 10_000, '标签出现');
    await evalJs(`(document.querySelector('[aria-label="切换编辑模式"]')?.click(), true)`);
    await waitFor(async () => ((await activeTab())?.editing ? true : null), 8_000, '编辑态');
    await waitFor(() => evalJs(`!!document.querySelector('textarea.input-proxy')`), 8_000, '编辑层');
    check('M1 编辑层挂载', (await activeTab())?.editing === true);

    currentStep = 'M2 修饰键单击加光标';
    await altClick(0, 1);
    await altClick(2, 1);
    await altClick(4, 1);
    const extras = await extraCount();
    check('M2 三个附加光标渲染', extras === 3, String(extras));

    currentStep = 'M3 多光标连续键入';
    await insertText('X');
    await delay(400);
    const afterX = await rows(0, 5);
    check(
      'M3a 四处同时插入 X',
      JSON.stringify(afterX) === JSON.stringify(['XaXaa', 'bbb', 'cXcc', 'ddd', 'eXee']),
      JSON.stringify(afterX),
    );
    check('M3b 光标列前进且保留附加光标', (await extraCount()) === 3);
    await insertText('Y');
    await delay(400);
    const afterY = await rows(0, 5);
    check(
      'M3c 连续多光标键入',
      afterY?.[0] === 'XYaYXaa' && afterY?.[2] === 'cXYcc',
      JSON.stringify(afterY),
    );

    currentStep = 'M4 单步撤销';
    // 直接走 undo_edit 命令（CDP 组合键注入 Ctrl+Z 在 WebView2 下不稳，命令路径等价且更可靠）
    await evalJs(
      `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); await window.__TAURI_INTERNALS__.invoke('undo_edit', { tabId: v.activeTabId }); return true; })()`,
    );
    await delay(400);
    const afterUndo1 = await rows(0, 5);
    check(
      'M4a 一次撤销回退 Y（保留 X 批次）',
      afterUndo1?.[0] === 'XaXaa' && afterUndo1?.[2] === 'cXcc',
      JSON.stringify(afterUndo1),
    );
    await evalJs(
      `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); await window.__TAURI_INTERNALS__.invoke('undo_edit', { tabId: v.activeTabId }); return true; })()`,
    );
    await delay(400);
    const afterUndo2 = await rows(0, 5);
    check(
      'M4b 二次撤销完全还原且不脏',
      JSON.stringify(afterUndo2) === JSON.stringify(['aaa', 'bbb', 'ccc', 'ddd', 'eee']) &&
        (await activeTab())?.dirty === false,
      JSON.stringify(afterUndo2),
    );

    currentStep = 'M5 矩形拖选替换';
    await altDrag(1, 0, 3, 1);
    const rects = await rectCount();
    check('M5a 矩形选择渲染', typeof rects === 'number' && rects >= 3, String(rects));
    await insertText('Z');
    await delay(400);
    const afterRect = await rows(0, 5);
    check(
      'M5b 矩形列替换',
      JSON.stringify(afterRect) === JSON.stringify(['aaa', 'Zbb', 'Zcc', 'Zdd', 'eee']),
      JSON.stringify(afterRect),
    );
    check('M5c 矩形转为列光标', (await extraCount()) === 2 && (await rectCount()) === 0);

    currentStep = 'M6 列键入延续';
    await insertText('W');
    await delay(400);
    const afterW = await rows(0, 5);
    check('M6 列光标连续键入', afterW?.[1] === 'ZWbb' && afterW?.[3] === 'ZWdd', JSON.stringify(afterW));

    currentStep = 'M7 Esc 收起';
    await pressKey('Escape', 'Escape', 27);
    check('M7 Esc 清空多光标', (await extraCount()) === 0 && (await rectCount()) === 0);

    currentStep = 'M8 多光标退格（单撤销步）';
    await plainClick(1, 2);
    await altClick(3, 2);
    await pressKey('Backspace', 'Backspace', 8);
    await delay(400);
    const afterBackspace = await rows(0, 5);
    check(
      'M8a 两处同步退格',
      afterBackspace?.[1] === 'Zbb' && afterBackspace?.[3] === 'Zdd',
      JSON.stringify(afterBackspace),
    );
    await evalJs(
      `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); await window.__TAURI_INTERNALS__.invoke('undo_edit', { tabId: v.activeTabId }); return true; })()`,
    );
    await delay(400);
    const afterUndo3 = await rows(0, 5);
    check('M8b 一次撤销还原两处', afterUndo3?.[1] === 'ZWbb' && afterUndo3?.[3] === 'ZWdd', JSON.stringify(afterUndo3));

    currentStep = 'M9 修饰键单击切换';
    await altClick(0, 1);
    const toggledOn = await extraCount();
    await altClick(0, 1);
    const toggledOff = await extraCount();
    check('M9 同位置再点移除', toggledOn === 1 && toggledOff === 0, `${toggledOn}→${toggledOff}`);

    currentStep = 'M10 设置门控禁用';
    const snapshot = await evalJs(`window.__TAURI_INTERNALS__.invoke('get_settings')`);
    const shortcuts = snapshot.shortcuts?.bindings ?? snapshot.shortcuts;
    snapshot.app.editor.multiCursor.enabled = false;
    await evalJs(
      `window.__TAURI_INTERNALS__.invoke('save_settings', { request: { app: ${JSON.stringify(
        snapshot.app,
      )}, reader: ${JSON.stringify(snapshot.reader)}, shortcuts: ${JSON.stringify(shortcuts)} } })`,
    );
    await delay(500);
    await altClick(2, 1);
    const gated = await extraCount();
    check('M10a 禁用后修饰键点击不产生光标', gated === 0, String(gated));
    snapshot.app.editor.multiCursor.enabled = true;
    await evalJs(
      `window.__TAURI_INTERNALS__.invoke('save_settings', { request: { app: ${JSON.stringify(
        snapshot.app,
      )}, reader: ${JSON.stringify(snapshot.reader)}, shortcuts: ${JSON.stringify(shortcuts)} } })`,
    );
    await delay(500);
    await altClick(2, 1);
    const reenabled = await extraCount();
    check('M10b 恢复启用后生效', reenabled === 1, String(reenabled));
    await pressKey('Escape', 'Escape', 27);

    currentStep = 'M11 截图';
    await altClick(0, 1);
    await altClick(2, 1);
    await altDrag(1, 0, 3, 2);
    const shot = await client.send('Page.captureScreenshot', { format: 'png' });
    mkdirSync(dirname(screenshotPath), { recursive: true });
    writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'));
    check('M11 截图保存', existsSync(screenshotPath), screenshotPath);
  } finally {
    clearTimeout(watchdog);
    try {
      if (child.pid) {
        spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
      }
    } catch {
      // 忽略清理失败
    }
    await removeWithRetry(workDir);
  }

  const failed = checks.filter((item) => !item.passed);
  console.log(`\n结果：${checks.length - failed.length}/${checks.length} 通过`);
  if (failed.length > 0) {
    for (const item of failed) console.log(`FAILED: ${item.name} ${item.detail}`);
    process.exitCode = 1;
  }
}

main().catch((error) => {
  clearTimeout(watchdog);
  console.error(`未捕获错误（${currentStep}）：${error.message}`);
  process.exitCode = 1;
});
