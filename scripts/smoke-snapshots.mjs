// S-Read-TXT 快照/版本历史 E2E（常驻套件）。
// 场景：
//   S1 打开样本并进入编辑（默认 zh-CN 文案、新数据目录）
//   S2 「文件 → 版本历史…」面板 + 「立即快照」→ 列表 1 条
//   S3 再次修改 + 快照 → 列表 2 条（最新在上）
//   S4 恢复较旧快照 → 内容回退 + 脏标记；Ctrl+Z 撤销恢复
//   S5 删除一条快照（确认框）→ 列表减一
//   S6 快照文件确实落盘（data/snapshots/<key>/*.snap）
//   S7 截图 docs/screenshots/p3-snapshots.png
// 依赖：debug 构建（node scripts/dev.mjs build）。
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';

const root = process.cwd();
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(tmpdir(), `srt-snapshots-${Date.now()}`);
mkdirSync(work, { recursive: true });
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
const port = 9600 + Math.floor(Math.random() * 250);

const SAMPLE = 'snp-line-1\nsnp-line-2\nsnp-line-3';
const samplePath = join(work, 'snap-sample.txt');
writeFileSync(samplePath, SAMPLE, 'utf8');

const results = [];
function check(name, ok, detail = '') {
  results.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  [${detail}]` : ''}`);
}

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const child = spawn(exe, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
    SRT_NO_ELEVATION: '1',
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: 'ignore',
});
child.unref();

let client = null;
let msgId = 0;
const pending = new Map();

async function findTarget() {
  for (let i = 0; i < 120; i += 1) {
    try {
      const res = await fetch(`http://127.0.0.1:${port}/json/list`);
      const list = await res.json();
      const page = list.find((t) => t.type === 'page' && t.url.includes('tauri.localhost'));
      if (page) return page.webSocketDebuggerUrl;
    } catch {
      // 端口未就绪
    }
    await delay(250);
  }
  throw new Error('未找到 CDP 目标');
}

async function connect(url) {
  client = new WebSocket(url);
  await new Promise((resolve, reject) => {
    client.onopen = resolve;
    client.onerror = () => reject(new Error('CDP 连接失败'));
  });
  client.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve } = pending.get(msg.id);
      pending.delete(msg.id);
      resolve(msg);
    }
  };
}

function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++msgId;
    pending.set(id, { resolve });
    client.send(JSON.stringify({ id, method, params }));
    setTimeout(() => {
      if (pending.has(id)) {
        pending.delete(id);
        reject(new Error(`CDP 超时：${method}`));
      }
    }, 15000);
  });
}

async function evalJs(expression) {
  const res = await send('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (res.result?.exceptionDetails) {
    throw new Error(`eval 异常：${JSON.stringify(res.result.exceptionDetails).slice(0, 300)}`);
  }
  return res.result?.result?.value;
}

async function waitForValue(expression, predicate, timeoutMs = 10000, label = '') {
  const start = Date.now();
  let last;
  while (Date.now() - start < timeoutMs) {
    try {
      last = await evalJs(expression);
      if (predicate(last)) return last;
    } catch {
      // 重试
    }
    await delay(200);
  }
  throw new Error(`等待超时${label ? `（${label}）` : ''}：最后值 ${JSON.stringify(last)}`);
}

async function pressKey(key, code, keyCode, modifiers = 0) {
  await send('Input.dispatchKeyEvent', {
    type: 'rawKeyDown',
    key,
    code,
    windowsVirtualKeyCode: keyCode,
    modifiers,
  });
  await send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    key,
    code,
    windowsVirtualKeyCode: keyCode,
    modifiers,
  });
}

async function insertText(text) {
  await send('Input.insertText', { text });
  await delay(150);
}

const clickByText = (text) =>
  evalJs(
    `(() => {
      const nodes = [...document.querySelectorAll('button.item')];
      const hit = nodes.find((n) => n.textContent.trim().startsWith(${JSON.stringify(text)}));
      if (hit) { hit.click(); return true; }
      return false;
    })()`,
  );

