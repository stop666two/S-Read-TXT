// 隐私数据端到端冒烟（P4）：用量统计、部分勾选清除（历史/快照/批注/剪贴板）、
// 未勾选会话保留、二次确认与清除后用量刷新。
//
// 依赖：已构建的 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 数据隔离：SRT_DATA_DIR 指向项目 tmp 工作目录；通过后清理，失败保留现场。

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget } from './lib/smoke-cdp.mjs';
import { removeWithRetryAsync } from './lib/system.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-privacy-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
mkdirSync(join(dataDir, 'logs'), { recursive: true });
mkdirSync(join(dataDir, 'snapshots', 'ab'), { recursive: true });
mkdirSync(join(dataDir, 'annotations'), { recursive: true });
writeFileSync(join(dataDir, 'settings.json'), JSON.stringify({ schemaVersion: 17 }), 'utf8');
writeFileSync(join(dataDir, 'history.jsonl'), '{"a":1}\n{"b":2}\n{"c":3}\n', 'utf8');
writeFileSync(
  join(dataDir, 'session.json'),
  JSON.stringify({
    schemaVersion: 4,
    focusedLabel: 'main',
    windows: [
      {
        label: 'main',
        window: { width: 1100, height: 760, maximized: false },
        panes: [{ pane: 'main#1', activeTabIndex: 0, tabs: [] }],
        layout: { type: 'leaf', pane: 'main#1' },
      },
    ],
  }),
  'utf8',
);
writeFileSync(join(dataDir, 'snapshots', 'ab', '1.snap'), 'x', 'utf8');
writeFileSync(join(dataDir, 'annotations', 'h.json'), '{}', 'utf8');
writeFileSync(
  join(dataDir, 'clipboard-history.json'),
  JSON.stringify({ schemaVersion: 1, entries: [{ text: 'a', at: 't' }] }),
  'utf8',
);
writeFileSync(join(dataDir, 'logs', 'app.log.1'), 'old', 'utf8');
const port = Number(argValue('--port', String(9020 + Math.floor(Math.random() * 50))));

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
  const result = await client.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(`eval 异常: ${result.exceptionDetails.text}`);
  return result.result.value;
};

const waitFor = async (client, expression, predicate, timeoutMs = 8000) => {
  const deadline = Date.now() + timeoutMs;
  let last;
  for (;;) {
    last = await evalIn(client, expression);
    if (predicate(last)) return last;
    if (Date.now() > deadline) return last;
    await delay(150);
  }
};

try {
  let mainWs = null;
  for (let attempt = 1; attempt <= 3 && !mainWs; attempt += 1) {
    try {
      mainWs = await findTarget(port, 'tauri.localhost');
    } catch (error) {
      if (attempt === 3) throw error;
      await delay(2500);
    }
  }
  const main = await createClient(mainWs);
  await main.send('Page.enable');
  await dismissOnboarding((expression) => evalIn(main, expression));

  const usage = await evalIn(main, `window.__TAURI_INTERNALS__.invoke('privacy_usage')`);
  check(
    'P1 用量统计与预置数据一致',
    usage.history >= 3 && usage.session === 1 && usage.snapshots === 1 && usage.annotations === 1 && usage.clipboard === 1 && usage.logs >= 1,
    JSON.stringify(usage),
  );

  await waitFor(main, `!!document.querySelector('button[title="设置"]')`, (v) => v === true);
  await evalIn(main, `(() => { document.querySelector('button[title="设置"]').click(); return true; })()`);
  const settingsWs = await findTarget(port, 'settings.html');
  const settings = await createClient(settingsWs);
  await settings.send('Page.enable');
  await waitFor(
    settings,
    `[...document.querySelectorAll('.tabs [role="tab"]')].some((b) => b.textContent.trim() === '系统')`,
    (v) => v === true,
  );
  await evalIn(
    settings,
    `(() => { [...document.querySelectorAll('.tabs [role="tab"]')].find((b) => b.textContent.trim() === '系统').click(); return true; })()`,
  );

  const sectionReady = await waitFor(settings, `!!document.querySelector('[data-setting="clearPrivacy"]')`, (v) => v === true);
  check('P2 设置窗口显示隐私区块', sectionReady === true);
  const countHistory = await waitFor(
    settings,
    `document.querySelector('[data-privacy-count="history"]')?.textContent?.trim() ?? ''`,
    (v) => v !== '' && v !== '正在载入配置…',
  );
  check('P3 隐私计数回显（历史≥3）', Number(countHistory) >= 3, `count=${countHistory}`);

  await evalIn(
    settings,
    `(() => { const el = document.querySelector('[data-privacy-check="session"]'); if (el.checked) el.click(); return true; })()`,
  );
  await evalIn(settings, `(() => { document.querySelector('[data-setting="clearPrivacy"]').click(); return true; })()`);
  const confirmReady = await waitFor(settings, `!!document.querySelector('[role="alertdialog"] .btn.danger')`, (v) => v === true);
  check('P4 清除前弹出二次确认', confirmReady === true);
  await evalIn(settings, `(() => { document.querySelector('[role="alertdialog"] .btn.danger').click(); return true; })()`);
  await delay(600);

  check(
    'P5 历史文件已清空',
    existsSync(join(dataDir, 'history.jsonl')) && readFileSync(join(dataDir, 'history.jsonl'), 'utf8').trim() === '',
  );
  check('P6 快照目录已清空', !existsSync(join(dataDir, 'snapshots', 'ab', '1.snap')));
  check('P7 批注文件已清空', !existsSync(join(dataDir, 'annotations', 'h.json')));
  check(
    'P8 剪贴板已清空',
    (() => {
      const raw = readFileSync(join(dataDir, 'clipboard-history.json'), 'utf8');
      return JSON.parse(raw).entries.length === 0;
    })(),
  );
  check('P9 未勾选的会话文件保留', existsSync(join(dataDir, 'session.json')));

  const usageAfter = await evalIn(main, `window.__TAURI_INTERNALS__.invoke('privacy_usage')`);
  check(
    'P10 清除后用量刷新一致',
    usageAfter.history === 0 &&
      usageAfter.session === 1 &&
      usageAfter.snapshots === 0 &&
      usageAfter.annotations === 0 &&
      usageAfter.clipboard === 0,
    JSON.stringify(usageAfter),
  );

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
    await removeWithRetryAsync(work);
  }
}
