// 比较/三方合并端到端冒烟（P3-6）：
// 覆盖工具菜单入口、比较窗口并排/统一视图、统计与导航、一致文件提示、
// 三方合并冲突逐块选择（我方/他方/双方/不用）、写回（.bak 备份）与撤销。
//
// 依赖：已构建的 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 数据隔离：SRT_DATA_DIR 指向项目 tmp 工作目录；结束清理。

import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  argValue,
  createClient,
  delay,
  dismissOnboarding,
  findTarget,
  waitForValue,
} from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-compare-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
const port = Number(argValue('--port', String(9580 + Math.floor(Math.random() * 400))));

const leftFile = join(work, 'left.txt');
const rightFile = join(work, 'right.txt');
const sameA = join(work, 'same-a.txt');
const sameB = join(work, 'same-b.txt');
const baseFile = join(work, 'base.txt');
const oursFile = join(work, 'ours.txt');
const theirsFile = join(work, 'theirs.txt');
const pad = Array.from({ length: 80 }, (_, i) => `L${String(i).padStart(2, '0')}`);
writeFileSync(leftFile, [...pad, ''].join('\n'));
writeFileSync(
  rightFile,
  [...pad.map((line) => (line === 'L30' ? 'X30' : line)), 'EXTRA', ''].join('\n'),
);
writeFileSync(sameA, 'same\ncontent\n');
writeFileSync(sameB, 'same\ncontent\n');
writeFileSync(baseFile, 'a\nb\nc\nd\n');
writeFileSync(oursFile, 'a\nX\nc\nd\n');
writeFileSync(theirsFile, 'a\nY\nc\nD\n');

const results = [];
const check = (name, cond, extra = '') => {
  results.push(cond === true);
  console.log(`${cond === true ? 'PASS' : 'FAIL'}  ${name}${extra ? `  → ${extra}` : ''}`);
};

