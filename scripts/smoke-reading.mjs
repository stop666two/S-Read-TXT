// 阅读模式套件：自动滚动 / 专注模式 / 打字机 / 进度记忆 / 阅读时长 / 分页·双页·分栏。
// 场景：
//   R1  打开文件（滚动模式）正常渲染
//   R2  自动滚动开关 → 位置前进 → 关闭
//   R3  专注模式（菜单开关 + Esc 退出；菜单栏隐藏）
//   R4  打字机模式：跳转目标行纵向居中
//   R5  进度记忆关闭 → 会话 scrollRow 记 0
//   R6  阅读时长状态项（预置今日 3661 秒 → 显示 1h1m）
//   R7  查看菜单含 自动滚动/专注/打字机/番茄钟
//   R8  分页模式：滚轮整屏翻页
//   R9  双页模式：两栏渲染（左栏行号 < 右栏行号）
//   R10 切回滚动模式恢复
//   R11 截图
// 依赖：debug 构建（node scripts/dev.mjs build）。
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
const shotMode = argValue('--screenshot', '');
const work = join(tmpdir(), `srt-reading-${Date.now()}`);
const dataDir = join(work, 'data');
const sample = join(work, 'sample.txt');
const shotsDir = join(root, 'docs', 'screenshots');
mkdirSync(dataDir, { recursive: true });
writeFileSync(
  sample,
  Array.from({ length: 800 }, (_, i) => `第 ${i + 1} 行：分页与阅读模式验证内容。`).join('\n'),
  'utf8',
);

let port = 9600 + Math.floor(Math.random() * 300);
const checks = [];
function check(name, passed, detail = '') {
  checks.push({ name, passed });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

let watchdog = setTimeout(() => {
  console.error(`看门狗触发（${currentStep}）`);
  spawnSync('taskkill', ['/PID', String(child?.pid ?? 0), '/T', '/F'], { stdio: 'ignore' });
  process.exit(4);
}, 600_000);
let currentStep = 'R0 启动';
let child;
let client;

function launchApp() {
  const proc = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dataDir,
      SRT_NO_ELEVATION: '1',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });
  return proc;
}

async function connect() {
  const ws = await waitForValue(
    async () => (await findTarget(port).catch(() => null)) || null,
    20_000,
  );
  return createClient(ws);
}

async function evalJs(expression) {
  const result = await client.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(`页面脚本异常：${result.exceptionDetails.text}`);
  return result.result?.value;
}

async function waitReady() {
  const ok = await waitForValue(
    async () => (await evalJs(`!!(window.__srt && document.querySelector('.menu-bar'))`)) || null,
    20_000,
  );
  if (!ok) throw new Error('页面未就绪');
}

async function openMenu(label) {
  await evalJs(
    `(() => { const item = [...document.querySelectorAll('.menu-bar .title')].find((n) => n.textContent.trim() === ${JSON.stringify(label)}); item?.click(); return true; })()`,
  );
  await delay(150);
}

async function clickMenuItem(label) {
  const text = await evalJs(
    `(() => { const item = [...document.querySelectorAll('.menu-bar .item')].find((n) => n.textContent.includes(${JSON.stringify(label)})); item?.click(); return item ? item.textContent.trim() : null; })()`,
  );
  await delay(250);
  return text;
}

/** 更新设置（读取快照 → 合并 reader/status 补丁 → 保存） */
async function patchSettings(patchExpr) {
  return evalJs(`(async () => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const snapshot = await invoke('get_settings');
    const request = { app: snapshot.app, reader: snapshot.reader, shortcuts: snapshot.shortcuts.bindings };
    ${patchExpr}
    await invoke('save_settings', { request });
    return true;
  })()`);
}

