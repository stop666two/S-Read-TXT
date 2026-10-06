// 命令面板端到端冒烟（P4）：Ctrl+Shift+P 打开、过滤、键盘/点击执行、禁用态与 Esc 关闭。
//
// 依赖：已构建的 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 数据隔离：SRT_DATA_DIR 指向项目 tmp 工作目录；通过后清理，失败保留现场。

import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-palette-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
writeFileSync(join(dataDir, 'settings.json'), JSON.stringify({ schemaVersion: 17 }), 'utf8');
const port = Number(argValue('--port', String(8780 + Math.floor(Math.random() * 50))));

const results = [];
const check = (name, ok, extra = '') => {
  results.push({ name, ok });
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${extra ? `  → ${extra}` : ''}`);
};

let child = null;
const killApp = async () => {
  if (child) {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    child = null;
  }
  for (let i = 0; i < 30; i += 1) {
    const out = spawnSync('tasklist', ['/FI', 'IMAGENAME eq s-read-txt.exe', '/NH'], { encoding: 'utf8' });
    if (!String(out.stdout).includes('s-read-txt.exe')) return;
    await delay(200);
  }
};
const watchdog = setTimeout(() => {
  console.log('FAIL  看门狗超时（300s）');
  process.exitCode = 4;
  void killApp();
}, 300_000);
process.on('exit', () => spawnSync('taskkill', ['/IM', 's-read-txt.exe', '/T', '/F'], { stdio: 'ignore' }));

child = spawn(exe, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
    SRT_NO_ELEVATION: '1',
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: 'ignore',
});

const evalIn = async (client, expression) => {
  const result = await client.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(`eval 异常: ${result.exceptionDetails.text}`);
  return result.result.value;
};

const waitFor = async (client, expression, predicate, timeoutMs = 6000) => {
  const deadline = Date.now() + timeoutMs;
  let last;
  for (;;) {
    last = await evalIn(client, expression);
    if (predicate(last)) return last;
    if (Date.now() > deadline) return last;
    await delay(120);
  }
};

/** 发送按键（modifiers：Ctrl=2、Shift=8） */
async function sendKey(client, { key, code, vk, modifiers = 0, text }) {
  const base = { key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
  await client.send('Input.dispatchKeyEvent', { type: 'keyDown', ...base, text });
  await client.send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
}

const paletteOpenExpr = `!!document.querySelector('[data-command-palette]')`;

try {
  let mainWs = null;
  for (let attempt = 1; attempt <= 3 && !mainWs; attempt += 1) {
    try {
      mainWs = await findTarget(port);
    } catch (error) {
      if (attempt === 3) throw error;
      await delay(2500);
    }
  }
  const main = await createClient(mainWs);
  await main.send('Page.enable');
  await dismissOnboarding((expression) => evalIn(main, expression));
  await main.send('Runtime.evaluate', { expression: 'document.body.focus()' });

  await sendKey(main, { key: 'P', code: 'KeyP', vk: 80, modifiers: 2 | 8, text: 'P' });
  const opened = await waitFor(main, paletteOpenExpr, (v) => v === true);
  check('P1 Ctrl+Shift+P 打开命令面板', opened === true);

  const count = await evalIn(main, `document.querySelectorAll('[data-command-item]').length`);
  check('P2 命令数量充足（≥30）', Number(count) >= 30, `count=${count}`);

  await evalIn(main, `(() => { const el = document.querySelector('[data-command-input]'); el.focus(); return true; })()`);
  await main.send('Input.insertText', { text: '设置' });
  const filteredFirst = await waitFor(
    main,
    `document.querySelector('[data-command-item]')?.getAttribute('data-command-item') ?? ''`,
    (v) => v === 'file.settings',
  );
  check('P3a 过滤后第一项为设置命令', filteredFirst === 'file.settings', `first=${filteredFirst}`);
  await sendKey(main, { key: 'Enter', code: 'Enter', vk: 13 });
  const settingsWs = await findTarget(port, 'settings.html');
  check('P3b Enter 执行并打开设置窗口', typeof settingsWs === 'string' && settingsWs.length > 0);

  await sendKey(main, { key: 'P', code: 'KeyP', vk: 80, modifiers: 2 | 8, text: 'P' });
  await waitFor(main, paletteOpenExpr, (v) => v === true);
  const undoDisabled = await waitFor(
    main,
    `document.querySelector('[data-command-item="edit.undo"]')?.disabled ?? null`,
    (v) => v === true,
  );
  check('P4 无文件时 edit.undo 禁用', undoDisabled === true);

  await sendKey(main, { key: 'Escape', code: 'Escape', vk: 27 });
  const closed = await waitFor(main, paletteOpenExpr, (v) => v === false);
  check('P5 Esc 关闭面板', closed === false);

  await sendKey(main, { key: 'P', code: 'KeyP', vk: 80, modifiers: 2 | 8, text: 'P' });
  await waitFor(main, paletteOpenExpr, (v) => v === true);
  await evalIn(main, `document.querySelector('[data-command-item="file.new"]').click()`);
  const tabs = await waitFor(
    main,
    `(async () => (await window.__TAURI_INTERNALS__.invoke('list_tabs')).tabs.filter((t) => t.untitled != null).length)()`,
    (v) => Number(v) === 1,
  );
  check('P6 点击执行「新建文件」生效', Number(tabs) === 1, `untitled=${tabs}`);

  const failed = results.filter((r) => !r.ok);
  console.log(`结果：${results.length - failed.length}/${results.length} 通过`);
  process.exitCode = failed.length === 0 ? 0 : 1;
} catch (error) {
  console.error(`套件异常：${error?.stack ?? error}`);
  process.exitCode = 1;
} finally {
  clearTimeout(watchdog);
  await killApp();
  await delay(300);
  if (process.exitCode === 0) {
    rmSync(work, { recursive: true, force: true });
  }
}
