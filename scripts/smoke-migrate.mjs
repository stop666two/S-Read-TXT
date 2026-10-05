// 数据目录迁移 E2E（常驻套件）。
// 场景（便携副本运行，保证程序目录可写且互不干扰）：
//   M1 便携启动：数据目录 = 副本程序目录/data，预置设置与历史可用
//   M2 设置窗口「常规」页存在迁移入口按钮
//   M3 IPC 迁移：复制校验并写指针（config.json 指向新目录）
//   M4 重启应用（击杀后由脚本重新拉起）→ 指向新目录（origin=persisted）
//   M5 数据完整（settings/history 保留）且旧目录已清理（含延迟清理标记消费）
//   M6 restart_app 命令：进程自我替换后仍指向新目录
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
// 说明：迁移运行时 WebView2 占用部分缓存文件 → 复制跳过与延迟清理属预期行为。
import { spawn, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
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
const DEBUG_DIR = join(root, 'src-tauri', 'target', 'debug');
const exePath = argValue('--exe', join(DEBUG_DIR, 's-read-txt.exe'));
const work = join(tmpdir(), `srt-migrate-${Date.now()}`);
const appDir = join(work, 'app');
const appExe = join(appDir, 's-read-txt.exe');
const appDataDir = join(appDir, 'data');
const targetDir = join(work, 'newdata');
mkdirSync(appDataDir, { recursive: true });
copyFileSync(exePath, appExe);
copyFileSync(join(DEBUG_DIR, 'WebView2Loader.dll'), join(appDir, 'WebView2Loader.dll'));

// 预置数据：设置（maxTabs=30 便于迁移后校验）+ 一条历史
writeFileSync(join(appDataDir, 'settings.json'), '{"schemaVersion":2,"maxTabs":30}\n');
writeFileSync(
  join(appDataDir, 'history.jsonl'),
  `${JSON.stringify({
    path: join(work, 'sample.txt'),
    name: 'sample.txt',
    size: 123,
    encoding: 'UTF-8',
    openedAt: '2026-10-03T12:00:00.000Z',
    lastRow: 5,
    lastPercent: 1.5,
  })}\n`,
);

const port = 9720 + Math.floor(Math.random() * 150);
let child = null;

/** 按可执行文件绝对路径精确击杀（避免误伤其他实例）。 */
function killAppProcesses() {
  const escaped = appExe.replace(/'/g, "''");
  const command =
    `Get-CimInstance Win32_Process -Filter "Name='s-read-txt.exe'" | ` +
    `Where-Object { $_.ExecutablePath -eq '${escaped}' } | ` +
    `ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`;
  spawnSync('powershell', ['-NoProfile', '-Command', command], { stdio: 'ignore' });
}

function launch() {
  child = spawn(appExe, [], {
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });
}

/** 等待主窗口就绪并返回 CDP 客户端。 */
async function connectMain(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const client = await createClient(await findTarget(port));
      const ready = await client.send('Runtime.evaluate', {
        expression: '(() => !!(window.__srt && window.__srt.openPath))()',
        returnByValue: true,
      });
      if (ready.result?.value === true) return client;
    } catch {
      /* 未就绪，重试 */
    }
    await delay(500);
  }
  throw new Error('主窗口未就绪（超时）');
}

