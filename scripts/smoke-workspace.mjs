// 工作区（多文件）查找与替换 E2E（常驻套件）。
// 场景：
//   W1 打开两个文件（edit-a 进编辑态；read-b 只读态）
//   W2 编辑菜单 →「工作区查找与替换…」→ 弹窗出现
//   W3 搜索 alpha → 2 个文件 · 3 处匹配（编辑态跨行引擎 + 只读逐行）
//   W4 编辑中 / 只读 徽标
//   W5 点击编辑态命中 → 切换标签 + 选区定位（.selection 出现）
//   W6 点击只读命中 → 切换标签
//   W7 全部替换（确认框）→ 仅编辑态命中被替换（编辑中标签变脏）
//   W8 单步撤销还原（脏标记回落）
//   W9 设置关闭多文件搜索后 → 搜索报「已设置关闭」
//   W10 Esc 关闭弹窗
//   W11 截图
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  argValue,
  createClient,
  delay,
  dismissOnboarding,
  findTarget,
  openPathDone,
  waitForValue,
} from './lib/smoke-cdp.mjs';

const root = resolve(resolve(fileURLToPath(import.meta.url), '..'), '..');
const exe = argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe'));
const work = join(tmpdir(), `srt-workspace-${Date.now()}`);
mkdirSync(work, { recursive: true });
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
const port = 9700 + Math.floor(Math.random() * 200);

const editA = join(work, 'edit-a.txt');
const readB = join(work, 'read-b.txt');
writeFileSync(editA, 'alpha\nbeta\nalpha again\ngamma\n', 'utf8');
writeFileSync(readB, 'alpha read\nnothing\n', 'utf8');

