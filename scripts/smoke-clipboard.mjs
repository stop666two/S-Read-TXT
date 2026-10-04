// 剪贴板历史与复制格式 E2E（P1-5 常驻套件）。
// 场景：
//   C1 编辑模式 + 全选复制 → 历史新增一条（文本与选区一致）
//   C2 文件持久化（data/clipboard-history.json 含条目）
//   C3 菜单「剪贴板历史…」→ 弹窗条目可见
//   C4 「插入」把条目插入编辑区（字节数增加）
//   C5 单条删除 → 空态
//   C6 连续复制两条 → 清空（确认）→ 空态
//   C7 historyLimit=0 时复制不记录；恢复 200
//   C8 菜单「复制为→HTML」成功（剪贴板纯文本兜底与选区一致）
//   C9 截图
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
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

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe'));
const screenshot = process.argv.includes('--screenshot');
const workDir = join(tmpdir(), `srt-clipboard-${Date.now()}`);
const dataDir = join(workDir, 'data');
mkdirSync(dataDir, { recursive: true });
const samplePath = join(workDir, 'clip.txt');
const SAMPLE = 'alpha\nbeta\ngamma';
writeFileSync(samplePath, SAMPLE, 'utf8');
const port = 10100 + Math.floor(Math.random() * 300);

let passed = 0;
let failed = 0;
let currentStep = '初始化';
const failures = [];

function check(name, ok, detail = '') {
  if (ok) {
    passed += 1;
    console.log(`  ✓ ${name}`);
  } else {
    failed += 1;
    failures.push(`${name}${detail ? `：${detail}` : ''}`);
    console.error(`  ✗ ${name}${detail ? `：${detail}` : ''}`);
  }
}

const watchdogMs = 240_000;
const watchdog = setTimeout(() => {
  console.error(`看门狗超时（${watchdogMs}ms），最后步骤：${currentStep}`);
  child?.kill();
  process.exit(4);
}, watchdogMs);

const child = spawn(exePath, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: 'ignore',
});

function killTree() {
  if (child?.pid) {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  }
}

