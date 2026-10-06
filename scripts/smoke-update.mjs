// 更新检查端到端冒烟（P4）：本地桩服务器 + 设置 UI 配置更新源 + 关于页全流程。
// 覆盖：离线确认取消/继续、发现新版本、下载与摘要校验、摘要不符、缺更新源、源不可达。
//
// 依赖：已构建的 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 数据隔离：SRT_DATA_DIR 指向项目 tmp 工作目录；通过后清理，失败保留现场。

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget } from './lib/smoke-cdp.mjs';
import { removeWithRetryAsync } from './lib/system.mjs';
import { startStubUpdateServer } from './lib/stub-update-server.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-update-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
writeFileSync(join(dataDir, 'settings.json'), JSON.stringify({ schemaVersion: 17 }), 'utf8');
const port = Number(argValue('--port', String(9060 + Math.floor(Math.random() * 50))));

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

const waitFor = async (client, expression, predicate, timeoutMs = 12000) => {
  const deadline = Date.now() + timeoutMs;
  let last;
  for (;;) {
    last = await evalIn(client, expression);
    if (predicate(last)) return last;
    if (Date.now() > deadline) return last;
    await delay(150);
  }
};

const setInput = async (client, selector, value) => {
  await evalIn(
    client,
    `(() => {
      const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return false;
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
      setter.call(el, ${JSON.stringify(value)});
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      el.blur();
      return true;
    })()`,
  );
};

const statusIs = (client, expected, timeoutMs = 12000) =>
  waitFor(
    client,
    `document.querySelector('[data-update-status]')?.getAttribute('data-update-status') ?? ''`,
    (v) => v === expected,
    timeoutMs,
  );

const switchTab = async (client, name) => {
  await evalIn(
    client,
    `(() => { [...document.querySelectorAll('.tabs [role="tab"]')].find((b) => b.textContent.trim() === ${JSON.stringify(name)}).click(); return true; })()`,
  );
  await delay(150);
};

/** 切到「系统」页设置更新源，再切回「关于」页等待输入就绪。 */
const setSource = async (client, value) => {
  await switchTab(client, '系统');
  await waitFor(client, `!!document.querySelector('[data-setting="app.update.sourceUrl"]')`, (v) => v === true);
  await setInput(client, '[data-setting="app.update.sourceUrl"]', value);
  await delay(350);
  await switchTab(client, '关于');
  await waitFor(client, `!!document.querySelector('[data-update-check]')`, (v) => v === true);
};

/** 无网络模式下点击检查：等确认弹窗并按需确认/取消。 */
const clickCheck = async (client, { confirm }) => {
  await evalIn(client, `(() => { document.querySelector('[data-update-check]').click(); return true; })()`);
  const dialog = await waitFor(client, `!!document.querySelector('[role="alertdialog"] .btn.danger')`, (v) => v === true, 4000);
  if (!dialog) return false;
  if (confirm) {
    await evalIn(client, `(() => { document.querySelector('[role="alertdialog"] .btn.danger').click(); return true; })()`);
  } else {
    await evalIn(client, `(() => { document.querySelector('[role="alertdialog"] .btn').click(); return true; })()`);
  }
  return true;
};

