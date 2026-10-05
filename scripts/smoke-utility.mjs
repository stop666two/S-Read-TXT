// 工具功能 E2E（P3-6 常驻套件）。
// 场景：
//   U1 首运异常提示：无快照时不提示；存在可恢复快照时提示
//   U2 拆分行数模式（预览 + 执行 + 磁盘校验）
//   U3 拆分预览后关闭不写盘
//   U4 拆分标记模式（纯文本标记执行）
//   U5 拆分非法正则提示与有效正则预览
//   U6 批量重命名：扫描 5 文件 + 同名冲突预览标红且禁止执行
//   U7 重命名执行（前缀 + 序号）→ 磁盘校验 → 撤销恢复
//   U8 工具菜单含「拆分文件…」「批量重命名…」
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  createClient,
  delay,
  dismissOnboarding,
  findTarget,
  waitForValue,
} from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-utility-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });

// 拆分样例：3000 行（行数模式 1000/片 → 3 片）
const bigFile = join(work, 'big.txt');
writeFileSync(bigFile, Array.from({ length: 3000 }, (_, i) => `line-${i + 1}`).join('\n') + '\n');
// 标记样例
const markerFile = join(work, 'parts.txt');
writeFileSync(markerFile, 'A1\nA2\n=== SPLIT\nB1\n=== SPLIT\nC1\n');

// 端口段避开 Windows 保留区间 10008–10107，并与既有套件错开；
// 每次启动用递增端口，避免上一次实例的 WebView2 子进程未即时释放调试端口。
const port = 9450 + Math.floor(Math.random() * 80);
let launchCount = 0;

let child = null;
let passed = 0;
let failed = 0;
let step = '启动';

const check = (name, cond, extra = '') => {
  if (cond === true) {
    passed += 1;
    console.log(`PASS  ${name}${extra ? `  → ${extra}` : ''}`);
  } else {
    failed += 1;
    console.log(`FAIL  ${name}${extra ? `  → ${extra}` : ''}`);
  }
};

const killAll = () => {
  try {
    spawnSync('taskkill', ['/IM', 's-read-txt.exe', '/T', '/F'], { stdio: 'ignore' });
  } catch {
    /* 忽略 */
  }
};

const watchdog = setTimeout(() => {
  console.log(`FAIL  看门狗超时（420s，当前步骤：${step}）`);
  process.exitCode = 4;
  killAll();
}, 420_000);

killAll();
await delay(600);

const evalIn = async (client, expression) => {
  const result = await client.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result?.value;
};

const launch = async (dir) => {
  const debugPort = port + launchCount;
  launchCount += 1;
  child = spawn(exe, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dir,
      SRT_NO_ELEVATION: '1',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
    },
    stdio: 'ignore',
  });
  let ws = null;
  for (let attempt = 0; attempt < 3 && ws === null; attempt += 1) {
    try {
      ws = await findTarget(debugPort);
    } catch (error) {
      if (attempt === 2) throw error;
      console.log(`  重启诊断：第 ${attempt + 1} 次未发现调试目标（等待后重试）`);
      await delay(2000);
    }
  }
  const client = await createClient(ws);
  await waitForValue(
    async () => ((await evalIn(client, '!!window.__srt?.openPath')) ? true : null),
    20000,
  );
  await dismissOnboarding((expr) => evalIn(client, expr));
  return client;
};

const stopApp = async () => {
  killAll();
  for (let i = 0; i < 40; i += 1) {
    const out = spawnSync('tasklist', ['/FI', 'IMAGENAME eq s-read-txt.exe', '/NH'], {
      encoding: 'utf8',
    });
    if (!String(out.stdout).includes('s-read-txt.exe')) {
      await delay(1000);
      return;
    }
    await delay(250);
  }
};

/** 页面内全部 toast 文本 */
const toastText = (client) =>
  evalIn(
    client,
    `[...document.querySelectorAll('.toast .text')].map((n) => n.textContent).join('|')`,
  );

/** 菜单点击（打开标题 → 点击条目；含重试） */
const menuClick = async (client, title, itemText) => {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    await evalIn(client, `(document.body.click(), true)`);
    await delay(80);
    await evalIn(
      client,
      `(() => { const t = [...document.querySelectorAll('.menu-bar .title')].find((n) => n.textContent.trim() === ${JSON.stringify(title)}); t?.click(); return !!t; })()`,
    );
    const clicked = await waitForValue(
      () =>
        evalIn(
          client,
          `(() => { const it = [...document.querySelectorAll('.menu-bar .item')].find((n) => n.textContent.trim().startsWith(${JSON.stringify(itemText)})); if (!it) return null; it.click(); return true; })()`,
        ),
      2000,
    );
    if (clicked === true) return true;
  }
  return false;
};