try {
  const wsUrl = await findTarget(port, 'tauri.localhost/');
  const client = await createClient(wsUrl);
  await client.send('Runtime.enable');
  await client.send('Page.enable');
  const evalJs = async (expression) => {
    const result = await client.send('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (result.exceptionDetails) {
      const detail =
        result.exceptionDetails.exception?.description ??
        JSON.stringify(result.exceptionDetails.exception ?? result.exceptionDetails);
      throw new Error(`页面求值失败：${detail}`);
    }
    return result.result.value;
  };
  const invoke = (cmd, args = {}) =>
    evalJs(
      `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}).then((v) => (v === undefined ? null : v))`,
    );
  const clickBySelector = (selector) =>
    evalJs(
      `(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return false; el.click(); return true; })()`,
    );
  const clickByText = (text, scope = 'button') =>
    evalJs(
      `(() => { const el = [...document.querySelectorAll(${JSON.stringify(scope)})].find((b) => b.textContent?.includes(${JSON.stringify(text)})); if (!el) return false; el.click(); return true; })()`,
    );

  await waitForValue(() => evalJs(`Boolean(window.__srt && window.__TAURI_INTERNALS__)`), 30_000);
  await dismissOnboarding(evalJs);
  await delay(200);

  // C1 打开 + 编辑 + 全选复制
  currentStep = 'C1 打开复制';
  await evalJs(openPathDone(samplePath));
  await waitForValue(
    () => evalJs(`[...document.querySelectorAll('.row')].some((r) => r.textContent?.includes('alpha'))`),
    20_000,
  );
  await clickBySelector('[aria-label="切换编辑模式"]');
  await waitForValue(() => evalJs(`Boolean(document.querySelector('textarea.input-proxy'))`), 15_000);
  await evalJs(`document.querySelector('textarea.input-proxy')?.focus()`);
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyDown',
    modifiers: 2,
    key: 'a',
    code: 'KeyA',
    windowsVirtualKeyCode: 65,
  });
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    modifiers: 2,
    key: 'a',
    code: 'KeyA',
    windowsVirtualKeyCode: 65,
  });
  await delay(150);
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyDown',
    modifiers: 2,
    key: 'c',
    code: 'KeyC',
    windowsVirtualKeyCode: 67,
  });
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    modifiers: 2,
    key: 'c',
    code: 'KeyC',
    windowsVirtualKeyCode: 67,
  });
  await delay(400);
  const list1 = await invoke('list_clipboard_history');
  check('C1a 复制后历史非空', Array.isArray(list1) && list1.length >= 1, JSON.stringify(list1?.length));
  check('C1b 历史文本与选区一致', list1?.[0]?.text === SAMPLE, JSON.stringify(list1?.[0]?.text));

  // C2 持久化文件
  currentStep = 'C2 持久化';
  const historyFile = join(dataDir, 'clipboard-history.json');
  await waitForValue(() => existsSync(historyFile), 5_000).catch(() => {});
  check('C2 历史写入 data/clipboard-history.json', existsSync(historyFile));
  if (existsSync(historyFile)) {
    const raw = readFileSync(historyFile, 'utf8');
    check('C2b 文件包含条目文本', raw.includes('alpha') && raw.includes('beta'));
  }

  // C3 菜单打开历史弹窗
  currentStep = 'C3 弹窗';
  await clickByText('编辑', '.title');
  await delay(150);
  await clickByText('剪贴板历史', '.dropdown .item');
  await waitForValue(() => evalJs(`Boolean(document.querySelector('[data-clipboard-item]'))`), 8_000);
  check('C3 弹窗条目可见', (await evalJs(`document.querySelectorAll('[data-clipboard-item]').length`)) >= 1);

  // C4 插入（先收拢选区到行首，插入应使字节数增加；关闭弹窗 + 字节数增加）
  currentStep = 'C4 插入';
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyDown',
    key: 'Home',
    code: 'Home',
    windowsVirtualKeyCode: 36,
  });
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    key: 'Home',
    code: 'Home',
    windowsVirtualKeyCode: 36,
  });
  await delay(200);
  const bytesBefore = await evalJs(`window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs[0].byteLen)`);
  await clickBySelector('[data-clipboard-insert]');
  await waitForValue(() => evalJs(`!document.querySelector('[data-clipboard-item]')`), 8_000);
  await delay(400);
  const bytesAfter = await evalJs(`window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs[0].byteLen)`);
  check('C4a 弹窗已关闭', (await evalJs(`Boolean(document.querySelector('[data-clipboard-item]'))`)) === false);
  check('C4b 插入后字节数增加', bytesAfter === bytesBefore + SAMPLE.length, `${bytesBefore} → ${bytesAfter}`);

  // C5 单条删除 → 空态
  currentStep = 'C5 删除单条';
  await clickByText('编辑', '.title');
  await delay(150);
  await clickByText('剪贴板历史', '.dropdown .item');
  await waitForValue(() => evalJs(`Boolean(document.querySelector('[data-clipboard-remove]'))`), 8_000);
  await clickBySelector('[data-clipboard-remove]');
  await waitForValue(() => evalJs(`Boolean(document.querySelector('[data-clipboard-empty]'))`), 8_000);
  check('C5 删除后空态', (await evalJs(`Boolean(document.querySelector('[data-clipboard-empty]'))`)) === true);
  await clickBySelector('[data-setting="clipboardHistory.close"]');
  await delay(200);

  // C6 复制两条 → 清空确认 → 空态
  currentStep = 'C6 清空';
  await evalJs(`document.querySelector('.reader')?.focus()`);
  await evalJs(`window.__TAURI_INTERNALS__.invoke('add_clipboard_entry', { text: 'one' })`);
  await evalJs(`window.__TAURI_INTERNALS__.invoke('add_clipboard_entry', { text: 'two' })`);
  check('C6a 直连添加两条', (await invoke('list_clipboard_history')).length === 2);
  const menuClicked = await clickByText('编辑', '.title');
  check('C6 步骤：点击编辑菜单', menuClicked === true);
  await delay(150);
  const menuItemClicked = await clickByText('剪贴板历史', '.dropdown .item');
  check('C6 步骤：点击剪贴板历史项', menuItemClicked === true);
  await waitForValue(() => evalJs(`Boolean(document.querySelector('[data-setting="clipboardHistory.clear"]'))`), 8000);
  check(
    'C6 步骤：弹窗打开',
    await evalJs(`Boolean(document.querySelector('[data-setting="clipboardHistory.clear"]'))`),
  );
  // 弹窗打开时异步刷新列表：必须等条目渲染出来再点「清空」，
  // 否则按钮处于 disabled（空态）而点击无效（曾致 C6b/C6c/C8d 竞态级联失败）。
  await waitForValue(
    async () =>
      ((await evalJs(`document.querySelectorAll('[data-clipboard-item]').length >= 2`)) ? true : null),
    5000,
    150,
  );
  await clickBySelector('[data-setting="clipboardHistory.clear"]');
  // 确认弹窗按动画/帧渲染出现：批量高压下固定 300ms 等待不稳，改为轮询（曾致 C6b/C6c/C8d 级联失败）
  const confirmButtons = await waitForValue(async () => {
    const buttons = await evalJs(
      `(() => { const d = document.querySelector('[role="alertdialog"]'); return d ? [...d.querySelectorAll('button')].map((b) => b.textContent?.trim() ?? '') : null; })()`,
    );
    return Array.isArray(buttons) && buttons.length > 0 ? buttons : null;
  }, 5000, 150);
  check('C6b 清空确认弹窗出现', Array.isArray(confirmButtons), JSON.stringify(confirmButtons));
  if (Array.isArray(confirmButtons)) {
    const target = confirmButtons.findIndex((text) => text.includes('清空'));
    const index = target >= 0 ? target : confirmButtons.length - 1;
    await evalJs(
      `document.querySelectorAll('[role="alertdialog"] button')[${index}]?.click()`,
    );
  }
  await waitForValue(async () => (await invoke('list_clipboard_history')).length === 0, 3000);
  const afterClear = await invoke('list_clipboard_history');
  check('C6c 清空后后端为空', Array.isArray(afterClear) && afterClear.length === 0, JSON.stringify(afterClear?.length));
  await evalJs(`document.querySelector('[data-setting="clipboardHistory.close"]')?.click()`);
  await delay(200);

  // C7 上限 0 禁用（editor 属于 app 配置节内）
  currentStep = 'C7 上限 0';
  const snapshot = await invoke('get_settings');
  const withLimit = (limit) => ({
    app: {
      ...snapshot.app,
      editor: {
        ...snapshot.app.editor,
        clipboard: { ...snapshot.app.editor.clipboard, historyLimit: limit },
      },
    },
    reader: snapshot.reader,
    shortcuts: snapshot.shortcuts.bindings,
  });
  await invoke('save_settings', { request: withLimit(0) });
  await delay(300);
  await evalJs(`document.querySelector('textarea.input-proxy')?.focus()`);
  await invoke('add_clipboard_entry', { text: 'should-not-record' });
  const afterDisable = await invoke('list_clipboard_history');
  check('C7a 上限 0 时不记录', Array.isArray(afterDisable) && afterDisable.length === 0);
  await invoke('save_settings', { request: withLimit(200) });
  await delay(200);

  // C8 复制为 HTML（菜单子项；纯文本兜底与选区一致）
  currentStep = 'C8 复制为 HTML';
  await evalJs(`document.querySelector('textarea.input-proxy')?.focus()`);
  // 重选全文
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyDown',
    modifiers: 2,
    key: 'a',
    code: 'KeyA',
    windowsVirtualKeyCode: 65,
  });
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    modifiers: 2,
    key: 'a',
    code: 'KeyA',
    windowsVirtualKeyCode: 65,
  });
  await delay(150);
  await clickByText('编辑', '.title');
  await delay(150);
  // 悬停「复制为」展开子菜单
  const wrapBox = await evalJs(
    `(() => { const el = [...document.querySelectorAll('.submenu-wrap')].find((w) => w.textContent?.includes('复制为')); if (!el) return null; const r = el.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`,
  );
  check('C8a 复制为子菜单存在', Boolean(wrapBox));
  if (wrapBox) {
    await client.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x: wrapBox.x, y: wrapBox.y });
    await delay(200);
    const clicked = await evalJs(
      `(() => { const el = [...document.querySelectorAll('.flyout .item')].find((b) => b.textContent?.trim() === 'HTML'); if (!el) return false; el.click(); return true; })()`,
    );
    check('C8b 点击「HTML」成功', clicked === true);
    await delay(400);
    const clipText = await evalJs(
      `window.__TAURI_INTERNALS__.invoke('plugin:clipboard-manager|read_text').then((v) => v ?? '')`,
    );
    const listHtml = await invoke('list_clipboard_history');
    check('C8c 剪贴板纯文本兜底与记录一致', clipText === listHtml?.[0]?.text, JSON.stringify(clipText));
    check('C8d HTML 复制记录历史', Array.isArray(listHtml) && listHtml.length === 1, JSON.stringify(listHtml?.length));
  }

  // C9 截图
  currentStep = 'C9 截图';
  if (screenshot) {
    const shot = await client.send('Page.captureScreenshot', { format: 'png' });
    const shotPath = join(root, 'docs', 'screenshots', 'phase-p1-clipboard.png');
    writeFileSync(shotPath, Buffer.from(shot.data, 'base64'));
    console.log(`  截图：${shotPath}`);
  }

  client.close();
} catch (error) {
  failed += 1;
  failures.push(`${currentStep} 异常：${error instanceof Error ? error.message : String(error)}`);
  console.error(`✗ ${currentStep} 异常：`, error);
} finally {
  clearTimeout(watchdog);
  killTree();
  // WebView2 子进程随进程树回收需要时间：轮询删除，避免批量高压下 EPERM 致套件崩溃
  for (let attempt = 0; attempt < 40 && existsSync(workDir); attempt += 1) {
    try {
      rmSync(workDir, { recursive: true, force: true });
    } catch {
      // 仍被占用：下轮重试
    }
    if (existsSync(workDir)) await delay(250);
  }
}

console.log(`\n剪贴板历史 E2E：${passed} 通过 / ${failed} 失败`);
if (failed > 0) {
  console.error('失败项：');
  for (const item of failures) console.error(`  - ${item}`);
  process.exitCode = 1;
}