let stub = null;
try {
  stub = await startStubUpdateServer();
  const stubUrl = `http://127.0.0.1:${stub.port}/latest.json`;
  const badUrl = `http://127.0.0.1:${stub.port}/latest-bad.json`;

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

  await waitFor(main, `!!document.querySelector('button[title="设置"]')`, (v) => v === true);
  await evalIn(main, `(() => { document.querySelector('button[title="设置"]').click(); return true; })()`);
  const settingsWs = await findTarget(port, 'settings.html');
  const settings = await createClient(settingsWs);
  await settings.send('Page.enable');
  await switchTab(settings, '系统');
  const rowReady = await waitFor(settings, `!!document.querySelector('[data-setting="app.update.sourceUrl"]')`, (v) => v === true);
  check('U1 设置页存在更新源输入', rowReady === true);
  await setInput(settings, '[data-setting="app.update.sourceUrl"]', stubUrl);
  await delay(400);

  await switchTab(settings, '关于');
  const aboutReady = await waitFor(settings, `!!document.querySelector('[data-update-check]')`, (v) => v === true);
  check('U2 关于页显示更新检查按钮', aboutReady === true);

  const requestsBefore = stub.requestCount();
  const dialogShown = await clickCheck(settings, { confirm: false });
  await delay(500);
  const idleAfterCancel = await evalIn(
    settings,
    `document.querySelector('[data-update-status]')?.getAttribute('data-update-status') ?? ''`,
  );
  check(
    'U3 无网络模式先确认且取消不联网',
    dialogShown === true && idleAfterCancel === 'idle' && stub.requestCount() === requestsBefore,
    `status=${idleAfterCancel} requests=${stub.requestCount() - requestsBefore}`,
  );

  await clickCheck(settings, { confirm: true });
  const available = await statusIs(settings, 'available');
  const availableText = await evalIn(settings, `document.querySelector('[data-update-status]')?.textContent ?? ''`);
  check(
    'U4 发现新版本并展示版本号',
    available === 'available' && availableText.includes('0.0.2-beta'),
    `text=${availableText.trim()}`,
  );

  await evalIn(settings, `(() => { document.querySelector('[data-update-download]').click(); return true; })()`);
  const downloaded = await statusIs(settings, 'downloaded', 30000);
  const updatesDir = join(dataDir, 'updates');
  let fileOk = false;
  let fileInfo = 'missing';
  if (existsSync(updatesDir)) {
    const files = readdirSync(updatesDir);
    const target = files.find((name) => name.endsWith('.exe'));
    if (target) {
      const bytes = readFileSync(join(updatesDir, target));
      const hash = createHash('sha256').update(bytes).digest('hex');
      fileOk = bytes.length === stub.payloadLength && hash === stub.sha256;
      fileInfo = `${target} ${bytes.length}B sha=${hash.slice(0, 12)}…`;
    }
  }
  const revealReady = await evalIn(settings, `!!document.querySelector('[data-update-reveal]')`);
  check('U5 下载成功且文件摘要一致', downloaded === 'downloaded' && fileOk && revealReady === true, fileInfo);

  await settings.send('Emulation.setDeviceMetricsOverride', {
    width: 980,
    height: 720,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await delay(250);
  const shot = await settings.send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(root, 'docs', 'screenshots', 'phase-p4-update.png'), Buffer.from(shot.data, 'base64'));

  await setSource(settings, badUrl);
  await clickCheck(settings, { confirm: true });
  await statusIs(settings, 'available');
  await evalIn(settings, `(() => { document.querySelector('[data-update-download]').click(); return true; })()`);
  const digestErr = await statusIs(settings, 'error', 30000);
  const digestText = await evalIn(settings, `document.querySelector('[data-update-error]')?.textContent ?? ''`);
  const filesAfterBad = existsSync(updatesDir) ? readdirSync(updatesDir) : [];
  check(
    'U6 摘要不符时报错且不落盘',
    digestErr === 'error' && digestText.includes('校验') && filesAfterBad.filter((n) => n.endsWith('.exe')).length === 1,
    `text=${digestText.trim()}`,
  );

  await setSource(settings, '');
  const requestsBeforeEmpty = stub.requestCount();
  await clickCheck(settings, { confirm: true });
  const sourceErr = await statusIs(settings, 'error');
  const sourceText = await evalIn(settings, `document.querySelector('[data-update-error]')?.textContent ?? ''`);
  check(
    'U7 未配置更新源时给出本地提示',
    sourceErr === 'error' && sourceText.includes('更新源') && stub.requestCount() === requestsBeforeEmpty,
    `text=${sourceText.trim()}`,
  );

  await setSource(settings, 'http://127.0.0.1:9/latest.json');
  await clickCheck(settings, { confirm: true });
  const failed = await statusIs(settings, 'error', 20000);
  const failedText = await evalIn(settings, `document.querySelector('[data-update-error]')?.textContent ?? ''`);
  check('U8 源不可达时给出失败提示', failed === 'error' && failedText.includes('失败'), `text=${failedText.trim()}`);

  const failedCount = results.filter((r) => !r.ok).length;
  console.log(`结果：${results.length - failedCount}/${results.length} 通过`);
  process.exitCode = failedCount === 0 ? 0 : 1;
} catch (error) {
  console.error(`套件异常：${error?.stack ?? error}`);
  process.exitCode = 1;
} finally {
  clearTimeout(watchdog);
  await killApp();
  if (stub) await stub.close();
  await delay(300);
  if (process.exitCode === 0) {
    await removeWithRetryAsync(work);
  }
}