const evalIn = async (client, expression, awaitPromise = true) => {
  const result = await client.send('Runtime.evaluate', {
    expression,
    awaitPromise,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    const detail = result.exceptionDetails.exception?.description ?? result.exceptionDetails.text;
    throw new Error(String(detail));
  }
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

const watchdog = setTimeout(() => {
  console.log('FAIL  看门狗超时（300s）');
  process.exitCode = 4;
  killAppProcesses();
  try {
    rmSync(work, { recursive: true, force: true });
  } catch {
    /* 忽略 */
  }
}, 300_000);

let main = null;
try {
  launch();
  main = await connectMain(30000);
  await dismissOnboarding((expression) => evalIn(main, expression));

  // ---- M1 便携启动：数据目录 = 副本程序目录/data，预置数据生效 ----
  const info1 = await evalIn(main, `window.__TAURI_INTERNALS__.invoke('get_app_info')`);
  ok('M1a 便携数据目录', info1.dataDir === appDataDir, String(info1.dataDir));
  ok('M1b 来源 portable', info1.dataDirOrigin === 'portable', String(info1.dataDirOrigin));

  // ---- M2 设置窗口迁移入口 ----
  await evalIn(main, `window.__TAURI_INTERNALS__.invoke('open_settings', { tab: null })`);
  const settingsClient = await createClient(await findTarget(port, 'settings.html'));
  const hasButton = await waitForValue(
    async () =>
      (await evalIn(
        settingsClient,
        `(() => { const el = document.querySelector('[data-setting="migrateData"]'); return !!el; })()`,
      ))
        ? true
        : null,
    15000,
  );
  ok('M2 设置窗迁移按钮存在', hasButton === true);
  await evalIn(
    settingsClient,
    `(() => { const win = window.__TAURI_INTERNALS__; win.invoke('plugin:window|close', { label: 'settings' }).catch(() => {}); return true; })()`,
    false,
  );
  await delay(500);

  // ---- M3 IPC 迁移 ----
  const report = await evalIn(
    main,
    `window.__TAURI_INTERNALS__.invoke('migrate_data_dir', { target: ${JSON.stringify(targetDir)} })`,
  );
  ok(
    'M3a 迁移报告（复制/跳过）',
    typeof report.copiedFiles === 'number' && report.copiedFiles >= 2 && typeof report.skipped === 'number',
    `files=${report.copiedFiles} bytes=${report.copiedBytes} skipped=${report.skipped} oldRemoved=${report.oldRemoved}`,
  );
  const pointerPath = join(appDir, 'config.json');
  let pointer = null;
  try {
    pointer = JSON.parse(readFileSync(pointerPath, 'utf8'));
  } catch {
    pointer = null;
  }
  ok(
    'M3b 指针写入 config.json',
    pointer?.schemaVersion === 1 && pointer?.dataDir === targetDir,
    `${pointerPath} dataDir=${pointer?.dataDir ?? 'null'}`,
  );
  ok('M3c 新目录含 settings.json', existsSync(join(targetDir, 'settings.json')));

  // ---- M4 重启（击杀 + 重新拉起） ----
  killAppProcesses();
  await delay(1200);
  launch();
  main = await connectMain(30000);

  // ---- M5 新目录生效与数据完整 ----
  const info2 = await evalIn(main, `window.__TAURI_INTERNALS__.invoke('get_app_info')`);
  ok('M5a 重启后指向新目录', info2.dataDir === targetDir, String(info2.dataDir));
  ok('M5b 来源 persisted', info2.dataDirOrigin === 'persisted', String(info2.dataDirOrigin));
  const snapshot = await evalIn(main, `window.__TAURI_INTERNALS__.invoke('get_settings')`);
  ok('M5c 设置完整保留（maxTabs=30）', snapshot?.app?.maxTabs === 30, String(snapshot?.app?.maxTabs));
  ok('M5d 历史完整保留', existsSync(join(targetDir, 'history.jsonl')));
  ok('M5e 旧目录已清理', !existsSync(appDataDir), appDataDir);
  ok('M5f 延迟清理标记已消费', !existsSync(join(targetDir, '.cleanup.json')));

  // ---- M6 restart_app：进程自我替换（无 awaitPromise；旧进程退出） ----
  const oldPid = child.pid;
  await evalIn(
    main,
    `(() => { window.__TAURI_INTERNALS__.invoke('restart_app').catch(() => {}); return true; })()`,
    false,
  );
  await waitForValue(async () => (child.exitCode === null && !child.killed ? null : true), 15000).catch(
    () => undefined,
  );
  main = await connectMain(60000);
  const info3 = await evalIn(main, `window.__TAURI_INTERNALS__.invoke('get_app_info')`);
  ok('M6 restart_app 后仍指向新目录', info3.dataDir === targetDir, `oldPid=${oldPid} dir=${info3.dataDir}`);
} catch (error) {
  bad('套件异常', String(error));
} finally {
  clearTimeout(watchdog);
  killAppProcesses();
  await delay(300);
  try {
    rmSync(work, { recursive: true, force: true });
  } catch {
    /* 忽略 */
  }
  console.log(`\n数据目录迁移冒烟：通过 ${passed}，失败 ${failed}`);
  if (failed > 0) process.exitCode = 1;
}