async function clickMenu(label) {
  const ok = await evalJs(
    `(() => {
      const b = [...document.querySelectorAll('.menu-bar button')].find((x) => x.textContent.trim() === ${JSON.stringify(label)});
      if (b) { b.click(); return true; }
      return false;
    })()`,
  );
  if (!ok) throw new Error(`菜单未找到：${label}`);
  await delay(250);
}

async function ensureEditing() {
  const clicked = await evalJs(
    `(() => { const b = document.querySelector('[aria-label="切换编辑模式"]'); if (b) { b.click(); return true; } return false; })()`,
  );
  if (!clicked) throw new Error('编辑按钮未找到');
  await waitForValue(`document.querySelector('.input-proxy') !== null`, (v) => v === true, 10000, '编辑层');
  await delay(200);
  await evalJs(`(() => { document.querySelector('.input-proxy')?.focus(); return true; })()`);
}

async function openHistoryPanel() {
  await clickMenu('文件');
  await clickByText('版本历史');
  await waitForValue(`!!document.querySelector('[data-snapshots-panel]')`, (v) => v === true, 8000, '快照面板');
}

async function clickSnapshotNow() {
  await waitForValue(
    `!!document.querySelector('[data-snap-now]:not([disabled])')`,
    (v) => v === true,
    10000,
    '立即快照按钮就绪',
  );
  await evalJs(`(() => { document.querySelector('[data-snap-now]')?.click(); return true; })()`);
}

async function snapshotDirStats() {
  const base = join(dataDir, 'snapshots');
  if (!existsSync(base)) return { files: 0, dirs: 0 };
  let files = 0;
  let dirs = 0;
  for (const entry of readdirSync(base, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      dirs += 1;
      files += readdirSync(join(base, entry.name)).filter((name) => name.endsWith('.snap')).length;
    }
  }
  return { files, dirs };
}

