// 可访问性与性能模式端到端冒烟（P4）：设置联动主窗 reduce-motion / 字体缩放 /
// 高对比叠加 / 焦点增强 / 屏幕阅读器增强 / 性能模式，并保存系统页截图。
//
// 依赖：已构建的 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 数据隔离：SRT_DATA_DIR 指向项目 tmp 工作目录；通过后清理，失败保留现场。

import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-a11y-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
writeFileSync(join(dataDir, 'settings.json'), JSON.stringify({ schemaVersion: 17 }), 'utf8');
const port = Number(argValue('--port', String(8860 + Math.floor(Math.random() * 50))));

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
  const result = await client.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(`eval 异常: ${result.exceptionDetails.text}`);
  return result.result.value;
};

const waitFor = async (client, expression, predicate, timeoutMs = 6000) => {
  const deadline = Date.now() + timeoutMs;
  let last;
  for (;;) {
    last = await evalIn(client, expression);
    if (predicate(last)) return last;
    if (Date.now() > deadline) return last;
    await delay(150);
  }
};

const setSelect = (selector, value) => `(() => {
  const el = document.querySelector(${JSON.stringify(selector)});
  if (!el) return false;
  const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set;
  setter.call(el, ${JSON.stringify(String(value))});
  el.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
})()`;

const setNumber = (selector, value) => `(() => {
  const el = document.querySelector(${JSON.stringify(selector)});
  if (!el) return false;
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
  setter.call(el, ${JSON.stringify(String(value))});
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
  return true;
})()`;

const setToggle = (selector, checked) => `(() => {
  const el = document.querySelector(${JSON.stringify(selector)});
  if (!el) return false;
  if (el.checked !== ${checked}) el.click();
  return el.checked === ${checked};
})()`;

const classState = `(() => ({
  cls: document.documentElement.className,
  zoom: document.documentElement.style.zoom,
  toastLive: document.querySelector('.toast-region')?.getAttribute('aria-live') ?? 'missing',
}))()`;

try {
  let mainWs = null;
  for (let attempt = 1; attempt <= 3 && !mainWs; attempt += 1) {
    try {
      mainWs = await findTarget(port);
    } catch (error) {
      if (attempt === 3) throw error;
      await delay(2500);
    }
  }
  const main = await createClient(mainWs);
  await main.send('Page.enable');
  await dismissOnboarding((expression) => evalIn(main, expression));
  check('A0a 主窗就绪', true);

  await waitFor(main, `!!document.querySelector('button[title="设置"]')`, (v) => v === true, 10000);
  await evalIn(main, `(() => { document.querySelector('button[title="设置"]').click(); return true; })()`);
  let settingsWs = null;
  for (let attempt = 1; attempt <= 3 && !settingsWs; attempt += 1) {
    try {
      settingsWs = await findTarget(port, 'settings.html');
    } catch (error) {
      if (attempt === 3) throw error;
      await delay(2500);
    }
  }
  const settings = await createClient(settingsWs);
  await settings.send('Page.enable');

  const tabReady = await waitFor(
    settings,
    `[...document.querySelectorAll('.tabs [role="tab"]')].map((b) => b.textContent.trim()).join(',')`,
    (v) => typeof v === 'string' && v.includes('系统') && v.includes('常规'),
  );
  check('A0b 设置窗含系统页签', String(tabReady).includes('系统'), `tabs=${tabReady}`);

  await evalIn(
    settings,
    `(() => { const tab = [...document.querySelectorAll('.tabs [role="tab"]')].find((b) => b.textContent.trim() === '系统'); tab.click(); return true; })()`,
  );
  const groupCount = await waitFor(settings, `document.querySelectorAll('.group').length`, (v) => v >= 3);
  check('A1 系统页含多组卡片（a11y/system/tools/update）', groupCount >= 3, `groups=${groupCount}`);

  await evalIn(settings, setSelect('select[data-setting="app.a11y.reduceMotion"]', 'on'));
  const rmOn = await waitFor(main, classState, (v) => v?.cls?.includes('reduce-motion'));
  check('A2 减少动画=始终减少 → 主窗挂 reduce-motion', String(rmOn.cls).includes('reduce-motion'), rmOn.cls);

  await evalIn(settings, setNumber('input[data-setting-num="app.a11y.fontScale"]', 125));
  const zoomMain = await waitFor(main, classState, (v) => v?.zoom === '1.25');
  const zoomSettings = await waitFor(settings, classState, (v) => v?.zoom === '1.25');
  check(
    'A3 字体缩放 125% → 两窗 zoom=1.25',
    zoomMain.zoom === '1.25' && zoomSettings.zoom === '1.25',
    `main=${zoomMain.zoom} settings=${zoomSettings.zoom}`,
  );

  await evalIn(settings, setToggle('input[data-setting="app.a11y.highContrastOverlay"]', true));
  const hc = await waitFor(main, classState, (v) => v?.cls?.includes('hc-overlay'));
  check('A4 高对比叠加 → hc-overlay', String(hc.cls).includes('hc-overlay'), hc.cls);

  await evalIn(settings, setToggle('input[data-setting="app.a11y.focusVisible"]', false));
  const fv = await waitFor(main, classState, (v) => !v?.cls?.includes('focus-strong'));
  check('A5 关闭焦点增强 → 移除 focus-strong', !String(fv.cls).includes('focus-strong'), fv.cls);

  await evalIn(settings, setToggle('input[data-setting="app.a11y.screenReader"]', false));
  const sr = await waitFor(main, classState, (v) => v?.toastLive === 'off');
  check('A6 关闭屏幕阅读器增强 → toast aria-live=off', sr.toastLive === 'off', `live=${sr.toastLive}`);

  await evalIn(settings, setToggle('input[data-setting="app.system.performanceMode"]', true));
  const pm = await waitFor(main, classState, (v) => v?.cls?.includes('performance-mode'));
  check(
    'A7 性能模式 → performance-mode（含减少动画）',
    String(pm.cls).includes('performance-mode') && String(pm.cls).includes('reduce-motion'),
    pm.cls,
  );

  await settings.send('Emulation.setDeviceMetricsOverride', {
    width: 980,
    height: 720,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await delay(300);
  const shot = await settings.send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(root, 'docs', 'screenshots', 'phase-p4-a11y.png'), Buffer.from(shot.data, 'base64'));
  check('A8 截图已保存', true);

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
    rmSync(work, { recursive: true, force: true });
  }
}
