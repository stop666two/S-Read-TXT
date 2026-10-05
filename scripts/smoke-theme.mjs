// 主题系统 E2E（常驻套件）。
// 覆盖：
//   T1 默认跟随系统（解析 light/dark + data-theme-base 写入）
//   T2 六套内置主题切换（themeId 与 --base 令牌逐套断言）
//   T3 跟随系统解析
//   T4 持久化：切换后重启（验证启动前令牌预载，无浅色闪烁路径）
//   T5 导入用户主题（含自定义 base 令牌生效）
//   T6 删除用户主题（清单移除 + 文件删除；active 先切回 light）
//   T7 非法导入拒绝（缺令牌）
//   T8 内置主题删除拒绝
//   T9 切换动画：切换瞬间挂 .theme-anim，随后移除
//   T10 新主题截图（人工目检）
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
const work = join(tmpdir(), `srt-theme-${Date.now()}`);
mkdirSync(work, { recursive: true });
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
const port = 9600 + Math.floor(Math.random() * 250);
const shotsDir = join(root, 'docs', 'screenshots');

const THEMES = [
  { id: 'light', base: '#FAF9F7', baseKind: 'light' },
  { id: 'dark', base: '#1E1E1E', baseKind: 'dark' },
  { id: 'eye-green', base: '#EAF1E6', baseKind: 'light' },
  { id: 'paper-cream', base: '#F5EFE0', baseKind: 'light' },
  { id: 'high-contrast', base: '#FFFFFF', baseKind: 'light' },
  { id: 'minimal-gray', base: '#F7F7F7', baseKind: 'light' },
];

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
  console.error(`主题套件看门狗超时（步骤：${currentStep}），强制退出`);
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

/** 启动应用（可重复：重启复用同一数据目录与端口） */
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
    const ready = await evalJs(`typeof window.__srt?.setTheme`);
    return ready === 'function' ? true : null;
  }, 20_000);
  await dismissOnboarding(evalJs);
}

/** 停止应用（仅本应用进程树；严禁按进程名全局清理） */
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

/** 主题 id（数据属性） */
const themeId = () => evalJs(`document.documentElement.dataset.themeId ?? null`);
/** --base 令牌（大写归一便于断言） */
const baseVar = () =>
  evalJs(`getComputedStyle(document.documentElement).getPropertyValue('--base').trim().toUpperCase()`);

/** 切换主题并等待生效（system 解析为 light/dark 之一） */
async function setTheme(id) {
  await evalJs(`(window.__srt.setTheme('${id}'), true)`);
  return waitForValue(async () => {
    const current = await themeId();
    if (id === 'system') return current === 'light' || current === 'dark' ? current : null;
    return current === id ? current : null;
  }, 6000);
}

/** 捕获式 invoke（返回 {ok, value|error}，不抛异常） */
const invokeCaught = (command, args = {}) =>
  evalJs(
    `window.__TAURI_INTERNALS__.invoke('${command}', ${JSON.stringify(args)})
      .then((value) => ({ ok: true, value }))
      .catch((error) => ({ ok: false, error: JSON.stringify(error) }))`,
  );

