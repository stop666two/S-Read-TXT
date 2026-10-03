// 背景图 E2E（P0-6 常驻套件）。
// 覆盖：
//   B1 设置背景图文件（复制入库 + 单文件驻留）
//   B2 启用后主窗口出现背景层（data URL + 默认不透明度）
//   B3 参数调整（不透明度/模糊/亮度/平铺）+ 拉伸填充
//   B4 截图（人工目检）
//   B5 重启持久化
//   B6 文件缺失时优雅降级（无背景层，应用存活）
//   B7 幂等清理 + 非法文件名拒绝
//   B8 非法格式导入拒绝
//   B9 关闭开关后背景层消失
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
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
const work = join(tmpdir(), `srt-bg-${Date.now()}`);
mkdirSync(work, { recursive: true });
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
const port = 9700 + Math.floor(Math.random() * 200);
const shotsDir = join(root, 'docs', 'screenshots');

// 1×1 PNG（真实图片字节；用于扩展名与 MIME 校验通过）
const PNG_BASE64 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';
const seedPng = join(work, 'bg-seed.png');
writeFileSync(seedPng, Buffer.from(PNG_BASE64, 'base64'));
const badTxt = join(work, 'bg-seed.txt');
writeFileSync(badTxt, 'not an image', 'utf8');

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
const check = (name, condition, extra = '') => (condition ? ok(name, extra) : bad(name, extra));

let currentStep = '启动';
const watchdog = setTimeout(() => {
  console.error(`背景图套件看门狗超时（步骤：${currentStep}），强制退出`);
  process.exit(3);
}, 300_000);

let child = null;
let client = null;

const evalIn = async (target, expression) => {
  const result = await target.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result?.value;
};
const evalJs = (expression) => evalIn(client, expression);

async function startApp() {
  child = spawn(exe, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });
  const target = await findTarget(port, 'tauri.localhost');
  client = await createClient(target);
  await waitForValue(async () => {
    const ready = await evalJs(`typeof window.__srt?.openPath`);
    return ready === 'function' ? true : null;
  }, 20_000);
  await dismissOnboarding(evalJs);
}

async function stopApp() {
  if (child) {
    try {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    } catch {
      // 忽略
    }
    child = null;
    await delay(700);
  }
}

/** 捕获式 invoke（返回 {ok, value|error}，不抛异常） */
const invokeCaught = (command, args = {}) =>
  evalJs(
    `window.__TAURI_INTERNALS__.invoke('${command}', ${JSON.stringify(args)})
      .then((value) => ({ ok: true, value }))
      .catch((error) => ({ ok: false, error: JSON.stringify(error) }))`,
  );

/** 读取设置快照（JSON 字符串） */
const getSettings = () =>
  evalJs(
    `window.__TAURI_INTERNALS__.invoke('get_settings').then((value) => JSON.stringify(value))`,
  );

/** 以 reader 补丁保存设置（app/shortcuts 原样回传） */
async function saveReaderPatch(readerPatch) {
  const snapshot = JSON.parse(await getSettings());
  const reader = { ...snapshot.reader, ...readerPatch };
  return invokeCaught('save_settings', {
    request: { app: snapshot.app, reader, shortcuts: snapshot.shortcuts.bindings },
  });
}

/** 背景层内联样式（不存在时 null） */
const layerStyle = () =>
  evalJs(`document.querySelector('[data-bg-layer]')?.getAttribute('style') ?? null`);

/** 压缩空白的背景层样式（浏览器会把内联样式规范化序列化，需忽略空格差异） */
const compactLayerStyle = async () => {
  const style = await layerStyle();
  return style ? style.replace(/\s+/g, '') : null;
};