let currentStep = 'W0 启动';
const checks = [];
function check(name, ok, detail) {
  checks.push({ name, ok: !!ok, detail: detail === undefined ? '' : String(detail) });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail === undefined ? '' : ` :: ${detail}`}`);
}

const child = spawn(exe, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: 'ignore',
});

function killTree() {
  spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
}
function removeWithRetry(dir) {
  for (let i = 0; i < 12; i += 1) {
    try {
      rmSync(dir, { recursive: true, force: true });
      return;
    } catch {
      spawnSync('cmd', ['/c', 'ping', '-n', '1', '127.0.0.1', '>nul'], { stdio: 'ignore' });
    }
  }
}

const watchdog = setTimeout(() => {
  console.error(`WATCHDOG 触发（240s），最后步骤：${currentStep}`);
  killTree();
  removeWithRetry(work);
  process.exit(4);
}, 240_000);

let client = null;
const send = (method, params) => client.send(method, params);
const evalRaw = async (expression) => {
  const result = await send('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) {
    throw new Error(`页面表达式失败：${result.exceptionDetails.text}`);
  }
  return result.result?.value;
};
const evalJs = (expression) => evalRaw(`(async () => { ${expression} })()`);
const waitUntil = async (fn, timeoutMs = 8000) => (await waitForValue(fn, timeoutMs)) === true;
const waitEquals = async (fn, expected, timeoutMs = 8000) =>
  waitUntil(async () => (await fn()) === expected, timeoutMs);
const waitText = (selector, expected, timeoutMs = 8000) =>
  waitEquals(
    () => evalRaw(`document.querySelector(${JSON.stringify(selector)})?.textContent ?? null`),
    expected,
    timeoutMs,
  );
const clickByText = async (text, selector = 'button') =>
  evalRaw(`(() => {
    const nodes = [...document.querySelectorAll(${JSON.stringify(selector)})];
    const hit = nodes.find((node) => node.textContent?.trim() === ${JSON.stringify(text)});
    if (!hit) return false;
    hit.click();
    return true;
  })()`);
const activeTabId = () =>
  evalRaw(`window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.activeTabId)`);

async function main() {
  currentStep = 'W0 等待就绪';
  const wsUrl = await findTarget(port);
  client = await createClient(wsUrl);
  await send('Runtime.enable', {});
  await waitUntil(() => evalRaw('typeof window.__srt !== "undefined"'), 30_000);
  await dismissOnboarding(evalRaw);
  await delay(200);

  currentStep = 'W1 打开文件';
  await evalJs(`void window.__srt.openPath(${JSON.stringify(editA)}); return true;`);
  await waitUntil(async () => (await activeTabId()) !== null, 15_000);
  await delay(400);
  // 进入编辑态（走真实按钮路径：直调 invoke 不刷新前端）
  await evalJs(`document.querySelector('[aria-label="切换编辑模式"]')?.click(); return true;`);
  await waitUntil(
    () => evalJs(`return window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs.some((t) => t.editing))`),
    10_000,
  );
  await evalJs(`void window.__srt.openPath(${JSON.stringify(readB)}); return true;`);
  await waitEquals(
    () => evalJs(`return window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs.length)`),
    2,
    10_000,
  );
  await delay(400);

  currentStep = 'W2 打开工作区弹窗';
  await clickByText('编辑');
  await delay(250);
  const menuClicked = await clickByText('工作区查找与替换…', '.dropdown button, .item');
  check('W2a 菜单项存在且可点击', menuClicked);
  await waitUntil(() => evalJs(`return !!document.querySelector('[data-setting="ws.query"]')`));
  check('W2b 弹窗出现', await evalJs(`return !!document.querySelector('[data-setting="ws.query"]')`));

  currentStep = 'W3 搜索';
  await evalJs(`document.querySelector('[data-setting="ws.query"]').focus(); return true;`);
  await send('Input.insertText', { text: 'alpha' });
  // 开启区分大小写：确保「全部替换」后编辑态不再命中（ALPHA ≠ alpha）
  await evalJs(`document.querySelector('[data-setting="ws.case"]').click(); return true;`);
  await evalJs(`document.querySelector('[data-setting="ws.search"]').click(); return true;`);
  const summaryOk = await waitText('[data-ws-summary]', '共 2 个文件 · 3 处匹配');
  check('W3 汇总（2 文件 3 处）', summaryOk, String(await evalJs(`return document.querySelector('[data-ws-summary]')?.textContent ?? ''`)));
  check('W3b 区分大小写已开启', (await evalJs(`return document.querySelector('[data-setting="ws.case"]').checked`)) === true);
  const fileBadges = await evalJs(`
    const heads = [...document.querySelectorAll('[data-ws-file]')];
    return heads.map((h) => h.textContent);
  `);
  check('W4 徽标（编辑中/只读）', JSON.stringify(fileBadges).includes('编辑中') && JSON.stringify(fileBadges).includes('只读'), JSON.stringify(fileBadges));

  currentStep = 'W5 跳转编辑态命中';
  const firstEditHit = await evalJs(`
    const files = [...document.querySelectorAll('.ws-file')];
    const target = files.find((f) => f.querySelector('[data-ws-file]')?.textContent?.includes('edit-a.txt'));
    const hit = target?.querySelectorAll('[data-ws-hit]')[1];
    hit?.click();
    return !!hit;
  `);
  check('W5a 编辑态第三个命中可点击', firstEditHit);
  await delay(700);
  const editTabId = await evalJs(`return window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs.find((t) => t.name === 'edit-a.txt')?.tabId ?? null)`);
  await waitEquals(activeTabId, editTabId);
  check('W5b 切换回编辑态标签', true);
  const selectionShown = await waitUntil(
    () => evalJs(`return document.querySelectorAll('.selection').length > 0`),
    6000,
  );
  check('W5c 命中选区已建立', selectionShown === true, `active=${await activeTabId()}`);

  currentStep = 'W6 跳转只读命中';
  const readHit = await evalJs(`
    const files = [...document.querySelectorAll('.ws-file')];
    const target = files.find((f) => f.querySelector('[data-ws-file]')?.textContent?.includes('read-b.txt'));
    const hit = target?.querySelector('[data-ws-hit]');
    hit?.click();
    return !!hit;
  `);
  check('W6a 只读命中可点击', readHit);
  const readTabId = await evalJs(`return window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs.find((t) => t.name === 'read-b.txt')?.tabId ?? null)`);
  await waitEquals(activeTabId, readTabId);
  check('W6b 切换至只读标签', true);

  currentStep = 'W7 全部替换';
  await evalJs(`document.querySelector('[data-setting="ws.replace"]').focus(); return true;`);
  await send('Input.insertText', { text: 'ALPHA' });
  await evalJs(`document.querySelector('[data-setting="ws.replaceAll"]').click(); return true;`);
  await waitUntil(() => evalJs(`return !!document.querySelector('[role="alertdialog"]')`));
  await clickByText('全部替换', '[role="alertdialog"] button');
  const replacedOk = await waitText('[data-ws-replaced]', '已替换 2 处，跳过 1 个文件');
  check('W7a 替换汇总（2 处 / 跳过 1 文件）', replacedOk);
  const dirtyAfterReplace = await evalJs(`return window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs.find((t) => t.name === 'edit-a.txt')?.dirty ?? null)`);
  check('W7b 编辑态标签变脏', dirtyAfterReplace === true, String(dirtyAfterReplace));
  const summaryAfter = await waitText('[data-ws-summary]', '共 1 个文件 · 1 处匹配');
  check('W7c 重扫结果更新（只读剩余 1 处）', summaryAfter === true);

  currentStep = 'W8 单步撤销';
  // 切回编辑态标签并聚焦编辑器后撤销
  await evalJs(`
    const tabsBar = [...document.querySelectorAll('.tab')];
    const tab = tabsBar.find((t) => t.textContent?.includes('edit-a.txt'));
    tab?.click();
    return true;
  `);
  await waitEquals(activeTabId, editTabId);
  await delay(400);
  await evalJs(`document.querySelector('textarea.input-proxy')?.focus(); return true;`);
  await delay(200);
  const focusOk = await evalJs(`return document.activeElement?.classList?.contains('input-proxy') === true`);
  check('W8a 编辑器已聚焦', focusOk === true);
  let dirtyAfterUndo = true;
  for (let attempt = 0; attempt < 3 && dirtyAfterUndo; attempt += 1) {
    await send('Input.dispatchKeyEvent', { type: 'keyDown', modifiers: 2, key: 'z', code: 'KeyZ', windowsVirtualKeyCode: 90 });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', modifiers: 2, key: 'z', code: 'KeyZ', windowsVirtualKeyCode: 90 });
    dirtyAfterUndo =
      (await waitEquals(
        () =>
          evalJs(
            `return window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs.find((t) => t.name === 'edit-a.txt')?.dirty ?? null)`,
          ),
        false,
        2500,
      )) !== true;
  }
  check('W8 单步撤销还原（脏回落）', dirtyAfterUndo === false, String(dirtyAfterUndo));

  currentStep = 'W9 关闭开关拒绝';
  await evalJs(`
    const snap = await window.__TAURI_INTERNALS__.invoke('get_settings');
    snap.app.find.multifileEnabled = false;
    await window.__TAURI_INTERNALS__.invoke('save_settings', { request: { app: snap.app, reader: snap.reader, shortcuts: snap.shortcuts.bindings } });
    return true;
  `);
  await delay(300);
  await evalJs(`document.querySelector('[data-setting="ws.search"]').click(); return true;`);
  await waitUntil(() => evalJs(`return (document.querySelector('[data-ws-error]')?.textContent ?? '').length > 0`));
  const errText = await evalJs(`return document.querySelector('[data-ws-error]')?.textContent ?? ''`);
  check('W9 关闭后搜索被拒', String(errText).includes('已设置中关闭') || String(errText).includes('multifile') || String(errText).includes('多文件'), String(errText));
  // 恢复开关，避免影响后续套件（独立数据目录，这一步主要自证可逆）
  await evalJs(`
    const snap = await window.__TAURI_INTERNALS__.invoke('get_settings');
    snap.app.find.multifileEnabled = true;
    await window.__TAURI_INTERNALS__.invoke('save_settings', { request: { app: snap.app, reader: snap.reader, shortcuts: snap.shortcuts.bindings } });
    return true;
  `);

  currentStep = 'W10 Esc 关闭 + 截图';
  await send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  await send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  const closed = await waitUntil(() => evalJs(`return !document.querySelector('[data-setting="ws.query"]')`));
  check('W10 Esc 关闭弹窗', closed === true);
  const shot = argValue('--screenshot', '');
  if (shot !== '') {
    const data = await send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(shot, Buffer.from(data.data, 'base64'));
    check('W11 截图保存', true, shot);
  }

  const failed = checks.filter((item) => !item.ok);
  console.log(`\n工作区套件：${checks.length - failed.length}/${checks.length}`);
  if (failed.length > 0) {
    console.error(`失败项：${failed.map((item) => item.name).join('；')}`);
  }
  process.exitCode = failed.length === 0 ? 0 : 1;
}

main()
  .catch((error) => {
    console.error(`FAIL 异常（${currentStep}）：${error?.message ?? error}`);
    process.exitCode = 1;
  })
  .finally(() => {
    clearTimeout(watchdog);
    killTree();
    removeWithRetry(work);
  });
