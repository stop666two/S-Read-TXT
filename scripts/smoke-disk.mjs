// 磁盘占用与缓存清理 E2E（P0-8 常驻套件）。
// 场景：
//   D1 get_disk_usage 分项正确（预置日志/WebView/字体/备份/用户文件）且 total = 各项之和
//   D2 设置窗口「常规」页展示磁盘卡片（分项行 + 总量行）
//   D3 清理日志（UI 按钮 + 确认框）：种子日志删除、用户文件保留、占用刷新
//   D4 清理备份：*.bak / *.corrupt-* 删除、用户文件保留
//   D5 清理 WebView 缓存：种子文件删除（被占用的 WebView 自身文件计入 skipped 不报错）
//   D6 未知清理范围：拒绝并返回 INVALID_SCOPE
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
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
const exe = argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe'));
const work = join(tmpdir(), `srt-disk-${Date.now()}`);
mkdirSync(work, { recursive: true });
const dataDir = join(work, 'data');
mkdirSync(join(dataDir, 'logs'), { recursive: true });
mkdirSync(join(dataDir, 'webview', 'Cache'), { recursive: true });
mkdirSync(join(dataDir, 'fonts'), { recursive: true });

const LOG_BYTES = 64 * 1024;
const BACKUP_BYTES = 8 * 1024;
const CORRUPT_BYTES = 4 * 1024;
writeFileSync(join(dataDir, 'logs', 'seed-old.log'), Buffer.alloc(LOG_BYTES, 0x6c));
writeFileSync(join(dataDir, 'webview', 'Cache', 'seed.bin'), Buffer.alloc(8 * 1024, 0x77));
writeFileSync(join(dataDir, 'fonts', 'seed.ttf'), Buffer.alloc(4 * 1024, 0x66));
writeFileSync(join(dataDir, 'settings.json.bak'), Buffer.alloc(BACKUP_BYTES, 0x62));
writeFileSync(join(dataDir, 'reader.json.corrupt-1'), Buffer.alloc(CORRUPT_BYTES, 0x63));
writeFileSync(join(dataDir, 'settings.json'), '{"schemaVersion":2}\n');

// 端口段避开 Windows 保留区间 10008–10107（HNS/Hyper-V 排除段；落入则 WebView2 调试端口绑定失败、CDP 永不出现）
const port = 9600 + Math.floor(Math.random() * 250);
const child = spawn(exe, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: 'ignore',
});

/** 看门狗：超时明确失败，避免挂死。 */
const watchdog = setTimeout(() => {
  console.log('FAIL  看门狗超时（240s）');
  process.exitCode = 4;
  try {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } catch {
    /* 忽略 */
  }
}, 240_000);

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