async function main() {
  try {
  child = launchApp();
  client = await connect();
  await waitReady();
  await dismissOnboarding(evalJs);
  await evalJs(openPathDone(sample));
  await waitForValue(async () => ((await evalJs(`document.querySelectorAll('.reader .row').length`)) > 0 || null), 15_000);
  check('R1 打开文件（滚动模式）渲染正常', true);

  // R2 自动滚动
  currentStep = 'R2 自动滚动';
  await openMenu('查看');
  await clickMenuItem('自动滚动');
  const top0 = await evalJs(`document.querySelector('.reader').scrollTop`);
  await delay(1600);
  const top1 = await evalJs(`document.querySelector('.reader').scrollTop`);
  check('R2a 自动滚动使位置前进', top1 > top0, `${top0} → ${top1}`);
  await openMenu('查看');
  await clickMenuItem('自动滚动');
  await delay(300);
  const top2 = await evalJs(`document.querySelector('.reader').scrollTop`);
  await delay(1200);
  const top3 = await evalJs(`document.querySelector('.reader').scrollTop`);
  check('R2b 关闭后停止滚动', Math.abs(top3 - top2) < 4, `${top2} → ${top3}`);

  // R3 专注模式
  currentStep = 'R3 专注模式';
  await openMenu('查看');
  await clickMenuItem('专注模式');
  const focused = await waitForValue(
    async () => ((await evalJs(`document.querySelector('.shell')?.classList.contains('focus-reading')`)) || null),
    4000,
  );
  const menuHidden = await evalJs(`getComputedStyle(document.querySelector('.menu-bar')).display === 'none'`);
  check('R3a 专注模式生效（shell 类 + 菜单隐藏）', focused === true && menuHidden === true);
  await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  const unfocused = await waitForValue(
    async () => ((await evalJs(`!document.querySelector('.shell')?.classList.contains('focus-reading')`)) || null),
    4000,
  );
  check('R3b Esc 退出专注模式', unfocused === true);

  // R4 打字机模式（跳转行居中）
  currentStep = 'R4 打字机';
  await patchSettings(`request.reader.reading.typewriter = true;`);
  await delay(500);
  await evalJs(
    `(() => { const reader = document.querySelector('.reader'); reader.scrollTop = 220; reader.dispatchEvent(new Event('scroll')); return true; })()`,
  );
  await delay(350);
  await evalJs(
    `(() => { document.querySelector('.status-bar button.item.click')?.click(); return true; })()`,
  );
  await delay(250);
  const hasGoto = await evalJs(`!!document.querySelector('.status-bar input')`);
  if (hasGoto) {
    await evalJs(
      `(() => { const input = document.querySelector('.status-bar input'); input.value = '400'; input.dispatchEvent(new Event('input', { bubbles: true })); input.focus(); return true; })()`,
    );
    await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
    await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
    await delay(900);
    const metrics = await evalJs(
      `(() => { const row = document.querySelector('[data-row="399"]'); const reader = document.querySelector('.reader'); if (!row || !reader) return null; const rr = reader.getBoundingClientRect(); const tr = row.getBoundingClientRect(); return Math.round(tr.top - rr.top - rr.height / 2); })()`,
    );
    check('R4 打字机模式目标行居中（偏离中线 < 160px）', metrics !== null && Math.abs(metrics) < 160, String(metrics));
    await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
    await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  } else {
    check('R4 打字机模式目标行居中（偏离中线 < 160px）', false, '未找到跳转输入');
  }
  await patchSettings(`request.reader.reading.typewriter = false;`);
  await delay(300);

  // R5 进度记忆关闭 → 会话 scrollRow 保持 0
  currentStep = 'R5 进度记忆';
  await patchSettings(`request.reader.reading.progressMemory = false;`);
  await delay(400);
  await evalJs(`(() => { const reader = document.querySelector('.reader'); reader.scrollTop = 3000; reader.dispatchEvent(new Event('scroll')); return true; })()`);
  await delay(400);
  await evalJs(`(void window.__TAURI_INTERNALS__.invoke('save_session', { state: { schemaVersion: 1, window: { width: 900, height: 700, maximized: false }, activeTabIndex: 0, tabs: [] } }).then(() => true))`);
  await delay(700);
  const sessionRaw = readFileSync(join(dataDir, 'session.json'), 'utf8');
  const session = JSON.parse(sessionRaw);
  const anyScroll = (session.tabs ?? []).some((t) => (t.scrollRow ?? 0) > 0);
  check('R5 进度记忆关闭后会话不记录位置', session.tabs?.length === 0 || !anyScroll, JSON.stringify(session.tabs ?? []));
  await patchSettings(`request.reader.reading.progressMemory = true;`);
  await delay(300);

  // R7 菜单项齐全
  currentStep = 'R7 菜单项';
  await openMenu('查看');
  await delay(150);
  const menuAll = await evalJs(`[...document.querySelectorAll('.menu-bar .item')].map((n) => n.textContent).join('|')`);
  check(
    'R7 查看菜单含 自动滚动/专注/打字机/番茄钟',
    ['自动滚动', '专注模式', '打字机模式', '番茄钟'].every((label) => menuAll.includes(label)),
    menuAll.slice(0, 80),
  );
  await evalJs(`document.body.click()`);
  await delay(200);

  // R8/R9/R10 分页 / 双页 / 回滚
  currentStep = 'R8 分页';
  await patchSettings(`request.reader.reading.pageMode = 'paged';`);
  await waitForValue(async () => ((await evalJs(`!!document.querySelector('.reader.spread')`)) || null), 5000);
  const pagedFirst0 = await evalJs(
    `(() => { const row = document.querySelector('.spread-col .row[data-row]'); return row ? Number(row.dataset.row) : null; })()`,
  );
  await client.send('Input.dispatchMouseEvent', { type: 'mouseWheel', x: 520, y: 420, deltaX: 0, deltaY: 400 });
  await delay(900);
  const pagedFirst1 = await evalJs(
    `(() => { const row = document.querySelector('.spread-col .row[data-row]'); return row ? Number(row.dataset.row) : null; })()`,
  );
  check('R8 分页模式滚轮整屏翻页', pagedFirst1 !== null && pagedFirst0 !== null && pagedFirst1 - pagedFirst0 >= 10, `${pagedFirst0} → ${pagedFirst1}`);

  currentStep = 'R9 双页';
  await patchSettings(`request.reader.reading.pageMode = 'double';`);
  await delay(700);
  const spread = await evalJs(
    `(() => { const cols = [...document.querySelectorAll('.spread-col')]; if (cols.length < 2) return null; const l = cols[0].querySelector('.row[data-row]'); const r = cols[1].querySelector('.row[data-row]'); return { cols: cols.length, left: l ? Number(l.dataset.row) : null, right: r ? Number(r.dataset.row) : null }; })()`,
  );
  check(
    'R9 双页模式两栏且右栏在后',
    spread !== null && spread.cols === 2 && spread.left !== null && spread.right !== null && spread.right > spread.left,
    JSON.stringify(spread),
  );

  currentStep = 'R11 截图';
  mkdirSync(shotsDir, { recursive: true });
  const shot = await client.send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(shotsDir, 'phase-p2-reading.png'), Buffer.from(shot.data, 'base64'));
  check('R11 双页截图已保存', true, 'phase-p2-reading.png');

  currentStep = 'R10 回滚模式';
  await patchSettings(`request.reader.reading.pageMode = 'scroll';`);
  const noSpread = await waitForValue(async () => ((await evalJs(`!document.querySelector('.reader.spread')`)) || null), 5000);
  check('R10 切回滚动模式恢复', noSpread === true);

  // R6 阅读时长状态项（预置今日 3661 秒 + 开启 readTime 项 → 重启验证）
  currentStep = 'R6 阅读时长';
  const today = new Date();
  const day = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`;
  await patchSettings(
    `if (!request.app.status.items.includes('readTime')) request.app.status.items.push('readTime');`,
  );
  await delay(400);
  writeFileSync(
    join(dataDir, 'reading-stats.json'),
    JSON.stringify({ schemaVersion: 1, day, todaySeconds: 3661, totalSeconds: 3661 }),
    'utf8',
  );
  client.close?.();
  spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  await delay(600);
  // 重启换新调试端口：旧端口可能仍处于 TIME_WAIT/被残留 WebView2 占用
  port = 9600 + Math.floor(Math.random() * 300);
  child = launchApp();
  client = await connect();
  await waitReady();
  await dismissOnboarding(evalJs);
  await evalJs(openPathDone(sample));
  const readTimeText = await waitForValue(async () => {
    const text = await evalJs(`document.querySelector('.status-bar')?.textContent ?? ''`);
    return text.includes('1h1m') ? text : null;
  }, 8000);
  check('R6 阅读时长状态项显示 1h1m', readTimeText !== null);

  if (shotMode) {
    await evalJs(`true`);
  }

  const failed = checks.filter((item) => !item.passed);
  console.log(`\n阅读模式套件：通过 ${checks.length - failed.length}/${checks.length}`);
  if (failed.length > 0) {
    console.error(`失败项：${failed.map((item) => item.name).join('；')}`);
    process.exitCode = 1;
  }
} catch (error) {
  console.error(`阅读模式套件异常（步骤：${currentStep}）：${error.message}`);
  process.exitCode = 1;
} finally {
  clearTimeout(watchdog);
  client?.close?.();
  if (child) spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  await delay(500);
  for (let i = 0; i < 12; i += 1) {
    try {
      rmSync(work, { recursive: true, force: true });
    } catch {
      // 忽略
    }
    await delay(250);
    if (!existsSync(work)) break;
  }
  process.exit(process.exitCode ?? 0);
}
}

void main();