/** 以真实输入事件设置输入框值（兼容 Svelte bind:value） */
const setInput = (client, selector, value) =>
  evalIn(
    client,
    `(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return null; const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype; const setter = Object.getOwnPropertyDescriptor(proto, 'value').set; setter.call(el, ${JSON.stringify(String(value))}); el.dispatchEvent(new Event('input', { bubbles: true })); return el.value; })()`,
  );

const dialogOpen = (client) =>
  evalIn(client, `document.querySelector('[data-split-dialog]') !== null`);

try {
  // ---- U1 首运异常提示 ----
  step = 'U1 首运异常提示';
  let client = await launch(dataDir);
  await delay(2200);
  const firstToasts = await toastText(client);
  check('U1a 全新数据目录首启不提示异常退出', !String(firstToasts).includes('异常退出'), `toasts=${firstToasts}`);
  await stopApp();

  // 制造「异常退出 + 存在快照」：写快照文件且不写 .clean-exit
  const snapDir = join(dataDir, 'snapshots', 'deadbeef');
  mkdirSync(snapDir, { recursive: true });
  writeFileSync(join(snapDir, '1700000000000-0001.snap'), 'crash-recover\n');

  client = await launch(dataDir);
  const recovered = await waitForValue(async () => {
    const text = await toastText(client);
    return String(text).includes('异常退出') ? text : null;
  }, 6000);
  check('U1b 存在可恢复快照时提示异常退出', recovered !== null, `toasts=${recovered ?? ''}`);

  // ---- U2 拆分行数模式 ----
  step = 'U2 拆分行数模式';
  const opened = await menuClick(client, '工具', '拆分文件…');
  await waitForValue(async () => ((await dialogOpen(client)) ? true : null), 4000);
  check('U2a 工具菜单打开拆分对话框', opened === true && (await dialogOpen(client)) === true);
  await setInput(client, '[data-split-path]', bigFile);
  await setInput(client, '[data-split-lines]', '1000');
  await evalIn(client, `(document.querySelector('[data-split-preview-btn]')?.click(), true)`);
  const previewed = await waitForValue(async () => {
    const parts = await evalIn(client, `document.querySelectorAll('[data-split-part]').length`);
    const summary = await evalIn(
      client,
      `document.querySelector('[data-split-summary]')?.textContent ?? ''`,
    );
    return parts === 3 && summary.includes('共 3 片') ? { parts, summary } : null;
  }, 8000);
  if (previewed === null) {
    const diag = await evalIn(
      client,
      `JSON.stringify({ path: document.querySelector('[data-split-path]')?.value ?? null, lines: document.querySelector('[data-split-lines]')?.value ?? null, err: document.querySelector('[data-split-error]')?.textContent ?? '', parts: document.querySelectorAll('[data-split-part]').length })`,
    );
    check('U2b 预览 3000 行→3 片（每片 1000 行）', false, diag);
  } else {
    check('U2b 预览 3000 行→3 片（每片 1000 行）', true, JSON.stringify(previewed));
    const shot = await client.send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(join(root, 'docs', 'screenshots', 'phase-p36-split.png'), Buffer.from(shot.data, 'base64'));
  }
  await evalIn(client, `(document.querySelector('[data-split-apply]')?.click(), true)`);
  const applied = await waitForValue(
    () => evalIn(client, `document.querySelector('[data-split-result]') !== null`),
    8000,
  );
  const partsOnDisk = ['big-0001.txt', 'big-0002.txt', 'big-0003.txt'].map((name) =>
    existsSync(join(work, name)),
  );
  const firstPartLines = existsSync(join(work, 'big-0001.txt'))
    ? readFileSync(join(work, 'big-0001.txt'), 'utf8').trim().split('\n').length
    : 0;
  check(
    'U2c 执行拆分：3 个文件落盘且首片 1000 行',
    applied === true && partsOnDisk.every(Boolean) && firstPartLines === 1000,
    `files=${JSON.stringify(partsOnDisk)} lines=${firstPartLines}`,
  );

  // ---- U3 预览后关闭不写盘 ----
  step = 'U3 预览后关闭不写盘';
  await evalIn(client, `(document.querySelector('[data-split-close]')?.click(), true)`);
  await delay(300);
  const probeName = 'parts-0001.txt';
  const beforeCount = readdirSync(work).filter((name) => name.startsWith('parts-')).length;
  await menuClick(client, '工具', '拆分文件…');
  await waitForValue(async () => ((await dialogOpen(client)) ? true : null), 4000);
  await setInput(client, '[data-split-path]', markerFile);
  await evalIn(client, `(document.querySelector('[data-split-preview-btn]')?.click(), true)`);
  await waitForValue(
    async () =>
      ((await evalIn(client, `document.querySelectorAll('[data-split-part]').length`)) > 0
        ? true
        : null),
    6000,
  );
  await evalIn(client, `(document.querySelector('[data-split-close]')?.click(), true)`);
  await delay(400);
  const afterCount = readdirSync(work).filter((name) => name.startsWith('parts-')).length;
  check('U3 仅预览未写盘', beforeCount === 0 && afterCount === 0 && !existsSync(join(work, probeName)), `before=${beforeCount} after=${afterCount}`);

  // ---- U4 标记模式执行 ----
  step = 'U4 标记模式执行';
  await menuClick(client, '工具', '拆分文件…');
  await waitForValue(async () => ((await dialogOpen(client)) ? true : null), 4000);
  await setInput(client, '[data-split-path]', markerFile);
  await evalIn(client, `(document.querySelector('[data-split-mode="marker"]')?.click(), true)`);
  await setInput(client, '[data-split-marker]', '=== SPLIT');
  await evalIn(client, `(document.querySelector('[data-split-preview-btn]')?.click(), true)`);
  const markerPreview = await waitForValue(async () => {
    const parts = await evalIn(client, `document.querySelectorAll('[data-split-part]').length`);
    return parts === 3 ? parts : null;
  }, 8000);
  await evalIn(client, `(document.querySelector('[data-split-apply]')?.click(), true)`);
  await waitForValue(
    () => evalIn(client, `document.querySelector('[data-split-result]') !== null`),
    8000,
  );
  const part2 = existsSync(join(work, 'parts-0002.txt'))
    ? readFileSync(join(work, 'parts-0002.txt'), 'utf8')
    : '';
  check(
    'U4 标记模式：3 片且标记行归下一片',
    markerPreview === 3 && part2 === '=== SPLIT\nB1\n',
    `parts=${markerPreview} p2=${JSON.stringify(part2)}`,
  );

  // ---- U5 非法正则提示与有效正则预览 ----
  step = 'U5 正则校验';
  await evalIn(client, `(document.querySelector('[data-split-close]')?.click(), true)`);
  await delay(300);
  await menuClick(client, '工具', '拆分文件…');
  await waitForValue(async () => ((await dialogOpen(client)) ? true : null), 4000);
  await setInput(client, '[data-split-path]', markerFile);
  await evalIn(client, `(document.querySelector('[data-split-mode="marker"]')?.click(), true)`);
  await waitForValue(
    async () => ((await evalIn(client, `document.querySelector('[data-split-regex]') !== null`)) ? true : null),
    4000,
  );
  await evalIn(client, `(document.querySelector('[data-split-regex]')?.click(), true)`);
  await waitForValue(
    async () => ((await evalIn(client, `document.querySelector('[data-split-regex]')?.checked === true`)) ? true : null),
    4000,
  );
  await setInput(client, '[data-split-marker]', '(');
  await evalIn(client, `(document.querySelector('[data-split-preview-btn]')?.click(), true)`);
  const invalidShown = await waitForValue(async () => {
    const text = await evalIn(client, `document.querySelector('[data-split-error]')?.textContent ?? ''`);
    return text.includes('正则表达式无效') ? text : null;
  }, 6000);
  await setInput(client, '[data-split-marker]', '^=== ');
  await evalIn(client, `(document.querySelector('[data-split-preview-btn]')?.click(), true)`);
  const regexPreview = await waitForValue(async () => {
    const parts = await evalIn(client, `document.querySelectorAll('[data-split-part]').length`);
    return parts === 3 ? parts : null;
  }, 8000);
  check(
    'U5 非法正则提示 + 有效正则预览 3 片',
    invalidShown !== null && regexPreview === 3,
    `invalid=${invalidShown !== null} parts=${regexPreview}`,
  );
  await evalIn(client, `(document.querySelector('[data-split-close]')?.click(), true)`);
  await delay(200);

  // ---- U6 批量重命名：扫描 + 预览（同名冲突标红）----
  step = 'U6 重命名扫描与冲突预览';
  const renameDir = join(work, 'rename-dir');
  mkdirSync(renameDir, { recursive: true });
  for (const name of ['x.txt', 'xx.txt', 'keep.txt', 'a1.txt', 'b1.txt']) {
    writeFileSync(join(renameDir, name), `${name}\n`);
  }
  await menuClick(client, '工具', '批量重命名…');
  await waitForValue(
    async () => ((await evalIn(client, `document.querySelector('[data-rename-dialog]') !== null`)) ? true : null),
    4000,
  );
  await setInput(client, '[data-rename-dir]', renameDir);
  await evalIn(client, `(document.querySelector('[data-rename-scan]')?.click(), true)`);
  const scanned = await waitForValue(async () => {
    const text = await evalIn(client, `document.querySelector('.count')?.textContent ?? ''`);
    return text.includes('5') ? text : null;
  }, 6000);
  await setInput(client, '[data-rename-find]', 'x');
  await evalIn(client, `(document.querySelector('[data-rename-preview]')?.click(), true)`);
  const conflictRows = await waitForValue(async () => {
    const rows = await evalIn(
      client,
      `[...document.querySelectorAll('[data-rename-row]')].filter((n) => n.dataset.status === 'conflict').length`,
    );
    return rows === 2 ? rows : null;
  }, 6000);
  const blockedShown = await evalIn(
    client,
    `document.querySelector('[data-rename-blocked]') !== null`,
  );
  const shotRename = await client.send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(
    join(root, 'docs', 'screenshots', 'phase-p36-rename.png'),
    Buffer.from(shotRename.data, 'base64'),
  );
  check(
    'U6 扫描 5 个文件 + 预览同名冲突 2 条标红且禁止执行',
    scanned !== null && conflictRows === 2 && blockedShown === true,
    `scanned=${scanned !== null} conflicts=${conflictRows} blocked=${blockedShown}`,
  );

  // ---- U7 执行重命名（前缀 + 序号）并撤销 ----
  step = 'U7 重命名执行与撤销';
  await setInput(client, '[data-rename-find]', '');
  await setInput(client, '[data-rename-prefix]', 'pre-');
  await setInput(client, '[data-rename-suffix]', '-');
  await evalIn(client, `(document.querySelector('[data-rename-numbering]')?.click(), true)`);
  await evalIn(client, `(document.querySelector('[data-rename-preview]')?.click(), true)`);
  const okRows = await waitForValue(async () => {
    const rows = await evalIn(
      client,
      `[...document.querySelectorAll('[data-rename-row]')].filter((n) => n.dataset.status === 'ok').length`,
    );
    return rows === 5 ? rows : null;
  }, 6000);
  await evalIn(client, `(document.querySelector('[data-rename-apply]')?.click(), true)`);
  const renamedApplied = await waitForValue(async () => {
    const exists =
      existsSync(join(renameDir, 'pre-a1-01.txt')) && existsSync(join(renameDir, 'pre-xx-05.txt'));
    const gone = !existsSync(join(renameDir, 'x.txt')) && !existsSync(join(renameDir, 'a1.txt'));
    return exists && gone ? true : null;
  }, 8000);
  const undoVisible = await waitForValue(
    async () => ((await evalIn(client, `document.querySelector('[data-rename-undo]') !== null`)) ? true : null),
    6000,
  );
  await evalIn(client, `(document.querySelector('[data-rename-undo]')?.click(), true)`);
  const restored = await waitForValue(async () => {
    const back =
      existsSync(join(renameDir, 'x.txt')) && existsSync(join(renameDir, 'a1.txt')) && existsSync(join(renameDir, 'keep.txt'));
    const preGone = !existsSync(join(renameDir, 'pre-a1-01.txt'));
    return back && preGone ? true : null;
  }, 8000);
  check(
    'U7 执行改名（pre-*-序号）后撤销恢复原状',
    okRows === 5 && renamedApplied === true && undoVisible === true && restored === true,
    `ok=${okRows} applied=${renamedApplied} undo=${undoVisible} restored=${restored}`,
  );

  // ---- U8 工具菜单含三项入口（比较文件… 于后续任务加入）----
  step = 'U8 工具菜单入口';
  await evalIn(client, `(document.querySelector('[data-rename-close]')?.click(), true)`);
  await delay(250);
  await evalIn(client, `(document.body.click(), true)`);
  await delay(100);
  await evalIn(
    client,
    `(() => { const t = [...document.querySelectorAll('.menu-bar .title')].find((n) => n.textContent.trim() === '工具'); t?.click(); return true; })()`,
  );
  const toolItems = await waitForValue(async () => {
    const items = await evalIn(
      client,
      `[...document.querySelectorAll('.menu-bar .item')].map((n) => n.textContent.trim())`,
    );
    return Array.isArray(items) && items.some((item) => item.startsWith('拆分文件')) && items.some((item) => item.startsWith('批量重命名'))
      ? items
      : null;
  }, 4000);
  await evalIn(client, `(document.body.click(), true)`);
  check(
    'U8 工具菜单含「拆分文件…」「批量重命名…」',
    Array.isArray(toolItems),
    JSON.stringify(toolItems),
  );
} catch (error) {
  failed += 1;
  console.log(`FAIL  未预期异常：${error?.message ?? error}（当前步骤：${step}）`);
} finally {
  clearTimeout(watchdog);
  killAll();
  await delay(800);
}

console.log(`\n工具功能冒烟：${passed}/${passed + failed} 通过`);
if (failed > 0) {
  console.log(`工作目录保留：${work}`);
  process.exitCode = 1;
} else {
  try {
    rmSync(work, { recursive: true, force: true });
  } catch {
    console.log(`工作目录占用中，保留（${work}）`);
  }
}