async function main() {
  const wsUrl = await findTarget();
  await connect(wsUrl);
  await waitForValue(
    `!!(window.__srt && document.querySelector('.menu-bar'))`,
    (v) => v === true,
    20000,
    '界面就绪',
  );

  // S1 打开并进入编辑
  await evalJs(
    `(() => { void window.__srt.openPath(${JSON.stringify(samplePath)}); return true; })()`,
  );
  await waitForValue(
    `document.querySelector('.row[data-row="0"] .txt')?.textContent ?? ''`,
    (v) => v.includes('snp-line-1'),
    15000,
    '打开文件',
  );
  await ensureEditing();
  await pressKey('End', 'End', 35, 2); // Ctrl+End
  await delay(200);
  await insertText('!');
  const s1 = await evalJs(
    `(async () => (await window.__TAURI_INTERNALS__.invoke('list_tabs')).tabs[0].dirty)()`,
  );
  check('S1 编辑并修改（脏）', s1 === true, String(s1));

  // S2 打开面板 + 立即快照
  await openHistoryPanel();
  await clickSnapshotNow();
  await waitForValue(
    `document.querySelectorAll('[data-snap-item]').length`,
    (v) => v >= 1,
    10000,
    '快照 1',
  );
  const count1 = await evalJs(`document.querySelectorAll('[data-snap-item]').length`);
  check('S2 立即快照入列表', count1 >= 1, String(count1));

  // S3 再修改 + 再快照
  await evalJs(`(() => { document.querySelector('[data-snap-close]')?.click(); return true; })()`);
  await delay(250);
  await evalJs(`(() => { document.querySelector('.input-proxy')?.focus(); return true; })()`);
  await insertText('?');
  await openHistoryPanel();
  await clickSnapshotNow();
  await waitForValue(
    `document.querySelectorAll('[data-snap-item]').length`,
    (v) => v >= 2,
    10000,
    '快照 2',
  );
  const count2 = await evalJs(`document.querySelectorAll('[data-snap-item]').length`);
  const names = await evalJs(
    `[...document.querySelectorAll('[data-snap-name]')].map((n) => n.getAttribute('data-snap-name'))`,
  );
  check('S3 第二次快照且最新在上', count2 >= 2 && names[0] > names[1], JSON.stringify(names));

  // S4 恢复较旧快照（第二条）→ '?' 消失；Ctrl+Z 撤销恢复
  await evalJs(
    `(() => { const items = [...document.querySelectorAll('[data-snap-item]')]; items[1]?.querySelector('[data-snap-restore]')?.click(); return true; })()`,
  );
  await waitForValue(
    `document.querySelector('.row[data-row="2"] .txt')?.textContent ?? ''`,
    (v) => !v.includes('?'),
    10000,
    '恢复旧快照',
  );
  const afterRestore = await evalJs(
    `(async () => (await window.__TAURI_INTERNALS__.invoke('list_tabs')).tabs[0].dirty)()`,
  );
  check('S4a 恢复旧快照内容回退且脏', afterRestore === true);
  await evalJs(`(() => { document.querySelector('[data-snap-close]')?.click(); return true; })()`);
  await delay(250);
  await evalJs(
    `(() => { document.querySelector('.input-proxy')?.focus(); return true; })()`,
  );
  await pressKey('z', 'KeyZ', 90, 2); // Ctrl+Z
  await waitForValue(
    `document.querySelector('.row[data-row="2"] .txt')?.textContent ?? ''`,
    (v) => v.includes('?'),
    10000,
    '撤销恢复',
  );
  check('S4b Ctrl+Z 撤销恢复', true);

  // S5 删除一条快照
  await openHistoryPanel();
  await evalJs(`(() => { document.querySelector('[data-snap-del]')?.click(); return true; })()`);
  await delay(300);
  const confirmed = await evalJs(
    `(() => { const b = [...document.querySelectorAll('[role="alertdialog"] button')].find((x) => x.textContent.trim() === '删除'); if (b) { b.click(); return true; } return false; })()`,
  );
  await waitForValue(
    `document.querySelectorAll('[data-snap-item]').length`,
    (v) => v <= count2 - 1,
    10000,
    '删除后列表',
  );
  const count3 = await evalJs(`document.querySelectorAll('[data-snap-item]').length`);
  check('S5 删除快照（确认框生效）', confirmed === true && count3 === count2 - 1, `confirmed=${confirmed} count=${count3}`);

  // S6 快照文件落盘
  const stats = await snapshotDirStats();
  check('S6 快照文件已落盘', stats.files >= 1 && stats.dirs >= 1, JSON.stringify(stats));

  // S7 截图
  const shotPath = join(root, 'docs', 'screenshots', 'p3-snapshots.png');
  const shot = await send('Page.captureScreenshot', { format: 'png' });
  if (shot.result?.data) {
    writeFileSync(shotPath, Buffer.from(shot.result.data, 'base64'));
    check('S7 截图', true, relative(root, shotPath));
  } else {
    check('S7 截图', false, 'captureScreenshot 无数据');
  }
}

const watchdog = setTimeout(() => {
  console.error('看门狗超时（240s），强制退出');
  try {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } catch {
    // 忽略
  }
  process.exit(4);
}, 240_000);

try {
  await main();
} catch (error) {
  check('运行异常', false, String(error?.message ?? error));
} finally {
  clearTimeout(watchdog);
  try {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } catch {
    // 忽略
  }
  await delay(500);
  try {
    rmSync(work, { recursive: true, force: true });
  } catch {
    // 忽略
  }
  const failed = results.filter((r) => !r.ok);
  console.log(`\n结果：${results.length - failed.length}/${results.length} 通过`);
  process.exitCode = failed.length === 0 ? 0 : 1;
}
