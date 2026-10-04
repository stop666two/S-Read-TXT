#!/usr/bin/env node
// P2-3 标注（书签/高亮/注释）E2E（常驻套件）。
// 场景（A1–A12）：阅读态书签 → 编辑态高亮 → 注释/待办 → 面板（分区/勾选/删除）→
//   重启持久化 → 编辑后锚点重定位 → 清除 → 截图。
// 前置：已构建 debug 可执行文件（node scripts/dev.mjs build）。
// 用法：node scripts/smoke-annotations.mjs [--exe <路径>] [--screenshot <路径>]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const screenshotPath = argValue('--screenshot', join(root, 'docs', 'screenshots', 'p2-annotations.png'));
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '300000');

const workDir = join(tmpdir(), `srt-annot-${Date.now()}`);
const dataDir = join(workDir, 'data');
const testFile = join(workDir, 'annot-sample.txt');
const port = 9900 + Math.floor(Math.random() * 90);

const checks = [];
let currentStep = 'A0 启动';
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
    console.error(`可执行文件不存在：${exePath}（先运行 node scripts/dev.mjs build）`);
    process.exit(2);
  }
  mkdirSync(workDir, { recursive: true });
  mkdirSync(dataDir, { recursive: true });
  writeFileSync(testFile, 'line-1\nline-2\nline-3\nline-4\nline-5', 'utf8');

  let child = null;
  let client = null;
  const spawnApp = () => {
    child = spawn(exePath, [], {
      env: {
        ...process.env,
        SRT_DATA_DIR: dataDir,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
      },
      stdio: 'ignore',
    });
  };
  const waitClient = async () => {
    const started = Date.now();
    for (;;) {
      try {
        const wsUrl = await findTarget(port);
        client = await createClient(wsUrl);
        return;
      } catch {
        if (Date.now() - started > 20_000) throw new Error('CDP 目标未出现');
        await delay(250);
      }
    }
  };
  const killApp = async () => {
    try {
      if (child?.pid) spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    } catch {
      // 忽略
    }
    client = null;
    await delay(600);
  };

  spawnApp();

  try {
    await waitClient();
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
    const key = async (keyName, vk, modifiers = 0) => {
      await client.send('Input.dispatchKeyEvent', {
        type: 'rawKeyDown',
        key: keyName,
        windowsVirtualKeyCode: vk,
        modifiers,
      });
      await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key: keyName, windowsVirtualKeyCode: vk, modifiers });
      await delay(60);
    };
    const invoke = async (cmd, args = {}) =>
      evalJs(
        `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`,
      );
    const activeTabId = async () => {
      const view = await invoke('list_tabs');
      return view.activeTabId;
    };
    const annotationsOf = async () => {
      const tabId = await activeTabId();
      return invoke('list_annotations', { tabId });
    };
    const countOf = async (selector) =>
      evalJs(`document.querySelectorAll(${JSON.stringify(selector)}).length`);

    /** 等前端脚本挂载完成（CDP 目标出现可能早于页面脚本）。 */
    const waitReady = async (evalFn) => {
      const started = Date.now();
      for (;;) {
        try {
          const ok = await evalFn(`!!(window.__srt && window.__srt.openPath && document.querySelector('.menu-bar'))`);
          if (ok) return;
        } catch {
          // 忽略尽早失败
        }
        if (Date.now() - started > 20_000) throw new Error('前端未就绪');
        await delay(250);
      }
    };

    /** 打开菜单 → 悬停/点击子菜单 → 返回子菜单条目（按序点击用） */
    const openSubmenu = async (menuText, subText) => {
      await evalJs(`(() => {
        const menu = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === ${JSON.stringify(menuText)});
        menu?.click();
        return !!menu;
      })()`);
      await delay(180);
      await evalJs(`(() => {
        const wrap = [...document.querySelectorAll('.submenu-wrap')].find((w) => w.textContent.includes(${JSON.stringify(subText)}));
        wrap?.dispatchEvent(new MouseEvent('mouseenter'));
        return !!wrap;
      })()`);
      await delay(180);
    };
    const clickFlyoutItem = async (index) =>
      evalJs(`(() => {
        const wraps = [...document.querySelectorAll('.submenu-wrap')];
        const wrap = wraps.find((w) => w.textContent.includes('标注'));
        const item = wrap?.querySelectorAll('.flyout .item')[${index}];
        item?.click();
        return !!item;
      })()`);

    await waitReady(evalJs);
    await dismissOnboarding(evalJs);

    // ---- A1 打开样本（阅读态） ----
    currentStep = 'A1 打开样本';
    const opened = await evalJs(
      `(async () => { await window.__srt.openPath(${JSON.stringify(testFile)}); return true; })()`,
    );
    await delay(600);
    check('A1 样本已打开', opened === true && (await countOf('.row')) > 0, `rows=${await countOf('.row')}`);

    // ---- A2 阅读态书签（菜单 编辑→标注→切换书签；书签=顶部可见行） ----
    currentStep = 'A2 阅读态书签';
    await openSubmenu('编辑', '标注');
    await clickFlyoutItem(0);
    await delay(500);
    const bookmarks1 = (await annotationsOf()).bookmarks;
    check('A2 阅读态书签已添加且丝带渲染', bookmarks1.length === 1 && (await countOf('.bmark')) >= 1, `bookmarks=${bookmarks1.length}`);

    // ---- A3 进入编辑态并高亮选中 ----
    currentStep = 'A3 高亮选中';
    await evalJs(`(() => { document.querySelector('[aria-label="切换编辑模式"]')?.click(); return true; })()`);
    await delay(700);
    await evalJs(`(() => { document.querySelector('textarea.input-proxy')?.focus(); return true; })()`);
    await key('Home', 36, 2); // Ctrl+Home 文首
    await delay(150);
    for (let i = 0; i < 4; i += 1) await key('ArrowRight', 39, 8); // Shift+Right ×4 选中 'line'
    await openSubmenu('编辑', '标注');
    await clickFlyoutItem(1);
    await delay(600);
    const highlights1 = (await annotationsOf()).highlights;
    check(
      'A3 高亮已创建且行内渲染',
      highlights1.length === 1 && highlights1[0].endUtf16 - highlights1[0].startUtf16 === 4 && (await countOf('.hl')) >= 1,
      `hl=${highlights1.length} span=${await countOf('.hl')}`,
    );

    // ---- A4 注释（当前行，弹窗输入） ----
    currentStep = 'A4 注释';
    await openSubmenu('编辑', '标注');
    await clickFlyoutItem(2);
    await delay(400);
    const dialogOpen = await countOf('[data-note-dialog]');
    await evalJs(`(() => { document.querySelector('[data-note-input]')?.focus(); return true; })()`);
    await client.send('Input.insertText', { text: 'E2E-NOTE' });
    await delay(150);
    await evalJs(`(() => { document.querySelector('[data-note-ok]')?.click(); return true; })()`);
    await delay(600);
    const notes1 = (await annotationsOf()).notes;
    check(
      'A4 注释弹窗与落库',
      dialogOpen === 1 && notes1.some((n) => n.text === 'E2E-NOTE' && n.kind === 'note'),
      `notes=${notes1.length}`,
    );

    // ---- A5 待办 ----
    currentStep = 'A5 待办';
    await key('ArrowDown', 40, 0); // 移到下一行
    await openSubmenu('编辑', '标注');
    await clickFlyoutItem(3);
    await delay(400);
    await evalJs(`(() => { document.querySelector('[data-note-input]')?.focus(); return true; })()`);
    await client.send('Input.insertText', { text: 'E2E-TODO' });
    await delay(150);
    await evalJs(`(() => { document.querySelector('[data-note-ok]')?.click(); return true; })()`);
    await delay(600);
    const notes2 = (await annotationsOf()).notes;
    check(
      'A5 待办落库（kind=todo）',
      notes2.some((n) => n.text === 'E2E-TODO' && n.kind === 'todo'),
      `notes=${notes2.length}`,
    );

    // ---- A6 面板：分区计数 + 待办勾选 ----
    currentStep = 'A6 标注面板';
    await openSubmenu('编辑', '标注');
    await clickFlyoutItem(4);
    await delay(500);
    const panelOpen = await countOf('[data-annotations-panel]');
    const sectionCounts = await evalJs(`(() => ({
      bookmarks: document.querySelectorAll('[data-ann-section="bookmarks"] [data-ann-item="bookmark"]').length,
      highlights: document.querySelectorAll('[data-ann-section="highlights"] [data-ann-item="highlight"]').length,
      notes: document.querySelectorAll('[data-ann-section="notes"] [data-ann-item="note"]').length,
    }))()`);
    check(
      'A6 面板打开且三分区计数正确',
      panelOpen === 1 && sectionCounts.bookmarks === 1 && sectionCounts.highlights === 1 && sectionCounts.notes === 2,
      JSON.stringify(sectionCounts),
    );
    await evalJs(`(() => {
      const todo = [...document.querySelectorAll('[data-ann-item="note"]')].find((n) => n.textContent.includes('E2E-TODO'));
      todo?.querySelector('[data-ann-done]')?.click();
      return true;
    })()`);
    await delay(600);
    const todoDone = (await annotationsOf()).notes.find((n) => n.text === 'E2E-TODO')?.done;
    check('A6b 待办勾选已持久化', todoDone === true, `done=${todoDone}`);

    // ---- A7 截图（面板打开、含全部标注） ----
    currentStep = 'A7 截图';
    mkdirSync(dirname(screenshotPath), { recursive: true });
    const shot = await client.send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'));
    check('A7 截图保存', existsSync(screenshotPath), screenshotPath);
    await evalJs(`(() => { document.querySelector('[data-ann-close]')?.click(); return true; })()`);
    await delay(300);

    // ---- A8 重启持久化 ----
    currentStep = 'A8 重启持久化';
    await killApp();
    spawnApp();
    await waitClient();
    const evalJs2 = async (expression) => {
      const result = await client.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
      if (result.exceptionDetails) throw new Error(`页面执行异常：${result.exceptionDetails.text}`);
      return result.result?.value;
    };
    await waitReady(evalJs2);
    await dismissOnboarding(evalJs2);
    await evalJs2(
      `(async () => { await window.__srt.openPath(${JSON.stringify(testFile)}); return true; })()`,
    );
    await delay(900);
    const reCounts = await evalJs2(`(() => ({
      bmark: document.querySelectorAll('.bmark').length,
      hl: document.querySelectorAll('.hl').length,
      nmark: document.querySelectorAll('.nmark').length,
    }))()`);
    check(
      'A8 重启后标注渲染持久',
      reCounts.bmark >= 1 && reCounts.hl >= 1 && reCounts.nmark === 2,
      JSON.stringify(reCounts),
    );

    // ---- A9 编辑后锚点重定位（顶部插入换行 → 书签行 +1） ----
    currentStep = 'A9 锚点重定位';
    await evalJs2(`(() => { document.querySelector('[aria-label="切换编辑模式"]')?.click(); return true; })()`);
    await delay(700);
    await evalJs2(`(() => { document.querySelector('textarea.input-proxy')?.focus(); return true; })()`);
    await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Home', windowsVirtualKeyCode: 36, modifiers: 2 });
    await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Home', windowsVirtualKeyCode: 36, modifiers: 2 });
    await delay(150);
    await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Enter', windowsVirtualKeyCode: 13, modifiers: 0 });
    await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', windowsVirtualKeyCode: 13, modifiers: 0 });
    await delay(1500); // 防抖 refreshSoon 800ms + 重定位
    const view2 = await evalJs2(`window.__TAURI_INTERNALS__.invoke('list_tabs')`);
    const data2 = await evalJs2(
      `window.__TAURI_INTERNALS__.invoke('list_annotations', { tabId: ${view2.activeTabId} })`,
    );
    check(
      'A9 顶部插行后书签重定位到第 1 行',
      data2.bookmarks[0]?.row === 1,
      `row=${data2.bookmarks[0]?.row}`,
    );

    // ---- A10 清除本文件标注 ----
    currentStep = 'A10 清除标注';
    await openSubmenu('编辑', '标注');
    await clickFlyoutItem(5);
    await delay(400);
    const confirmOpen = await countOf('[role="alertdialog"]');
    await evalJs(`(() => {
      const dialog = document.querySelector('[role="alertdialog"]');
      const confirm = [...dialog.querySelectorAll('button')].find((b) => b.textContent.includes('清除'));
      confirm?.click();
      return true;
    })()`);
    await delay(700);
    const cleared = await annotationsOf();
    const marksLeft = await evalJs(`document.querySelectorAll('.bmark,.hl,.nmark').length`);
    check(
      'A10 清除后数据与标记全部消失',
      confirmOpen === 1 &&
        cleared.bookmarks.length === 0 &&
        cleared.highlights.length === 0 &&
        cleared.notes.length === 0 &&
        marksLeft === 0,
      `left=${marksLeft}`,
    );
  } finally {
    clearTimeout(watchdog);
    await killApp();
    await removeWithRetry(workDir);
  }

  const failed = checks.filter((item) => !item.passed);
  console.log(`\n标注套件：${checks.length - failed.length}/${checks.length} 通过`);
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
