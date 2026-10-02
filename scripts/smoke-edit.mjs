#!/usr/bin/env node
// 编辑流程冒烟（阶段 4b）：真实应用 + CDP 驱动，验证「进入编辑 → 输入 → 撤销 → 再输入 → 保存」。
//
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-edit.mjs [--exe <路径>] [--port 9223] [--screenshot <路径>]
// 说明：应用启动时注入 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>；
//       中文路径经 JSON.stringify 注入；断言基于真实 DOM 与真实磁盘文件。

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget, openPathDone, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const port = Number(argValue('--port', String(9200 + Math.floor(Math.random() * 600))));
const screenshotPath = argValue('--screenshot', join(root, 'docs', 'screenshots', 'phase4b-edit.png'));

const workDir = join(process.env.TEMP ?? '.', 'srt-smoke-edit');
const testFile = join(workDir, 'edit-sample.txt');
const originalText = '第一行文本\n第二行文本\n';
// 每次运行使用独立数据目录（含 WebView2 用户数据）：
// ① 并行/连续运行互不竞争配置锁（此前的空白窗口根因）；② 不污染真实便携数据。
const runDataDir = join(workDir, `data-${Date.now()}`);

const checks = [];
function check(name, passed, detail = '') {
  checks.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

async function main() {
  if (!existsSync(exePath)) {
    console.error(`可执行文件不存在：${exePath}（先运行 npm run tauri build -- --debug --no-bundle）`);
    process.exit(2);
  }
  mkdirSync(workDir, { recursive: true });
  rmSync(testFile, { force: true });
  writeFileSync(testFile, originalText, 'utf8');

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

    // 等待前端就绪；超时直接失败并给出提示（避免后续误报为「页面执行异常」）
    let ready = false;
    for (let attempt = 0; attempt < 180; attempt += 1) {
      ready = (await evalJs('!!window.__srt?.openPath && !!window.__TAURI_INTERNALS__')) === true;
      if (ready) break;
      await delay(250);
    }
    if (!ready) {
      throw new Error(
        '前端未就绪（window.__srt 未注入）。请确认：① 已运行 npm run tauri build -- --debug --no-bundle（cargo build/test 产物是 dev 语义，不内嵌前端）；② 无残留实例占用端口。',
      );
    }
    await dismissOnboarding(evalJs);

    // 1) 打开样本文件（轮询等待首行渲染完成）
    await evalJs(openPathDone(testFile));
    const firstRow = await waitForValue(async () => {
      const text = await evalJs(`document.querySelector('.row')?.textContent ?? ''`);
      return text || '';
    }, 8000);
    check('打开文件并渲染首行', firstRow === '第一行文本', `首行=「${firstRow}」`);

    // 2) 进入编辑模式（工具栏按钮；轮询等待后端状态）
    await evalJs(`(document.querySelector('[aria-label="切换编辑模式"]')?.click(), true)`);
    const editState = await waitForValue(async () => {
      const state = await evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); const t = v.tabs[0]; return { editing: t.editing, dirty: t.dirty, tabId: v.activeTabId }; })()`,
      );
      return state?.editing ? state : null;
    }, 8000);
    check('进入编辑模式（editing=true）', editState?.editing === true, JSON.stringify(editState));
    const proxyFocused = await waitForValue(
      async () =>
        (await evalJs(`document.activeElement?.classList.contains('input-proxy') ?? false`)) || null,
      4000,
    );
    check('隐藏输入框获得焦点', proxyFocused === true);

    // 3) 输入文本（CDP Input.insertText → beforeinput → 引擎；轮询等待结果）
    await client.send('Input.insertText', { text: 'ZZ' });
    const afterInsert = await waitForValue(async () => {
      const state = await evalJs(
        `(async () => { const row = document.querySelector('.row')?.textContent ?? ''; const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return { row, dirty: v.tabs[0].dirty }; })()`,
      );
      return state.row === 'ZZ第一行文本' ? state : null;
    }, 6000);
    check(
      '输入后首行包含新文本',
      afterInsert?.row === 'ZZ第一行文本',
      `首行=「${afterInsert?.row ?? '(超时)'}」`,
    );
    check('输入后脏标记为真', afterInsert?.dirty === true);

    // 4) 撤销（Ctrl+Z 经 CDP 键盘事件；rawKeyDown 才会投递到页面）
    await evalJs(
      `(window.__keylog = [], window.__keylogHandler = (e) => window.__keylog.push(e.key), document.addEventListener('keydown', window.__keylogHandler, true), true)`,
    );
    await client.send('Input.dispatchKeyEvent', {
      type: 'rawKeyDown',
      key: 'z',
      code: 'KeyZ',
      windowsVirtualKeyCode: 90,
      nativeVirtualKeyCode: 90,
      modifiers: 2,
    });
    await client.send('Input.dispatchKeyEvent', {
      type: 'keyUp',
      key: 'z',
      code: 'KeyZ',
      windowsVirtualKeyCode: 90,
      nativeVirtualKeyCode: 90,
      modifiers: 2,
    });
    const afterUndo = await waitForValue(async () => {
      const state = await evalJs(
        `(async () => { const row = document.querySelector('.row')?.textContent ?? ''; const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return { row, dirty: v.tabs[0].dirty }; })()`,
      );
      return state.row === '第一行文本' && state.dirty === false ? state : null;
    }, 6000);
    const keylog = await evalJs(
      `(document.removeEventListener('keydown', window.__keylogHandler, true), window.__keylog.join(','))`,
    );
    check(
      '撤销后首行还原',
      afterUndo?.row === '第一行文本',
      `首行=「${afterUndo?.row ?? '(超时)'}」键盘日志=[${keylog}]`,
    );
    check('撤销后脏标记清除', afterUndo?.dirty === false);

    // 5) 再输入并保存（走保存弹窗：保持当前编码 + 默认备份；全程轮询）
    await client.send('Input.insertText', { text: 'Y' });
    await waitForValue(async () => {
      const row = await evalJs(`document.querySelector('.row')?.textContent ?? ''`);
      return row === 'Y第一行文本' ? row : null;
    }, 6000);
    await evalJs(`(document.querySelector('[aria-label="保存"]')?.click(), true)`);
    const dialogSeen = await waitForValue(
      async () => (await evalJs(`document.querySelector('[role="dialog"]') !== null`)) || null,
      4000,
    );
    check('保存弹窗打开', dialogSeen === true);
    const confirmClicked = await evalJs(
      `(() => { const dlg = document.querySelector('[role="dialog"]'); if (!dlg) return 'no-dialog'; const btn = [...dlg.querySelectorAll('button')].find((b) => b.textContent.trim() === '保存'); if (!btn) return 'no-button'; btn.click(); return 'clicked'; })()`,
    );
    check('保存弹窗确认点击', confirmClicked === 'clicked', confirmClicked);
    let disk = '';
    for (let attempt = 0; attempt < 40; attempt += 1) {
      try {
        disk = readFileSync(testFile, 'utf8');
      } catch {
        disk = '';
      }
      if (disk === `Y${originalText}`) break;
      await delay(200);
    }
    check('磁盘文件已写入新内容', disk === `Y${originalText}`, `磁盘=「${disk.replace(/\n/g, '\\n')}」`);
    const afterSave = await waitForValue(async () => {
      const state = await evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return { dirty: v.tabs[0].dirty }; })()`,
      );
      return state.dirty === false ? state : null;
    }, 5000);
    check('保存后脏标记清除', afterSave?.dirty === false);

    // 6) 截图（编辑态画面，供人工核验视觉效果）
    try {
      const shot = await client.send('Page.captureScreenshot', { format: 'png' });
      mkdirSync(dirname(screenshotPath), { recursive: true });
      writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'));
      check('截图已保存', true, screenshotPath);
    } catch (error) {
      check('截图已保存', false, String(error));
    }
  } finally {
    client?.close();
    // 整树结束：仅回收本应用进程树（/T 会连带其 WebView2 子进程，避免孤儿锁配置目录）。
    // 严禁按进程名杀 msedgewebview2 —— WebView2 是系统共享运行时，其他应用也在使用！
    if (child.pid) {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    }
    try {
      rmSync(runDataDir, { recursive: true, force: true });
    } catch {
      // 数据目录清理失败不影响测试结论（位于系统临时目录）
    }
  }

  const failed = checks.filter((item) => !item.passed);
  console.log(`\n冒烟结果：${checks.length - failed.length}/${checks.length} 通过`);
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((error) => {
  console.error(`冒烟失败：${error?.message ?? error}`);
  process.exit(3);
});