try {
  const main = await createClient(await findTarget(port));
  const evalMain = (expression) => evalIn(main, expression);
  await waitForValue(
    async () => ((await evalMain('(() => !!(window.__srt && window.__srt.openPath))()')) ? true : null),
    30000,
  );
  await dismissOnboarding(evalMain);

  /** 经主窗口调用 get_disk_usage 并解析。 */
  const usage = async () => {
    const raw = await evalMain(
      `window.__TAURI_INTERNALS__.invoke('get_disk_usage').then((r) => JSON.stringify(r)).catch((e) => 'ERR:' + JSON.stringify(e))`,
    );
    if (typeof raw !== 'string' || raw.startsWith('ERR:')) throw new Error(`get_disk_usage 失败：${raw}`);
    return JSON.parse(raw);
  };

  // ---- D1 分项统计 ----
  const first = await usage();
  const item = (key) => first.items.find((entry) => entry.key === key) ?? { bytes: 0, files: 0 };
  const sum = first.items.reduce((acc, entry) => acc + entry.bytes, 0);
  const d1 =
    item('logs').bytes >= LOG_BYTES &&
    item('backups').bytes >= BACKUP_BYTES + CORRUPT_BYTES &&
    item('webview').bytes >= 8 * 1024 &&
    item('fonts').bytes >= 4 * 1024 &&
    first.totalBytes === sum;
  if (d1) ok('D1 占用分项与总量正确', `total=${first.totalBytes}`);
  else bad('D1 占用分项', JSON.stringify(first));

  // ---- D2 设置窗口磁盘卡片 ----
  await evalMain(`window.__TAURI_INTERNALS__.invoke('open_settings', {}).then(() => true).catch(() => false)`);
  const settingsTarget = await waitForValue(async () => await findTarget(port, 'settings.html'), 15000);
  const settingsClient = await createClient(settingsTarget);
  const evalSet = (expression) => evalIn(settingsClient, expression);
  await waitForValue(
    async () =>
      (await evalSet(`(() => !!document.querySelector('[data-disk-item="logs"]'))()`)) ? true : null,
    15000,
  );
  const d2 = await evalSet(
    `(() => { const logs = document.querySelector('[data-disk-item="logs"]');
      const total = document.querySelector('[data-disk-total]');
      const header = [...document.querySelectorAll('.section-title')].some((el) => el.textContent.includes('磁盘占用'));
      return logs !== null && total !== null && header; })()`,
  );
  if (d2) ok('D2 设置窗口展示磁盘卡片');
  else bad('D2 磁盘卡片缺失');

  // ---- D3 清理日志（UI + 确认框）----
  await evalSet(
    `(() => { const btn = document.querySelector('[data-setting="clearLogs"]'); if (btn) btn.click(); return true; })()`,
  );
  await waitForValue(
    async () =>
      (await evalSet(
        `(() => [...document.querySelectorAll('button')].some((b) => b.textContent.trim() === '清理' && b.closest('[role="alertdialog"], [role="dialog"]')))()`,
      ))
        ? true
        : null,
    8000,
  );
  await evalSet(
    `(() => { const btn = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === '清理' && b.closest('[role="alertdialog"], [role="dialog"]')); if (btn) btn.click(); return true; })()`,
  );
  await waitForValue(
    async () =>
      (await evalSet(
        `(() => [...document.querySelectorAll('.toast, [data-toast]')].some((el) => el.textContent.includes('已释放')))()`,
      ))
        ? true
        : null,
    8000,
  );
  await delay(400);
  const afterLogs = await usage();
  const logsAfter = afterLogs.items.find((entry) => entry.key === 'logs')?.bytes ?? -1;
  const d3 =
    !existsSync(join(dataDir, 'logs', 'seed-old.log')) &&
    existsSync(join(dataDir, 'settings.json')) &&
    logsAfter < LOG_BYTES;
  if (d3) ok('D3 清理日志（UI+确认）', `logs=${logsAfter}`);
  else bad('D3 清理日志', `seed=${existsSync(join(dataDir, 'logs', 'seed-old.log'))} logs=${logsAfter}`);

  // ---- D4 清理备份 ----
  await evalSet(
    `(() => { const btn = document.querySelector('[data-setting="clearBackups"]'); if (btn) btn.click(); return true; })()`,
  );
  await waitForValue(
    async () =>
      (await evalSet(
        `(() => [...document.querySelectorAll('button')].some((b) => b.textContent.trim() === '清理' && b.closest('[role="alertdialog"], [role="dialog"]')))()`,
      ))
        ? true
        : null,
    8000,
  );
  await evalSet(
    `(() => { const btn = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === '清理' && b.closest('[role="alertdialog"], [role="dialog"]')); if (btn) btn.click(); return true; })()`,
  );
  await delay(800);
  const d4 =
    !existsSync(join(dataDir, 'settings.json.bak')) &&
    !existsSync(join(dataDir, 'reader.json.corrupt-1')) &&
    existsSync(join(dataDir, 'settings.json'));
  if (d4) ok('D4 清理备份文件');
  else bad('D4 清理备份', `bak=${existsSync(join(dataDir, 'settings.json.bak'))}`);

  // ---- D5 清理 WebView 缓存（种子文件可删；自身缓存被占用会计入 skipped）----
  await evalSet(
    `(() => { const btn = document.querySelector('[data-setting="clearWebview"]'); if (btn) btn.click(); return true; })()`,
  );
  await waitForValue(
    async () =>
      (await evalSet(
        `(() => [...document.querySelectorAll('button')].some((b) => b.textContent.trim() === '清理' && b.closest('[role="alertdialog"], [role="dialog"]')))()`,
      ))
        ? true
        : null,
    8000,
  );
  await evalSet(
    `(() => { const btn = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === '清理' && b.closest('[role="alertdialog"], [role="dialog"]')); if (btn) btn.click(); return true; })()`,
  );
  await delay(1000);
  const d5 = !existsSync(join(dataDir, 'webview', 'Cache', 'seed.bin'));
  if (d5) ok('D5 清理 WebView 缓存（种子已删）');
  else bad('D5 清理 WebView 缓存未生效');

  // ---- D6 未知清理范围拒绝 ----
  const raw = await evalMain(
    `window.__TAURI_INTERNALS__.invoke('clear_cache', { scope: 'nope' }).then(() => 'OK').catch((e) => 'ERR:' + JSON.stringify(e))`,
  );
  const d6 = typeof raw === 'string' && raw.includes('INVALID_SCOPE');
  if (d6) ok('D6 未知范围拒绝（INVALID_SCOPE）');
  else bad('D6 未知范围未拒绝', String(raw));
} finally {
  clearTimeout(watchdog);
  try {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } catch {
    /* 忽略 */
  }
  await delay(600);
  for (let attempt = 0; attempt < 12; attempt += 1) {
    try {
      rmSync(work, { recursive: true, force: true });
      break;
    } catch {
      await delay(250);
    }
  }
}

console.log(`\n磁盘占用套件：通过 ${passed}/${passed + failed}`);
if (failed > 0) process.exitCode = 1;
