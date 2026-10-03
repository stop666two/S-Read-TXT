// 设置窗口 v2（P0-4）E2E：注册表驱动界面——
//   V1 搜索过滤 / V2 无匹配提示 / V3 清空搜索恢复 / V4 单项恢复默认 /
//   V5 分组恢复默认（确认框） / V6 全部恢复默认（确认框） /
//   V7 导出按钮打开保存对话框 / V8 导入按钮打开文件对话框 /
//   V9 语言下拉即时切换 / V10 分组折叠展开。
// 前置：npm run tauri build -- --debug --no-bundle
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDialogOps } from './lib/dialog.mjs';
import { createClient, delay, dismissOnboarding, findTarget, waitForValue } from './lib/smoke-cdp.mjs';
import { removeWithRetry } from './lib/system.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(tmpdir(), `srt-settings-v2-${Date.now()}`);
mkdirSync(work, { recursive: true });
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
// 预置当前 schema 版本（避免迁移噪音；界面断言只看默认值）
writeFileSync(join(dataDir, 'settings.json'), JSON.stringify({ schemaVersion: 2 }), 'utf8');
const port = 10100 + Math.floor(Math.random() * 300);

const child = spawn(exe, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
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

/** 生成「设置 input/select 值并派发事件」的表达式（原生 setter 兼容受控组件） */
function setControl(selector, value, kind = 'input') {
  const proto = kind === 'select' ? 'HTMLSelectElement' : 'HTMLInputElement';
  return `(() => {
    const el = document.querySelector(${JSON.stringify(selector)});
    if (!el) return false;
    const setter = Object.getOwnPropertyDescriptor(${proto}.prototype, 'value').set;
    setter.call(el, ${JSON.stringify(String(value))});
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  })()`;
}

/** 点击匹配文本的按钮（限定在弹窗或全局） */
function clickByText(containerSelector, text) {
  return `(() => {
    const scope = ${containerSelector ? `document.querySelector(${JSON.stringify(containerSelector)})` : 'document'};
    if (!scope) return false;
    const btn = [...scope.querySelectorAll('button')].find((b) => b.textContent.includes(${JSON.stringify(text)}));
    if (!btn) return false;
    btn.click();
    return true;
  })()`;
}

const watchdog = setTimeout(() => {
  console.error('设置 v2 套件看门狗超时，强制退出');
  spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  process.exit(4);
}, 300_000);

try {
  const main = await createClient(await findTarget(port));
  const evalMain = (expression) => evalIn(main, expression);
  await waitForValue(
    async () => ((await evalMain('(() => !!(window.__srt && window.__srt.openPath))()')) ? true : null),
    30000,
  );
  await dismissOnboarding(evalMain);
  await delay(300);

  // 打开设置窗口（经命令；settings.html 是独立 CDP 目标）
  await evalMain(`(() => { void window.__TAURI_INTERNALS__.invoke('open_settings', {}); return true; })()`);
  const settingsTarget = await findTarget(port, 'settings.html');
  const settings = await createClient(settingsTarget);
  const evalSet = (expression) => evalIn(settings, expression);
  await waitForValue(
    async () => ((await evalSet(`document.querySelectorAll('[data-setting]').length`)) >= 5 ? true : null),
    20000,
  );
  const dialogOps = createDialogOps(child.pid);

  // ---- V1 搜索过滤 ----
  await evalSet(setControl('.searchbar input', '字号'));
  const filtered = await waitForValue(async () => {
    const ids = await evalSet(
      `[...document.querySelectorAll('[data-setting]')].map((e) => e.getAttribute('data-setting')).join('|')`,
    );
    return typeof ids === 'string' && ids.includes('reader.typography.fontSize') && !ids.includes('app.maxTabs')
      ? true
      : null;
  }, 6000);
  check('V1 搜索仅显示匹配项', filtered === true);

  // ---- V2 无匹配提示 ----
  await evalSet(setControl('.searchbar input', 'zzzz不存在zzzz'));
  const emptyShown = await waitForValue(async () => {
    const text = await evalSet(`document.querySelector('.empty')?.textContent ?? ''`);
    return typeof text === 'string' && text.includes('没有匹配') ? true : null;
  }, 6000);
  check('V2 无匹配提示', emptyShown === true);

  // ---- V3 清空搜索恢复分组视图 ----
  await evalSet(setControl('.searchbar input', ''));
  const restored = await waitForValue(
    async () => ((await evalSet(`!!document.querySelector('[data-setting="app.maxTabs"]')`)) ? true : null),
    6000,
  );
  check('V3 清空搜索恢复分组视图', restored === true);

  // ---- V4 单项恢复默认（阅读排版 → 字号 30 → 重置 → 16） ----
  await evalSet(clickByText('.tabs', '阅读排版'));
  await delay(300);
  const fontSizeSel = 'input[data-setting="reader.typography.fontSize"]';
  await evalSet(setControl(fontSizeSel, 30));
  await waitForValue(async () => ((await evalSet(`document.querySelector(${JSON.stringify(fontSizeSel)})?.value`)) === '30' ? true : null), 6000);
  await evalSet(`(() => { document.querySelector('[data-setting-reset="reader.typography.fontSize"]')?.click(); return true; })()`);
  const resetOk = await waitForValue(async () => {
    const value = await evalSet(`document.querySelector(${JSON.stringify(fontSizeSel)})?.value`);
    return value === '16' ? true : null;
  }, 8000);
  check('V4 单项恢复默认（字号 30→16）', resetOk === true);

  // ---- V5 分组恢复默认（段间距 20 → 重置本组 → 0） ----
  const spacingSel = 'input[data-setting="reader.typography.paragraphSpacing"]';
  await evalSet(setControl(spacingSel, 20));
  await waitForValue(async () => ((await evalSet(`document.querySelector(${JSON.stringify(spacingSel)})?.value`)) === '20' ? true : null), 6000);
  await evalSet(`(() => { document.querySelector('[data-setting-reset-group="reader.typography"]')?.click(); return true; })()`);
  await waitForValue(async () => ((await evalSet(`!!document.querySelector('[role="alertdialog"]')`)) ? true : null), 6000);
  await evalSet(clickByText('[role="alertdialog"]', '恢复本组'));
  const groupResetOk = await waitForValue(async () => {
    const value = await evalSet(`document.querySelector(${JSON.stringify(spacingSel)})?.value`);
    return value === '0' ? true : null;
  }, 8000);
  check('V5 分组恢复默认（段间距 20→0）', groupResetOk === true);

  // ---- V6 全部恢复默认（常规页：标签上限 30 → 全部恢复 → 20） ----
  await evalSet(clickByText('.tabs', '常规'));
  await delay(300);
  const tabsSel = 'input[data-setting="app.maxTabs"]';
  await evalSet(setControl(tabsSel, 30));
  await waitForValue(async () => ((await evalSet(`document.querySelector(${JSON.stringify(tabsSel)})?.value`)) === '30' ? true : null), 6000);
  await evalSet(`(() => { document.querySelector('[data-setting="resetAllSettings"]')?.click(); return true; })()`);
  await waitForValue(async () => ((await evalSet(`!!document.querySelector('[role="alertdialog"]')`)) ? true : null), 6000);
  await evalSet(clickByText('[role="alertdialog"]', '全部恢复'));
  const allResetOk = await waitForValue(async () => {
    const value = await evalSet(`document.querySelector(${JSON.stringify(tabsSel)})?.value`);
    return value === '20' ? true : null;
  }, 8000);
  check('V6 全部恢复默认（标签上限 30→20）', allResetOk === true);

  // ---- V7/V8 导出/导入按钮：原生对话框打开（取消关闭） ----
  await evalSet(`(() => { document.querySelector('[data-setting="exportSettings"]')?.click(); return true; })()`);
  const exportOpen = await dialogOps.waitDialog(true);
  if (exportOpen) dialogOps.dialogOp('close');
  check('V7 导出按钮打开保存对话框', exportOpen === true);
  await dialogOps.waitDialog(false, 5000);

  await evalSet(`(() => { document.querySelector('[data-setting="importSettings"]')?.click(); return true; })()`);
  const importOpen = await dialogOps.waitDialog(true);
  if (importOpen) dialogOps.dialogOp('close');
  check('V8 导入按钮打开文件对话框', importOpen === true);
  await dialogOps.waitDialog(false, 5000);

  // ---- V9 语言下拉即时切换（zh→en→zh） ----
  const localeSel = 'select[data-setting="app.locale"]';
  await evalSet(setControl(localeSel, 'en', 'select'));
  const enOk = await waitForValue(async () => {
    const label = await evalSet(
      `[...document.querySelectorAll('.tabs [role="tab"]')].map((b) => b.textContent.trim()).join('|')`,
    );
    return typeof label === 'string' && label.includes('General') ? true : null;
  }, 6000);
  await evalSet(setControl(localeSel, 'zh-CN', 'select'));
  const zhBack = await waitForValue(async () => {
    const label = await evalSet(
      `[...document.querySelectorAll('.tabs [role="tab"]')].map((b) => b.textContent.trim()).join('|')`,
    );
    return typeof label === 'string' && label.includes('常规') ? true : null;
  }, 6000);
  check('V9 语言下拉即时切换', enOk === true && zhBack === true, `en=${enOk} zh=${zhBack}`);

  // ---- V10 分组折叠/展开 ----
  await evalSet(`(() => { document.querySelector('.group[data-setting-group="app.basic"] .collapse')?.click(); return true; })()`);
  const collapsed = await waitForValue(
    async () => ((await evalSet(`!document.querySelector('.group[data-setting-group="app.basic"] .rows')`)) ? true : null),
    5000,
  );
  await evalSet(`(() => { document.querySelector('.group[data-setting-group="app.basic"] .collapse')?.click(); return true; })()`);
  const expanded = await waitForValue(
    async () => ((await evalSet(`!!document.querySelector('.group[data-setting-group="app.basic"] .rows')`)) ? true : null),
    5000,
  );
  check('V10 分组折叠/展开', collapsed === true && expanded === true, `collapsed=${collapsed} expanded=${expanded}`);

  console.log(`\n设置 v2 套件：通过 ${passed}/${passed + failed}`);
  process.exitCode = failed === 0 ? 0 : 1;
} catch (error) {
  console.error(`设置 v2 套件异常：${error?.message ?? error}`);
  process.exitCode = 1;
} finally {
  clearTimeout(watchdog);
  spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  await delay(500);
  try {
    removeWithRetry(work);
  } catch {
    // 清理失败不阻塞
  }
}
