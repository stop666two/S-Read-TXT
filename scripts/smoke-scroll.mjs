// 滚动渲染完整性 E2E（常驻套件）。
// 目的：编码「滑动多次后内容渲染不出来」这一缺陷类别——
//   A 随机跳转 30 轮；B 滚轮连发 5 轮；C 上下震荡 3 轮；D 滑块连调 13 轮。
// 断言：每轮 settle 后，可视区域内 .row 全部有文本（blank=0）；
//   滑块轮另断言 CSS 变量到达目标值；窗口占位无需滚动即已落定。
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createClient, delay, dismissOnboarding, findTarget, openPathDone, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(tmpdir(), `srt-scroll-${Date.now()}`);
mkdirSync(work, { recursive: true });
const dataDir = join(work, 'data');
const sample = join(work, 'big.txt');
writeFileSync(
  sample,
  Array.from({ length: 50_000 }, (_, i) => `第 ${i + 1} 行：这是用于滚动渲染完整性检验的示例内容。`).join('\n'),
  'utf8',
);
const port = 9500 + Math.floor(Math.random() * 300);

const child = spawn(exe, [], {
  env: { ...process.env, SRT_DATA_DIR: dataDir, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
  stdio: 'ignore',
});

const evalIn = async (client, expression) => {
  const result = await client.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
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
  await waitForValue(async () => ((await evalMain('(() => !!(window.__srt && window.__srt.openPath))()')) ? true : null), 30000);
  await dismissOnboarding(evalMain);
  await delay(300);
  await evalMain(openPathDone(sample));
  await waitForValue(async () => ((await evalMain(`document.querySelectorAll('.tab-bar .tab').length`)) >= 1 ? true : null), 15000);
  await delay(800);

  /** 现场：可视区域 .row 空白计数 + 滚动几何。 */
  const check = async () => {
    const raw = await evalMain(`(() => { const r = document.querySelector('.reader'); if (!r) return null;
      const rect = r.getBoundingClientRect();
      const rows = [...r.querySelectorAll('.row[data-row]')];
      let visible = 0; let blank = 0; const blanks = [];
      for (const el of rows) { const b = el.getBoundingClientRect();
        if (b.bottom > rect.top && b.top < rect.bottom && b.height > 0) { visible += 1;
          if (el.textContent.trim() === '') { blank += 1; if (blanks.length < 5) blanks.push(el.getAttribute('data-row')); } } }
      return JSON.stringify({ st: Math.round(r.scrollTop), sh: r.scrollHeight, visible, blank, blanks }); })()`);
    return raw ? JSON.parse(raw) : null;
  };

  // ---- A. 随机跳转 30 轮 ----
  let aFail = 0;
  for (let i = 0; i < 30; i += 1) {
    const target = ((i * 7919 + 13) % 97) / 97;
    await evalMain(`(() => { const r = document.querySelector('.reader'); if (r) r.scrollTop = ${target} * (r.scrollHeight - r.clientHeight); return true; })()`);
    await delay(420);
    const state = await check();
    if (!state || state.blank > 0) {
      aFail += 1;
      if (aFail === 1) console.log(`  ! A 首败 ROUND ${i}: ${JSON.stringify(state)}`);
    }
  }
  if (aFail === 0) ok('A 随机跳转 30 轮渲染完整');
  else bad('A 随机跳转', `失败 ${aFail}/30`);

  // ---- B. 滚轮连发 5 轮 ----
  const rectRaw = await evalMain(`(() => { const r = document.querySelector('.reader'); if (!r) return null; const b = r.getBoundingClientRect(); return JSON.stringify({ x: Math.round(b.left + b.width / 2), y: Math.round(b.top + b.height / 2) }); })()`);
  const point = rectRaw ? JSON.parse(rectRaw) : { x: 400, y: 400 };
  await evalMain(`(() => { const r = document.querySelector('.reader'); if (r) r.scrollTop = 0; return true; })()`);
  await delay(400);
  let bFail = 0;
  for (let burst = 0; burst < 5; burst += 1) {
    for (let k = 0; k < 20; k += 1) {
      await main.send('Input.dispatchMouseEvent', { type: 'mouseWheel', x: point.x, y: point.y, deltaX: 0, deltaY: 900 });
    }
    await delay(550);
    const state = await check();
    if (!state || state.blank > 0) {
      bFail += 1;
      if (bFail === 1) console.log(`  ! B 首败 burst ${burst}: ${JSON.stringify(state)}`);
    }
  }
  if (bFail === 0) ok('B 滚轮连发 5×20 渲染完整');
  else bad('B 滚轮连发', `失败 ${bFail}/5`);

  // ---- C. 上下震荡（快速往返 + 落点不受锚定拉扯） ----
  let cFail = 0;
  for (let round = 0; round < 3; round += 1) {
    for (let k = 0; k < 30; k += 1) {
      const ratio = k % 2 === 0 ? 0.9 : 0.1;
      await evalMain(`(() => { const r = document.querySelector('.reader'); if (r) r.scrollTop = ${ratio} * (r.scrollHeight - r.clientHeight); return true; })()`);
    }
    await delay(650);
    const state = await check();
    const raw2 = await evalMain(`(() => { const r = document.querySelector('.reader'); if (!r) return null;
      return JSON.stringify({ ratio: r.scrollTop / Math.max(1, r.scrollHeight - r.clientHeight) }); })()`);
    const ratio = raw2 ? JSON.parse(raw2).ratio : -1;
    const landed = ratio < 0.2 || ratio > 0.8;
    if (!state || state.blank > 0 || !landed) {
      cFail += 1;
      console.log(`  ! C 失败 round ${round}: landed=${ratio.toFixed(3)} ${JSON.stringify(state)}`);
    }
  }
  if (cFail === 0) ok('C 上下震荡 3×30 渲染完整且落点稳定');
  else bad('C 上下震荡', `失败 ${cFail}/3`);

  // ---- D. 滑块连调（排版变更多次后内容仍渲染 + 变量到位） ----
  await evalMain(`document.querySelector('button[title="设置"]')?.click() ?? true`);
  const set = await createClient(await findTarget(port, 'settings.html'));
  const evalSet = (expression) => evalIn(set, expression);
  await waitForValue(async () => ((await evalSet(`document.querySelectorAll('.tabs [role="tab"]').length`)) === 5 ? true : null), 20000);
  await evalSet(`(() => { const t = [...document.querySelectorAll('.tabs [role="tab"]')].find((b) => b.textContent.trim() === '阅读排版'); t?.click(); return true; })()`);
  await delay(500);
  const setNumber = (selector, value, events) =>
    evalSet(
      `(() => { const el = document.querySelector(${JSON.stringify(selector)});
        if (!el) return false;
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
        setter.call(el, '${value}');
        ${events.map((e) => `el.dispatchEvent(new Event('${e}', { bubbles: true }));`).join(' ')}
        return true; })()`,
    );
  const sizes = [8, 72, 16, 48, 16, 24, 16, 40, 16, 32, 16, 8, 16];
  let dFail = 0;
  for (let idx = 0; idx < sizes.length; idx += 1) {
    const size = sizes[idx];
    const viaSlider = idx % 2 === 1;
    // 奇数轮走真实滑块路径（input 实时 + change 提交），偶数轮走数字框（change 提交）
    if (viaSlider) {
      await setNumber('input[data-setting="reader.typography.fontSize"]', String(size), ['input', 'change']);
    } else {
      await setNumber('input[data-setting-num="reader.typography.fontSize"]', String(size), ['change']);
    }
    await waitForValue(async () => ((await evalMain(`getComputedStyle(document.documentElement).getPropertyValue('--reading-size').trim()`)) === `${size}px` ? true : null), 5000);
    await delay(320);
    const state = await check();
    const geom = await evalMain(`(() => { const r = document.querySelector('.reader'); const row = r?.querySelector('.row[data-row]');
      return JSON.stringify({ rowReal: row ? Math.round(row.getBoundingClientRect().height * 10) / 10 : null, sh: r?.scrollHeight ?? -1 }); })()`);
    const info = geom ? JSON.parse(geom) : null;
    if (!state || state.blank > 0) {
      dFail += 1;
      console.log(`  ! D 失败 size=${size}: blank=${state?.blank} ${JSON.stringify(info)}`);
    }
  }
  if (dFail === 0) ok('D 滑块连调 13 轮（含 8/72 极值）渲染完整');
  else bad('D 滑块连调', `失败 ${dFail}/13`);

  console.log(`\n滚动完整性套件：通过 ${passed}/${passed + failed}`);
  process.exitCode = failed === 0 ? 0 : 1;
} catch (error) {
  console.error('套件异常：', error instanceof Error ? error.message : error);
  process.exitCode = 1;
} finally {
  try {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } catch {
    // 忽略
  }
  await delay(400);
  for (let i = 0; i < 8; i += 1) {
    try {
      rmSync(work, { recursive: true, force: true });
    } catch {
      // 忽略
    }
    await delay(200);
  }
  process.exit(process.exitCode ?? 0);
}
