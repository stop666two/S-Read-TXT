#!/usr/bin/env node
// 自定义标题栏冒烟：真实应用 + CDP 驱动。
// 覆盖：标题（文件名 - 应用名）、拖拽区与三按钮存在、按钮最大化/还原、双击最大化/还原、
//       浅色/深色主题截图、遮罩让位（引导弹窗打开时标题栏可命中）、真实点击最小化（最后一步）。
//
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-titlebar.mjs [--exe <路径>] [--port 9225]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, findTarget, openPathDone, waitForValue } from './lib/smoke-cdp.mjs';

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
    await evalJs(openPathDone(testFile));
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
      await evalJs(`(window.__srt.setTheme('${theme}'), true)`);
      await delay(450);
      const shot = await client.send('Page.captureScreenshot', { format: 'png' });
      const path = join(root, 'docs', 'screenshots', `phase5-titlebar-${theme}.png`);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, Buffer.from(shot.data, 'base64'));
      shots.push(path);
    }
    check('T6 主题截图已保存', shots.length === 2, shots.join(' / '));

    // T7：引导弹窗打开时标题栏不被遮挡（本次修复的回归：遮罩顶部让位 --h-titlebar）
    currentStep = 'T7 遮罩不遮标题栏';
    const hit = await evalJs(
      `(() => {
         const overlay = document.querySelector('[role="dialog"][aria-label="使用向导"]') !== null;
         const el = document.elementFromPoint(Math.floor(window.innerWidth / 2), 6);
         return JSON.stringify({ overlay, drag: el !== null && el.closest('[data-tauri-drag-region]') !== null });
       })()`,
    );
    const hitInfo = JSON.parse(hit ?? '{}');
    check(
      'T7 引导弹窗打开时标题栏可命中（遮罩让位）',
      hitInfo.overlay === true && hitInfo.drag === true,
      hit ?? '',
    );

    // T9：标题栏「打开设置」齿轮 → 设置窗口打开 → 关闭（B 批次新增入口回归）
    currentStep = 'T9 标题栏设置入口';
    await evalJs(
      `(() => { const o = document.querySelector('[role="dialog"][aria-label="使用向导"]');
         if (!o) return true; const c = o.querySelector('.dont-show input'); if (c && !c.checked) c.click();
         [...o.querySelectorAll('button')].find((b) => b.textContent.includes('开始使用'))?.click(); return true; })()`,
    );
    await delay(300);
    const gearHit = await evalJs(
      `(() => { const b = document.querySelector('.title-bar [aria-label="打开设置"]'); if (!b) return false; b.click(); return true; })()`,
    );
    const settingsWs = await waitForValue(async () => {
      const found = await findTarget(port, 'settings.html').catch(() => null);
      return found ?? null;
    }, 8000);
    check('T9 标题栏设置齿轮 → 设置窗口打开', gearHit === true && settingsWs !== null);
    if (settingsWs) {
      const setClient = await createClient(settingsWs);
      /** 等待设置窗口的关闭按钮就绪并尝试关闭（负载下首次点击可能丢失，随后升级为按标签直连关闭） */
      const waitGone = (ms) =>
        waitForValue(async () => {
          const still = await findTarget(port, 'settings.html').catch(() => null);
          return still ? null : true;
        }, ms);
      await waitForValue(async () => {
        const probe = await setClient.send('Runtime.evaluate', {
          expression: `!!document.querySelector('.title-bar button[aria-label="关闭"]')`,
          returnByValue: true,
        });
        return probe.result?.value === true ? true : null;
      }, 8000);
      await setClient.send('Page.bringToFront').catch(() => {});
      await setClient
        .send('Runtime.evaluate', {
          expression: `document.querySelector('.title-bar button[aria-label="关闭"]')?.click() ?? true`,
          returnByValue: true,
        })
        .catch(() => {});
      let closed = await waitGone(5000);
      if (!closed) {
        await evalJs(
          `window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'settings' }).then(() => true).catch(() => false)`,
        ).catch(() => {});
        closed = await waitGone(8000);
      }
      setClient.close();
      check('T9b 设置窗口已关闭', closed === true);
    } else {
      check('T9b 设置窗口已关闭', false, '未打开');
    }

    // T8：真实鼠标点击标题栏「最小化」（坐标级输入验证按钮未被遮挡；JS .click() 会绕过命中测试）
    currentStep = 'T8 最小化（真实点击）';
    const rectJson = await evalJs(
      `(() => { const b = document.querySelector('[aria-label="最小化"]'); if (!b) return null; const r = b.getBoundingClientRect(); return JSON.stringify({ x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) }); })()`,
    );
    if (!rectJson) throw new Error('找不到最小化按钮');
    const { x, y } = JSON.parse(rectJson);
    await client.send('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 });
    await client.send('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', clickCount: 1 });
    const minimized = await waitForValue(async () => {
      const hidden = await evalJs(`document.visibilityState === 'hidden'`);
      if (hidden === true) return true;
      const result = await evalJs(
        `window.__TAURI_INTERNALS__.invoke('plugin:window|is_minimized', { label: 'main' }).then((v) => v === true).catch(() => false)`,
      );
      return result === true ? true : null;
    }, 6000);
    check('T8 真实点击最小化（按钮未被遮挡）', minimized === true);
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
