// 大纲/折叠/面包屑 E2E（常驻套件）。
// 场景：O1 打开；O2 大纲面板条目与层级；O3 点击跳转；O4 面包屑随顶部行；O5 折叠标记与隐藏行；
//   O6 展开全部；O7 设置关闭（大纲入口禁用、面包屑隐藏）；O8 截图。
// 依赖：debug 构建（node scripts/dev.mjs build）。
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

import { createClient, delay, dismissOnboarding, findTarget, openPathDone, waitForValue } from './lib/smoke-cdp.mjs';

const exePath = resolve(process.cwd(), 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const workDir = join(tmpdir(), `srt-outline-${Date.now()}`);
const dataDir = join(workDir, 'data');
mkdirSync(dataDir, { recursive: true });
const sample = join(workDir, 'outline.txt');
const sampleLines = [];
for (let row = 0; row < 60; row += 1) {
  if (row === 0) sampleLines.push('第一章 开始');
  else if (row === 20) sampleLines.push('  第一节 小节');
  else if (row === 40) sampleLines.push('第二章 结束');
  else sampleLines.push(`正文行 ${row}`);
}
writeFileSync(sample, `${sampleLines.join('\n')}\n`, 'utf8');
const shotPath = resolve('docs', 'screenshots', 'phase-p2-outline.png');
const port = 9400 + Math.floor(Math.random() * 300);
const pidFile = join(workDir, 'app.pid');

const checks = [];
function check(name, ok, detail = '') {
  checks.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` | ${detail}` : ''}`);
}

let child = null;
let client = null;
const watchdog = setTimeout(() => {
  console.error(`watchdog 触发（最后步骤：${currentStep}）`);
  process.exitCode = 4;
  cleanup();
}, 300_000);
let currentStep = 'startup';

function spawnApp() {
  child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dataDir,
      SRT_NO_ELEVATION: '1',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
    detached: false,
  });
  writeFileSync(pidFile, String(child.pid));
}

function killTree() {
  if (child?.pid) {
    try {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    } catch {
      /* 已退出 */
    }
  }
}

function removeWithRetry(path) {
  for (let i = 0; i < 12; i += 1) {
    try {
      rmSync(path, { recursive: true, force: true });
      return;
    } catch {
      const until = Date.now() + 250;
      while (Date.now() < until) {
        /* 等待句柄释放 */
      }
    }
  }
}

function cleanup() {
  try {
    killTree();
    removeWithRetry(workDir);
  } catch {
    /* 清理尽力而为 */
  }
}

async function evalJs(expression) {
  const result = await client.send('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.text ?? '页面脚本异常');
  }
  return result.result?.value;
}

async function waitFor(fn, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await fn();
    if (last) return last;
    await delay(120);
  }
  throw new Error(`等待超时：${label}（最后值：${JSON.stringify(last)}）`);
}

const waitReady = () =>
  waitFor(
    async () => evalJs(`!!(window.__srt && window.__TAURI_INTERNALS__)`),
    25_000,
    '前端桥接',
  );

async function clickText(text) {
  return evalJs(`(() => {
    const nodes = [...document.querySelectorAll('button, [role="menuitem"]')];
    const hit = nodes.find((node) => node.textContent?.trim() === ${JSON.stringify(text)});
    if (!hit) return false;
    hit.click();
    return true;
  })()`);
}

async function openViewMenu() {
  await evalJs(`(document.querySelector('.menu-bar button') !== null, true)`);
  const ok = await evalJs(`(() => {
    const bars = [...document.querySelectorAll('.menu-bar button')];
    const view = bars.find((node) => node.textContent?.trim().startsWith('查看'));
    if (!view) return false;
    view.click();
    return true;
  })()`);
  if (!ok) throw new Error('未找到「查看」菜单');
  await delay(150);
}

async function setDisplay(patch) {
  await evalJs(`(async () => {
    const snapshot = await window.__TAURI_INTERNALS__.invoke('get_settings');
    Object.assign(snapshot.app.display, ${JSON.stringify(patch)});
    await window.__TAURI_INTERNALS__.invoke('save_settings', {
      request: {
        app: snapshot.app,
        reader: snapshot.reader,
        shortcuts: snapshot.shortcuts?.bindings ?? snapshot.shortcuts,
      },
    });
    return true;
  })()`);
  await delay(600);
}

async function run() {
  spawnApp();
  currentStep = 'O0 等待 CDP 目标';
  const ws = await waitFor(() => findTarget(port, ''), 30_000, 'CDP 目标');
  client = await createClient(ws);
  await client.send('Runtime.enable');
  await waitReady();
  await dismissOnboarding(evalJs);
  await waitFor(() => evalJs(`!!document.querySelector('.empty .open-btn')`), 8_000, '欢迎页');

  currentStep = 'O1 打开样本';
  await evalJs(openPathDone(sample));
  await waitFor(() => evalJs(`document.querySelectorAll('.row').length > 10`), 10_000, '行渲染');
  check('O1 打开并渲染', (await evalJs(`document.querySelectorAll('.row').length`)) > 10);

  currentStep = 'O2 大纲面板';
  await openViewMenu();
  if (!(await clickText('大纲'))) throw new Error('未找到菜单项「大纲」');
  await waitFor(() => evalJs(`!!document.querySelector('[data-outline-panel]')`), 8_000, '大纲面板');
  const items = await waitFor(
    () => evalJs(`document.querySelectorAll('[data-outline-item]').length`),
    8_000,
    '大纲条目',
  );
  const rowsAttr = await evalJs(
    `[...document.querySelectorAll('[data-outline-item]')].map((n) => Number(n.dataset.outlineRow))`,
  );
  check('O2a 面板含 3 个章节', items === 3, String(items));
  check('O2b 条目行号 [0,20,40]', JSON.stringify(rowsAttr) === '[0,20,40]', JSON.stringify(rowsAttr));
  const pads = await evalJs(
    `[...document.querySelectorAll('[data-outline-item]')].map((n) => parseFloat(getComputedStyle(n).paddingLeft))`,
  );
  check('O2c 层级缩进递增', pads[1] > pads[0] && pads[2] === pads[0], JSON.stringify(pads));

  currentStep = 'O3 跳转';
  await evalJs(`(document.querySelector('[data-outline-item][data-outline-row="40"]').click(), true)`);
  await waitFor(
    () => evalJs(`document.querySelector('[data-breadcrumb]')?.textContent?.includes('第二章') ?? false`),
    8_000,
    '跳转后面包屑指向第二章',
  );
  check('O3 点击跳转（面包屑跟随）', true);
  await evalJs(`(document.querySelector('[data-outline-close]').click(), true)`);
  await waitFor(() => evalJs(`!document.querySelector('[data-outline-panel]')`), 8_000, '面板关闭');

  currentStep = 'O4 面包屑';
  const crumb = await evalJs(`document.querySelector('[data-breadcrumb]')?.textContent?.trim() ?? ''`);
  check('O4a 面包屑含当前章节', crumb.includes('第二章 结束'), crumb.slice(0, 40));
  await evalJs(`(document.querySelector('.reader').scrollTop = 0, true)`);
  await delay(500);
  const crumb2 = await waitFor(
    () => evalJs(`document.querySelector('[data-breadcrumb]')?.textContent?.trim() ?? ''`),
    8_000,
    '面包屑更新',
  );
  check('O4b 顶部行变化面包屑更新', crumb2.includes('第一章 开始'), crumb2.slice(0, 40));

  currentStep = 'O5 折叠标记';
  await setDisplay({ folding: 'heading' });
  const marks = await waitFor(
    () => evalJs(`document.querySelectorAll('[data-fold-mark]').length`),
    8_000,
    '折叠标记渲染',
  );
  check('O5a 折叠标记 3 个', marks === 3, String(marks));
  await evalJs(`(document.querySelector('[data-fold-mark="0"]').click(), true)`);
  await waitFor(() => evalJs(`document.querySelectorAll('.row').length === 21`), 8_000, '折叠隐藏行');
  check('O5b 折叠后渲染 21 行（0 与 40-59）', true);

  currentStep = 'O6 展开全部';
  await openViewMenu();
  if (!(await clickText('展开全部'))) throw new Error('未找到菜单项「展开全部」');
  await waitFor(() => evalJs(`document.querySelectorAll('.row').length > 20`), 8_000, '展开恢复');
  check('O6 展开全部恢复渲染', true);

  currentStep = 'O7 设置开关';
  await setDisplay({ breadcrumb: false });
  check('O7a 面包屑开关生效', (await evalJs(`!!document.querySelector('[data-breadcrumb]')`)) === false);
  await setDisplay({ outline: false });
  await openViewMenu();
  const outlineDisabled = await evalJs(`(() => {
    const items = [...document.querySelectorAll('.menu-bar button')];
    const hit = items.find((node) => node.textContent?.trim() === '大纲' && node.disabled !== undefined);
    return hit ? hit.disabled : null;
  })()`);
  check('O7b 大纲开关禁用菜单项', outlineDisabled === true, String(outlineDisabled));
  await evalJs(`(document.body.click(), true)`);
  await setDisplay({ outline: true, breadcrumb: true, folding: 'off' });

  currentStep = 'O8 截图';
  await delay(400);
  const shot = await client.send('Page.captureScreenshot', { format: 'png' });
  mkdirSync(dirname(shotPath), { recursive: true });
  writeFileSync(shotPath, Buffer.from(shot.data, 'base64'));
  check('O8 截图保存', existsSync(shotPath), shotPath);
}

run()
  .catch((error) => {
    console.error(`未捕获错误（${currentStep}）：${error?.message ?? error}`);
    process.exitCode = 1;
  })
  .finally(() => {
    clearTimeout(watchdog);
    cleanup();
    const failed = checks.filter((item) => !item.ok);
    console.log(`\n结果：${checks.length - failed.length}/${checks.length} 通过`);
    if (failed.length > 0) {
      for (const item of failed) console.log(`  失败：${item.name} ${item.detail}`);
      process.exitCode = 1;
    }
    client?.close?.();
  });