let child = null;
const killApp = async () => {
  if (child) {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    child = null;
  }
  for (let i = 0; i < 30; i += 1) {
    const out = spawnSync('tasklist', ['/FI', 'IMAGENAME eq s-read-txt.exe', '/NH'], {
      encoding: 'utf8',
    });
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
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result?.value;
};
const evalMain = (client, expr) => evalIn(client, `(window.__srt && (${expr}), true)`);

try {
  const mainClient = await createClient(await findTarget(port));
  await waitForValue(async () => ((await evalIn(mainClient, '!!window.__srt?.openPath')) ? true : null), 20000);
  await dismissOnboarding((expr) => evalIn(mainClient, expr));

  // C1：工具菜单入口 + 打开比较窗口
  await evalIn(mainClient, `(() => {
    const title = [...document.querySelectorAll('.menu-bar .title')].find((n) => n.textContent.trim() === '工具');
    title?.click();
    return true;
  })()`);
  await delay(200);
  const menuTexts = await evalIn(
    mainClient,
    `[...document.querySelectorAll('.menu-bar .item')].map((n) => n.textContent.trim())`,
  );
  await evalIn(mainClient, `(document.body.click(), true)`);
  check(
    'C1a 工具菜单含比较/合并入口',
    menuTexts.some((x) => x.startsWith('比较文件')) && menuTexts.some((x) => x.startsWith('三方合并')),
    JSON.stringify(menuTexts),
  );

  await evalMain(
    mainClient,
    `window.__TAURI_INTERNALS__.invoke('open_compare_window', { request: { mode: 'diff', left: ${JSON.stringify(leftFile)}, right: ${JSON.stringify(rightFile)}, extra: null } })`,
  );
  const compare = await createClient(await findTarget(port, 'compare.html'));
  const rowsAppeared = await waitForValue(async () => {
    const n = await evalIn(compare, `document.querySelectorAll('[data-diff-row]').length`);
    return n > 0 ? n : null;
  }, 12000);
  check('C1b 比较窗口打开并渲染差异行', rowsAppeared !== null, `rows=${rowsAppeared}`);

  // C2：统计 / 行号对齐 / 懒加载
  const stats = await evalIn(compare, `document.querySelector('[data-diff-stats]')?.textContent.trim() ?? ''`);
  check('C2a 统计正确（+2 −1）', stats.includes('+2') && stats.includes('1'), stats);
  const changeRows = await evalIn(compare, `(() => {
    const rows = [...document.querySelectorAll('[data-diff-row]')].filter((n) => n.dataset.kind === 'change');
    return rows.map((n) => ({ l: n.querySelector('[data-side="left"]')?.dataset.line ?? '', r: n.querySelector('[data-side="right"]')?.dataset.line ?? '' }));
  })()`);
  check(
    'C2b change 行左右行号对齐（30↔30）',
    changeRows.length === 1 && changeRows[0].l === '30' && changeRows[0].r === '30',
    JSON.stringify(changeRows),
  );
  const leftText = await waitForValue(async () => {
    await evalIn(compare, `(() => { const el = document.querySelector('[data-diff-scroll]'); el.scrollTop = 0; return true; })()`);
    const text = await evalIn(
      compare,
      `[...document.querySelectorAll('[data-side="left"]')].map((n) => n.querySelector('.code')?.textContent ?? '')`,
    );
    return Array.isArray(text) && text.includes('L00') ? text : null;
  }, 8000);
  check('C2c 左侧行文本按需加载', leftText !== null, JSON.stringify(leftText?.slice?.(0, 4)));

  // C2d：滚到底部验证纯插入行
  const insertRow = await waitForValue(async () => {
    await evalIn(compare, `(() => { const el = document.querySelector('[data-diff-scroll]'); el.scrollTop = el.scrollHeight; return true; })()`);
    const row = await evalIn(compare, `(() => {
      const found = [...document.querySelectorAll('[data-diff-row]')].find((n) => n.dataset.kind === 'insert');
      return found ? { l: found.querySelector('[data-side="left"]')?.dataset.line ?? '', r: found.querySelector('[data-side="right"]')?.dataset.line ?? '' } : null;
    })()`);
    return row ?? null;
  }, 8000);
  check('C2d 纯插入行（右 80、左空）', insertRow !== null && insertRow.l === '' && insertRow.r === '80', JSON.stringify(insertRow));

  // C3：统一视图 + 底部 EXTRA 行
  await evalIn(compare, `(document.querySelector('[data-compare-view="unified"]')?.click(), true)`);
  const unified = await waitForValue(async () => {
    const value = await evalIn(compare, `(() => {
      const rows = [...document.querySelectorAll('[data-diff-row]')];
      return { kinds: rows.map((n) => n.dataset.kind) };
    })()`);
    return value.kinds.includes('insert') && value.kinds.includes('delete') ? value : null;
  }, 6000);
  check('C3 统一视图含删除/插入行', unified !== null, JSON.stringify(unified?.kinds?.slice?.(0, 6)));
  await evalIn(compare, `(() => { const el = document.querySelector('[data-diff-scroll]'); el.scrollTop = el.scrollHeight; return true; })()`);
  const extraSign = await waitForValue(async () => {
    const found = await evalIn(compare, `(() => {
      const rows = [...document.querySelectorAll('[data-diff-row]')];
      const hit = rows.find((n) => (n.querySelector('.code')?.textContent ?? '') === 'EXTRA');
      return hit ? hit.querySelector('.sign')?.textContent ?? '' : null;
    })()`);
    return found ?? null;
  }, 8000);
  check('C3b 底部插入行 EXTRA 带 + 号', extraSign === '+', JSON.stringify(extraSign));

  // C4：导航
  await evalIn(compare, `(document.querySelector('[data-compare-view="side"]')?.click(), true)`);
  await delay(300);
  await evalIn(compare, `(() => { const el = document.querySelector('[data-diff-scroll]'); el.scrollTop = 0; return true; })()`);
  await delay(300);
  const before = await evalIn(compare, `document.querySelector('[data-diff-scroll]')?.scrollTop ?? -1`);
  await evalIn(compare, `(document.querySelector('[data-diff-next]')?.click(), true)`);
  await delay(300);
  const after = await evalIn(compare, `document.querySelector('[data-diff-scroll]')?.scrollTop ?? -1`);
  check('C4 下一处导航改变滚动位置', typeof before === 'number' && typeof after === 'number' && after !== before, `before=${before} after=${after}`);

  // 截图（并排视图，回到顶部）
  await evalIn(compare, `(() => { const el = document.querySelector('[data-diff-scroll]'); el.scrollTop = 0; return true; })()`);
  await delay(200);
  const shot = await compare.send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(root, 'docs', 'screenshots', 'phase-p36-compare.png'), Buffer.from(shot.data, 'base64'));

  // C5：一致文件
  await evalMain(
    mainClient,
    `window.__TAURI_INTERNALS__.invoke('open_compare_window', { request: { mode: 'diff', left: ${JSON.stringify(sameA)}, right: ${JSON.stringify(sameB)}, extra: null } })`,
  );
  const sameShown = await waitForValue(async () => {
    const n = await evalIn(compare, `document.querySelector('[data-diff-same]') !== null`);
    return n === true ? true : null;
  }, 10000);
  check('C5 一致文件提示内容一致', sameShown === true);

  // 三方合并
  await evalMain(
    mainClient,
    `window.__TAURI_INTERNALS__.invoke('open_compare_window', { request: { mode: 'merge', left: ${JSON.stringify(baseFile)}, right: ${JSON.stringify(oursFile)}, extra: ${JSON.stringify(theirsFile)} } })`,
  );
  const cardShown = await waitForValue(async () => {
    const n = await evalIn(compare, `document.querySelectorAll('[data-merge-conflict-item]').length`);
    return n === 1 ? n : null;
  }, 10000);
  check('M1 合并窗口显示 1 个冲突卡片', cardShown === 1, `cards=${cardShown}`);

  const outRows = async () =>
    evalIn(
      compare,
      `[...document.querySelectorAll('[data-merge-out-row]')].map((n) => ({ src: n.dataset.source, code: n.querySelector('.code')?.textContent ?? '' }))`,
    );
  const defaultRows = await waitForValue(async () => {
    const rows = await outRows();
    const hasOursX = rows.some((row) => row.code === 'X' && row.src === 'ours');
    const hasTheirsD = rows.some((row) => row.code === 'D' && row.src === 'theirs');
    return hasOursX && hasTheirsD ? rows : null;
  }, 10000);
  check('M2 默认采用我方且自动合并他方改动', defaultRows !== null, JSON.stringify(defaultRows ?? ''));

  // 写回与撤销
  await evalIn(compare, `(document.querySelector('[data-merge-write]')?.click(), true)`);
  const wrote = await waitForValue(async () => {
    const status = await evalIn(compare, `document.querySelector('[data-merge-write-status]')?.textContent ?? ''`);
    return status.includes('已写回') ? status : null;
  }, 8000);
  check('W1 写回成功提示（含备份路径）', wrote !== null && wrote.includes('.bak'), String(wrote));
  check('W2 写回内容按默认选择（a/X/c/D）', readFileSync(oursFile, 'utf8') === 'a\nX\nc\nD\n');
  check('W3 备份为写回前内容（a/X/c/d）', readFileSync(`${oursFile}.bak`, 'utf8') === 'a\nX\nc\nd\n');

  await evalIn(compare, `(document.querySelector('[data-merge-undo]')?.click(), true)`);
  const undone = await waitForValue(async () => {
    const status = await evalIn(compare, `document.querySelector('[data-merge-write-status]')?.textContent ?? ''`);
    return status.includes('已撤销') ? status : null;
  }, 8000);
  check('W4 撤销写回恢复原文件', undone !== null && readFileSync(oursFile, 'utf8') === 'a\nX\nc\nd\n', String(undone));

  // 冲突选择：他方 / 双方 / 不用
  await evalIn(compare, `(document.querySelector('[data-merge-choice="theirs"][data-conflict-index="0"]')?.click(), true)`);
  const switched = await waitForValue(async () => {
    const rows = await outRows();
    return rows.some((row) => row.code === 'Y' && row.src === 'theirs') ? rows : null;
  }, 6000);
  check('M3 选择他方后冲突行切换为 Y', switched !== null, JSON.stringify(switched ?? ''));

  await evalIn(compare, `(document.querySelector('[data-merge-choice="both"][data-conflict-index="0"]')?.click(), true)`);
  const both = await waitForValue(async () => {
    const rows = await outRows();
    const ours = rows.filter((row) => row.code === 'X');
    const theirs = rows.filter((row) => row.code === 'Y');
    return ours.length === 1 && theirs.length === 1 ? rows : null;
  }, 6000);
  check('M4 选择双方后 X 与 Y 同时存在', both !== null, JSON.stringify(both ?? ''));

  await evalIn(compare, `(document.querySelector('[data-merge-choice="none"][data-conflict-index="0"]')?.click(), true)`);
  const noneGone = await waitForValue(async () => {
    const gone = await evalIn(
      compare,
      `![...document.querySelectorAll('[data-merge-out-row]')].some((n) => ['X','Y'].includes(n.querySelector('.code')?.textContent ?? ''))`,
    );
    return gone === true ? true : null;
  }, 6000);
  check('M5 选择不用后 X/Y 均消失', noneGone === true);

  const shotMerge = await compare.send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(root, 'docs', 'screenshots', 'phase-p36-merge.png'), Buffer.from(shotMerge.data, 'base64'));
} finally {
  clearTimeout(watchdog);
  await killApp();
  await delay(800);
}

console.log(`\n比较/合并冒烟：${results.filter(Boolean).length}/${results.length} 通过`);
if (results.filter(Boolean).length !== results.length) {
  console.log(`工作目录保留：${work}`);
  process.exitCode = 1;
} else {
  try {
    rmSync(work, { recursive: true, force: true });
  } catch {
    console.log(`工作目录占用中，保留（${work}）`);
  }
}
