// 大文件双阈值 E2E（P0-1 常驻套件）。
// 场景：
//   L1 默认阈值（只读 100MB / 硬上限 2048MB）打开 101MiB 文件 → 只读打开 + Toast 提示
//   L2 状态栏出现「只读」标记
//   L3 工具栏编辑按钮禁用（title 给出原因）
//   L4 Ctrl+E 被拦截并提示不可编辑
//   L5 硬上限调低为 100MB 后，打开另一 101MiB 文件 → 拒绝（逐字提示）
//   L6 设置窗口 GeneralTab 出现「只读阈值」「硬上限」两个滑块
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
import { spawn, spawnSync } from 'node:child_process';
import { closeSync, mkdirSync, openSync, rmSync, writeFileSync, writeSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  createClient,
  delay,
  dismissOnboarding,
  findTarget,
  openPathDone,
  waitForValue,
} from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(tmpdir(), `srt-limits-${Date.now()}`);
mkdirSync(work, { recursive: true });
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
const port = 9800 + Math.floor(Math.random() * 150);

// 101 MiB 单行文件（> 只读阈值 100MiB，< 默认硬上限 2048MiB）
const MIB = 1024 * 1024;
function writeBig(name) {
  const path = join(work, name);
  const fd = openSync(path, 'w');
  const chunk = Buffer.alloc(MIB, 0x61); // 'a'
  for (let i = 0; i < 101; i += 1) writeSync(fd, chunk);
  closeSync(fd);
  return path;
}

const bigA = writeBig('big-a.txt');
const bigB = writeBig('big-b.txt');

const child = spawn(exe, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
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
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result?.value;
};

let passed = 0;
let failed = 0;
const ok = (name, extra = '') => {
  passed += 1;
  console.log(`PASS  ${name}${extra ? `  ← ${extra}` : ''}`);
};
const bad = (name, extra = '') => {
  failed += 1;
  console.log(`FAIL  ${name}${extra ? `  ← ${extra}` : ''}`);
};

/** 发送 Ctrl+E（CDP 键盘注入；window keydown 捕获阶段应拦截） */
async function sendCtrlE(client) {
  const base = { modifiers: 2, key: 'e', code: 'KeyE', windowsVirtualKeyCode: 69, nativeVirtualKeyCode: 69 };
  await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...base });
  await client.send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
}

try {
  const main = await createClient(await findTarget(port));
  const evalMain = (expression) => evalIn(main, expression);
  await waitForValue(
    async () => ((await evalMain('(() => !!(window.__srt && window.__srt.openPath))()')) ? true : null),
    30000,
  );
  await dismissOnboarding(evalMain);
  await delay(300);

  // ---- L1/L2/L3：只读打开 ----
  await evalMain(openPathDone(bigA));
  await waitForValue(
    async () => ((await evalMain(`document.querySelectorAll('.tab-bar .tab').length`)) >= 1 ? true : null),
    30000,
  );
  await delay(1200);
  const toastText = await evalMain(
    `(() => [...document.querySelectorAll('.toast .text')].map((e) => e.textContent).join('|'))()`,
  );
  if (typeof toastText === 'string' && toastText.includes('只读模式打开')) {
    ok('L1 打开 101MiB 文件提示只读', toastText.slice(0, 60));
  } else {
    bad('L1 只读提示', `toast=${toastText}`);
  }

  const badge = await evalMain(
    `(() => { const el = document.querySelector('.status-bar .readonly'); return el ? el.textContent.trim() : null; })()`,
  );
  if (badge === '只读') ok('L2 状态栏「只读」标记');
  else bad('L2 只读标记', `badge=${badge}`);

  const editBtn = await evalMain(
    `(() => { const el = document.querySelector('button[aria-label="切换编辑模式"]'); return el ? JSON.stringify({ disabled: el.disabled, title: el.title }) : null; })()`,
  );
  const editInfo = editBtn ? JSON.parse(editBtn) : null;
  if (editInfo && editInfo.disabled === true && String(editInfo.title).includes('只读阈值')) {
    ok('L3 编辑按钮禁用并给出原因');
  } else {
    bad('L3 编辑按钮', JSON.stringify(editInfo));
  }

  // ---- L4：Ctrl+E 被拦截 ----
  await sendCtrlE(main);
  await delay(600);
  const toast2 = await evalMain(
    `(() => [...document.querySelectorAll('.toast .text')].map((e) => e.textContent).join('|'))()`,
  );
  if (typeof toast2 === 'string' && toast2.includes('不可编辑')) {
    ok('L4 Ctrl+E 拦截并提示');
  } else {
    bad('L4 Ctrl+E 拦截', `toast=${toast2}`);
  }

  // ---- L5：硬上限调低 → 拒绝打开 ----
  writeFileSync(
    join(dataDir, 'settings.json'),
    JSON.stringify({ schemaVersion: 1, maxFileSizeMB: 100, hardLimitMB: 100 }),
    'utf8',
  );
  await delay(400);
  await evalMain(openPathDone(bigB));
  await delay(1200);
  const toast3 = await evalMain(
    `(() => [...document.querySelectorAll('.toast .text')].map((e) => e.textContent).join('|'))()`,
  );
  const tabCount = await evalMain(`document.querySelectorAll('.tab-bar .tab').length`);
  if (typeof toast3 === 'string' && toast3.includes('文件过大无法打开') && tabCount === 1) {
    ok('L5 超过硬上限拒绝打开（逐字提示）', `tabs=${tabCount}`);
  } else {
    bad('L5 硬上限拒绝', `toast=${toast3} tabs=${tabCount}`);
  }

  // ---- L6：设置窗口出现双滑块 ----
  await evalMain(`(() => { void window.__TAURI_INTERNALS__.invoke('open_settings', {}); return true; })()`);
  await delay(1500);
  const settingsTarget = await findTarget(port, 'settings.html');
  const settings = await createClient(settingsTarget);
  const evalSettings = (expression) => evalIn(settings, expression);
  await waitForValue(
    async () =>
      ((await evalSettings(`document.querySelectorAll('[data-setting]').length`)) >= 2 ? true : null),
    15000,
  );
  const sliders = await evalSettings(
    `(() => [...document.querySelectorAll('[data-setting]')].map((el) => el.getAttribute('data-setting')).join('|'))()`,
  );
  if (typeof sliders === 'string' && sliders.includes('maxFileSizeMB') && sliders.includes('hardLimitMB')) {
    ok('L6 设置窗口双阈值滑块', sliders);
  } else {
    bad('L6 双阈值滑块', `sliders=${sliders}`);
  }
  await evalSettings(`(() => { void window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'settings' }); return true; })()`);

  console.log(`\n双阈值套件：通过 ${passed}/${passed + failed}`);
  process.exitCode = failed === 0 ? 0 : 1;
} catch (error) {
  console.error('套件异常：', error instanceof Error ? error.message : error);
  process.exitCode = 1;
} finally {
  try {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } catch {
    // 忽略
  }
  await delay(500);
  for (let i = 0; i < 10; i += 1) {
    try {
      rmSync(work, { recursive: true, force: true });
    } catch {
      // 忽略
    }
    await delay(250);
  }
  process.exit(process.exitCode ?? 0);
}