try {
  await startApp();

  // ---- B1 设置背景图文件 ----
  currentStep = 'B1 设置文件';
  const imported = await invokeCaught('set_background_file', { path: seedPng });
  const storedName = imported.ok ? imported.value.fileName : null;
  const storedExists = storedName ? existsSync(join(dataDir, 'backgrounds', storedName)) : false;
  check(
    'B1 设置背景图（复制入库）',
    imported.ok === true && storedExists,
    `file=${storedName} std=${imported.ok} exists=${storedExists}`,
  );

  // ---- B2 启用并渲染背景层 ----
  currentStep = 'B2 启用渲染';
  const enabled = await saveReaderPatch({
    background: { enabled: true, file: storedName, opacity: 40, fill: 'cover', blur: 0, dim: 0 },
  });
  const styleAt40 = await waitForValue(async () => {
    const compact = await compactLayerStyle();
    return compact && compact.includes('data:image/png') ? compact : null;
  }, 8000);
  check(
    'B2 背景层出现（data URL + 默认不透明度）',
    enabled.ok === true && styleAt40 !== null && styleAt40.includes('opacity:0.4'),
    String(styleAt40).slice(0, 90),
  );

  // ---- B3 参数调整 ----
  currentStep = 'B3 参数调整';
  await saveReaderPatch({
    background: { enabled: true, file: storedName, opacity: 75, fill: 'tile', blur: 8, dim: -20 },
  });
  const tuned = await waitForValue(async () => {
    const style = await compactLayerStyle();
    if (!style) return null;
    const hit =
      style.includes('opacity:0.75') &&
      style.includes('blur(8px)') &&
      style.includes('brightness(0.8)') &&
      style.includes('background-repeat:repeat');
    return hit ? style : null;
  }, 8000);
  check('B3 参数调整（不透明度/模糊/亮度/平铺）', tuned !== null, String(tuned).slice(0, 110));

  await saveReaderPatch({
    background: { enabled: true, file: storedName, opacity: 75, fill: 'stretch', blur: 8, dim: -20 },
  });
  const stretched = await waitForValue(async () => {
    const style = await compactLayerStyle();
    return style && style.includes('background-size:100%100%') ? style : null;
  }, 6000);
  check('B3b 拉伸填充（background-size 100% 100%）', stretched !== null);

  // ---- B4 截图（人工目检） ----
  currentStep = 'B4 截图';
  mkdirSync(shotsDir, { recursive: true });
  await delay(300);
  const shot = await client.send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(shotsDir, 'phase6-bg.png'), Buffer.from(shot.data, 'base64'));
  check('B4 背景图截图已保存', true, 'phase6-bg.png');

  // ---- B5 重启持久化 ----
  currentStep = 'B5 重启持久化';
  await stopApp();
  await startApp();
  const persisted = await waitForValue(async () => {
    const style = await layerStyle();
    return style && style.includes('data:image/png') ? style : null;
  }, 10_000);
  check('B5 重启后背景层保持', persisted !== null, String(persisted).slice(0, 90));

  // ---- B6 文件缺失优雅降级 ----
  currentStep = 'B6 缺失降级';
  rmSync(join(dataDir, 'backgrounds', storedName), { force: true });
  await stopApp();
  await startApp();
  await delay(1200);
  const degraded = await layerStyle();
  const stillAlive = await evalJs(`typeof window.__srt?.openPath`);
  check(
    'B6 背景文件缺失 → 无背景层但应用存活',
    degraded === null && stillAlive === 'function',
    `layer=${degraded}`,
  );

  // ---- B7 幂等清理 + 非法名拒绝 ----
  currentStep = 'B7 清理与非法名';
  const clearGone = await invokeCaught('clear_background_file', { fileName: storedName });
  const clearBad = await invokeCaught('clear_background_file', { fileName: '../escape.png' });
  check(
    'B7 幂等清理 + 非法文件名拒绝',
    clearGone.ok === true &&
      clearBad.ok === false &&
      String(clearBad.error).includes('BACKGROUND_INVALID'),
    `gone=${clearGone.ok} bad=${String(clearBad.error).slice(0, 60)}`,
  );

  // ---- B8 非法格式导入拒绝 ----
  currentStep = 'B8 非法格式';
  const badImport = await invokeCaught('set_background_file', { path: badTxt });
  check(
    'B8 非法格式（.txt）导入拒绝',
    badImport.ok === false && String(badImport.error).includes('BACKGROUND_INVALID'),
    String(badImport.error).slice(0, 80),
  );

  // ---- B9 关闭开关 ----
  currentStep = 'B9 关闭开关';
  await saveReaderPatch({
    background: { enabled: false, file: storedName, opacity: 75, fill: 'stretch', blur: 8, dim: -20 },
  });
  const hidden = await waitForValue(async () => {
    const exists = await evalJs(`Boolean(document.querySelector('[data-bg-layer]'))`);
    return exists ? null : true;
  }, 6000);
  check('B9 关闭开关后背景层消失', hidden === true);

  console.log(`\n背景图套件：通过 ${passed}/${passed + failed}`);
  if (failed > 0) process.exitCode = 1;
} catch (error) {
  console.error(`背景图套件异常（步骤：${currentStep}）：${error.message}`);
  process.exitCode = 1;
} finally {
  clearTimeout(watchdog);
  await stopApp();
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