try {
  await startApp();

  // ---- T1 默认（跟随系统） ----
  currentStep = 'T1 默认主题';
  const t1 = await waitForValue(async () => {
    const id = await themeId();
    return id === 'light' || id === 'dark' ? id : null;
  }, 6000);
  const t1Base = await baseVar();
  const t1BaseKind = await evalJs(`document.documentElement.dataset.themeBase ?? null`);
  check(
    'T1 默认跟随系统（解析 light/dark + 令牌写入）',
    t1 !== null && t1BaseKind === (t1 === 'dark' ? 'dark' : 'light') && t1Base !== '',
    `id=${t1} base=${t1Base} kind=${t1BaseKind}`,
  );

  // ---- T2 六套内置主题切换 ----
  currentStep = 'T2 六套切换';
  let t2 = true;
  const t2Detail = [];
  for (const theme of THEMES) {
    const applied = await setTheme(theme.id);
    const base = await baseVar();
    const kind = await evalJs(`document.documentElement.dataset.themeBase ?? null`);
    if (applied !== theme.id || base !== theme.base || kind !== theme.baseKind) {
      t2 = false;
      t2Detail.push(`${theme.id}: id=${applied} base=${base}/${theme.base} kind=${kind}`);
    }
  }
  check('T2 六套内置主题切换（themeId + --base + 明暗基底）', t2, t2Detail.join(' | ') || '全部一致');

  // ---- T3 跟随系统 ----
  currentStep = 'T3 跟随系统';
  const t3 = await setTheme('system');
  check('T3 跟随系统解析为 light/dark', t3 !== null, `resolved=${t3}`);

  // ---- T4 持久化（重启预载） ----
  currentStep = 'T4 持久化';
  await setTheme('paper-cream');
  await delay(400);
  await stopApp();
  await startApp();
  const t4Id = await waitForValue(async () => {
    const id = await themeId();
    return id === 'paper-cream' ? id : null;
  }, 8000);
  const t4Base = await baseVar();
  check('T4 重启后保持主题（启动预载）', t4Id === 'paper-cream' && t4Base === '#F5EFE0', `id=${t4Id} base=${t4Base}`);

  // ---- T5 导入用户主题 ----
  currentStep = 'T5 导入用户主题';
  const lightManifest = JSON.parse(
    await evalJs(`window.__TAURI_INTERNALS__.invoke('get_theme', { id: 'light' }).then((t) => JSON.stringify(t))`),
  );
  const customManifest = {
    schemaVersion: 1,
    id: 'smoke-theme',
    name: '冒烟主题',
    nameEn: 'Smoke Theme',
    base: 'light',
    tokens: { ...lightManifest.tokens, base: '#123456' },
  };
  const customPath = join(work, 'smoke-theme.json');
  writeFileSync(customPath, JSON.stringify(customManifest, null, 2), 'utf8');
  const imported = await invokeCaught('import_theme', { path: customPath });
  const listed = await invokeCaught('list_themes');
  const listedIds = listed.ok ? listed.value.map((item) => item.id) : [];
  const appliedCustom = await setTheme('smoke-theme');
  const customBase = await baseVar();
  check(
    'T5 导入用户主题并应用（自定义 base 生效）',
    imported.ok && listedIds.includes('smoke-theme') && appliedCustom === 'smoke-theme' && customBase === '#123456',
    `import=${imported.ok} applied=${appliedCustom} base=${customBase}`,
  );

  // ---- T6 删除用户主题 ----
  currentStep = 'T6 删除用户主题';
  await setTheme('light');
  const removed = await invokeCaught('remove_theme', { id: 'smoke-theme' });
  const listedAfter = await invokeCaught('list_themes');
  const listedAfterIds = listedAfter.ok ? listedAfter.value.map((item) => item.id) : [];
  const fileGone = !existsSync(join(dataDir, 'themes', 'smoke-theme.json'));
  check(
    'T6 删除用户主题（清单移除 + 文件删除）',
    removed.ok && !listedAfterIds.includes('smoke-theme') && fileGone,
    `removed=${removed.ok} fileGone=${fileGone}`,
  );

  // ---- T7 非法导入拒绝 ----
  currentStep = 'T7 非法导入';
  const badPath = join(work, 'bad-theme.json');
  writeFileSync(
    badPath,
    JSON.stringify({ ...customManifest, id: 'bad-theme', tokens: { base: '#112233' } }, null, 2),
    'utf8',
  );
  const badImport = await invokeCaught('import_theme', { path: badPath });
  check(
    'T7 非法主题导入拒绝（缺令牌）',
    badImport.ok === false && String(badImport.error).includes('缺少令牌'),
    String(badImport.error).slice(0, 120),
  );

  // ---- T8 内置主题删除拒绝 ----
  currentStep = 'T8 内置删除拒绝';
  const builtinRemove = await invokeCaught('remove_theme', { id: 'light' });
  check(
    'T8 内置主题删除拒绝',
    builtinRemove.ok === false && String(builtinRemove.error).includes('内置主题不可删除'),
    String(builtinRemove.error).slice(0, 120),
  );

  // ---- T9 切换动画类 ----
  currentStep = 'T9 切换动画';
  await evalJs(`(window.__srt.setTheme('dark'), true)`);
  const animSeen = await waitForValue(async () => {
    const has = await evalJs(`document.documentElement.classList.contains('theme-anim')`);
    return has ? true : null;
  }, 2500);
  const animGone = await waitForValue(async () => {
    const has = await evalJs(`document.documentElement.classList.contains('theme-anim')`);
    return has ? null : true;
  }, 4000);
  check('T9 切换动画类（出现后移除）', animSeen === true && animGone === true);

  // ---- T10 截图（护眼绿 / 高对比） ----
  currentStep = 'T10 截图';
  mkdirSync(shotsDir, { recursive: true });
  for (const id of ['eye-green', 'high-contrast']) {
    await setTheme(id);
    await delay(450);
    const shot = await client.send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(join(shotsDir, `phase5-theme-${id}.png`), Buffer.from(shot.data, 'base64'));
  }
  check('T10 新主题截图已保存', true, 'phase5-theme-eye-green.png / phase5-theme-high-contrast.png');

  console.log(`\n主题系统套件：通过 ${passed}/${passed + failed}`);
  if (failed > 0) process.exitCode = 1;
} catch (error) {
  console.error(`主题套件异常（步骤：${currentStep}）：${error.message}`);
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
