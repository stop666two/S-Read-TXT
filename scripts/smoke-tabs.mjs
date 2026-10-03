// S-Read-TXT 多标签增强 E2E（阶段 5 完成）：中键关闭 / 右键菜单 / 拖拽排序 / 上限提示 / 溢出。
// 用法：node scripts/smoke-tabs.mjs [--exe path] [--screenshot]
// 前置：npm run tauri build -- --debug --no-bundle

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

import {
  argValue,
  createClient,
  delay,
  dismissOnboarding,
  findTarget,
  openPathDone,
  waitForValue,
} from './lib/smoke-cdp.mjs';

const exePath = argValue(
  '--exe',
  resolve(process.cwd(), 'src-tauri', 'target', 'debug', 's-read-txt.exe'),
);
const workDir = join(tmpdir(), 'srt-smoke-tabs');
const dataDir = join(workDir, 'data');
const port = 9000 + Math.floor(Math.random() * 300);

let passed = 0;
let failed = 0;
const failures = [];
let child = null;
let client = null;

/** 断言与输出（中文）。 */
function check(name, ok, detail = '') {
  if (ok) {
    passed += 1;
    console.log(`PASS  ${name}${detail ? `  ← ${detail}` : ''}`);
  } else {
    failed += 1;
    failures.push(name);
    console.log(`FAIL  ${name}${detail ? `  ← ${detail}` : ''}`);
  }
}

/** 结束本应用进程树（严禁按名称杀共享 WebView2）。 */
function killTree(pid) {
  if (pid) spawnSync('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore' });
}

async function main() {
  if (!existsSync(exePath)) {
    console.error(`可执行文件不存在：${exePath}`);
    process.exit(2);
  }
  rmSync(workDir, { recursive: true, force: true });
  mkdirSync(dataDir, { recursive: true });
  const fileA = join(workDir, 'za.txt');
  const fileB = join(workDir, 'zb.txt');
  const fileC = join(workDir, 'zc.txt');
  writeFileSync(fileA, 'A1\nA2\nA3\n', 'utf8');
  writeFileSync(fileB, 'B1\nB2\nB3\n', 'utf8');
  writeFileSync(fileC, 'C1\nC2\nC3\n', 'utf8');
  for (let i = 0; i < 10; i += 1) {
    writeFileSync(join(workDir, `extra-${i}.txt`), `E${i}-1\nE${i}-2\n`, 'utf8');
  }

  child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });
  client = await createClient(await findTarget(port));
  const evalJs = async (expression) => {
    const result = await client.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (result.exceptionDetails) throw new Error(`页面脚本异常：${result.exceptionDetails.text}`);
    return result.result?.value;
  };
  await waitForValue(async () => {
    const ready = await evalJs('!!window.__srt?.openPath && !!window.__TAURI_INTERNALS__');
    return ready ? true : null;
  }, 30000);
  await dismissOnboarding(evalJs);

  const names = () =>
    evalJs(
      `window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => JSON.stringify(v.tabs.map((t) => t.name)))`,
    ).then((raw) => JSON.parse(raw ?? '[]'));
  const count = () =>
    evalJs(`window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs.length)`);

  /** 标签中心坐标（按文件名子串定位）。 */
  const tabPoint = (fragment) =>
    evalJs(
      `(() => { const el = [...document.querySelectorAll('[data-tab-id]')].find((n) => n.textContent.includes(${JSON.stringify(fragment)})); if (!el) return 'none'; const r = el.getBoundingClientRect(); return JSON.stringify({ x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2), left: Math.round(r.left) }); })()`,
    ).then((raw) => (raw === 'none' ? null : JSON.parse(raw)));

  /** CDP 鼠标动作。 */
  const mouse = (type, x, y, button, buttons, clickCount = 1) =>
    client.send('Input.dispatchMouseEvent', { type, x, y, button, buttons, clickCount });

  try {
    // ---- T1 打开三个文件 ----
    for (const path of [fileA, fileB, fileC]) await evalJs(openPathDone(path));
    await waitForValue(async () => ((await count()) === 3 ? true : null), 8000);
    check(
      'T1 三个标签按打开顺序排列',
      JSON.stringify(await names()) === JSON.stringify(['za.txt', 'zb.txt', 'zc.txt']),
      JSON.stringify(await names()),
    );

    // ---- T2 中键关闭 zb ----
    const bPoint = await tabPoint('zb.txt');
    if (bPoint) {
      await mouse('mousePressed', bPoint.x, bPoint.y, 'middle', 4);
      await mouse('mouseReleased', bPoint.x, bPoint.y, 'middle', 0);
    }
    const closed = await waitForValue(async () => ((await count()) === 2 ? true : null), 6000);
    check('T2 中键关闭标签', closed === true && !(await names()).includes('zb.txt'), JSON.stringify(await names()));

    // ---- T3 拖拽排序：zc 拖到最前 ----
    const cPoint = await tabPoint('zc.txt');
    const aPoint = await tabPoint('za.txt');
    if (cPoint && aPoint) {
      await mouse('mousePressed', cPoint.x, cPoint.y, 'left', 1);
      const targetX = aPoint.left + 4;
      const steps = 5;
      for (let i = 1; i <= steps; i += 1) {
        const x = Math.round(cPoint.x + ((targetX - cPoint.x) * i) / steps);
        await mouse('mouseMoved', x, cPoint.y, 'left', 1);
        await delay(60);
      }
      await mouse('mouseReleased', targetX, cPoint.y, 'left', 0);
    }
    const reordered = await waitForValue(async () => {
      const order = await names();
      return order[0] === 'zc.txt' ? true : null;
    }, 8000);
    check('T3 拖拽排序（zc 移至首位）', reordered === true, JSON.stringify(await names()));

    // ---- T4 右键菜单：关闭其他 ----
    await evalJs(openPathDone(fileB));
    await waitForValue(async () => ((await count()) === 3 ? true : null), 8000);
    const aPoint2 = await tabPoint('za.txt');
    if (aPoint2) {
      await mouse('mousePressed', aPoint2.x, aPoint2.y, 'right', 2);
      await mouse('mouseReleased', aPoint2.x, aPoint2.y, 'right', 0);
    }
    const menuShown = await waitForValue(async () => {
      const countText = await evalJs(`document.querySelectorAll('.tab-menu button[role="menuitem"]').length`);
      return countText === 3 ? true : null;
    }, 6000);
    check('T4a 右键菜单出现（关闭/关闭其他/关闭全部）', menuShown === true);
    await evalJs(
      `(() => { const b = [...document.querySelectorAll('.tab-menu button')].find((x) => x.textContent.trim() === '关闭其他'); b?.click(); return true; })()`,
    );
    const othersClosed = await waitForValue(async () => ((await count()) === 1 ? true : null), 8000);
    check(
      'T4b 「关闭其他」仅保留当前标签',
      othersClosed === true && (await names())[0] === 'za.txt',
      JSON.stringify(await names()),
    );

    // ---- T5 右键菜单：关闭全部 ----
    await evalJs(openPathDone(fileC));
    await waitForValue(async () => ((await count()) === 2 ? true : null), 8000);
    const cPoint2 = await tabPoint('zc.txt');
    if (cPoint2) {
      await mouse('mousePressed', cPoint2.x, cPoint2.y, 'right', 2);
      await mouse('mouseReleased', cPoint2.x, cPoint2.y, 'right', 0);
    }
    await waitForValue(async () => ((await evalJs(`document.querySelectorAll('.tab-menu button').length`)) === 3 ? true : null), 6000);
    await evalJs(
      `(() => { const b = [...document.querySelectorAll('.tab-menu button')].find((x) => x.textContent.trim() === '关闭全部'); b?.click(); return true; })()`,
    );
    const allClosed = await waitForValue(async () => ((await count()) === 0 ? true : null), 8000);
    const emptyShown = await evalJs(
      `(() => { const el = document.querySelector('.empty'); return el ? (el.querySelector('.open-btn')?.textContent ?? '') : ''; })()`,
    );
    check('T5 「关闭全部」后回到空状态', allClosed === true && emptyShown.includes('打开'));

    // ---- T6 标签上限提示（maxTabs=2） ----
    const settingsRaw = await evalJs(
      `window.__TAURI_INTERNALS__.invoke('get_settings').then((s) => JSON.stringify(s))`,
    );
    const settings = JSON.parse(settingsRaw);
    const patched = {
      app: { ...settings.app, maxTabs: 2 },
      reader: settings.reader,
      shortcuts: settings.shortcuts.bindings ?? {},
    };
    await evalJs(
      `(async () => { await window.__TAURI_INTERNALS__.invoke('save_settings', { request: ${JSON.stringify(patched)} }); return true; })()`,
    );
    await delay(300);
    await evalJs(openPathDone(fileA));
    await evalJs(openPathDone(fileB));
    await waitForValue(async () => ((await count()) === 2 ? true : null), 8000);
    await evalJs(openPathDone(fileC));
    await delay(600);
    const toastText = await evalJs(
      `[...document.querySelectorAll('.toast .text')].map((n) => n.textContent.trim()).join('|')`,
    );
    check(
      'T6 超出上限提示且不新增标签',
      (await count()) === 2 && /上限/.test(toastText ?? ''),
      `count=${await count()} toast=${toastText}`,
    );

    // ---- T7 标签溢出：横向滚动（滚轮纵向转横向） ----
    await evalJs(
      `(async () => { const s = await window.__TAURI_INTERNALS__.invoke('get_settings');
          await window.__TAURI_INTERNALS__.invoke('save_settings', { request: { app: { ...s.app, maxTabs: 20 }, reader: s.reader, shortcuts: s.shortcuts.bindings } });
          return true; })()`,
    );
    for (let i = 0; i < 10; i += 1) await evalJs(openPathDone(join(workDir, `extra-${i}.txt`)));
    await waitForValue(async () => ((await count()) === 12 ? true : null), 12000);
    const barBox = await evalJs(
      `(() => { const bar = document.querySelector('.tab-bar'); if (!bar) return null; const r = bar.getBoundingClientRect(); return JSON.stringify({ x: Math.round(r.left + r.width - 30), y: Math.round(r.top + r.height / 2), sw: bar.scrollWidth, cw: bar.clientWidth }); })()`,
    ).then((raw) => (raw ? JSON.parse(raw) : null));
    check('T7a 标签栏出现横向溢出', barBox !== null && barBox.sw > barBox.cw, JSON.stringify(barBox));
    if (barBox) {
      await client.send('Input.dispatchMouseEvent', {
        type: 'mouseWheel',
        x: barBox.x,
        y: barBox.y,
        deltaX: 0,
        deltaY: 240,
      });
      await delay(400);
    }
    const scrollLeft = await evalJs(`document.querySelector('.tab-bar')?.scrollLeft ?? -1`);
    check('T7b 滚轮纵向转横向滚动', scrollLeft > 0, `scrollLeft=${scrollLeft}`);

    // ---- T8 拖拽取消（pointercancel）不产生重排且清理状态 ----
    const orderBefore = JSON.stringify(await names());
    await evalJs(
      `(() => { const el = [...document.querySelectorAll('[data-tab-id]')][1]; if (!el) return false;
          const r = el.getBoundingClientRect(); const y = r.top + r.height / 2;
          const opts = (x) => ({ bubbles: true, cancelable: true, clientX: x, clientY: y, pointerId: 7, button: 0, buttons: 1, isPrimary: true });
          el.dispatchEvent(new PointerEvent('pointerdown', opts(r.left + 8)));
          el.dispatchEvent(new PointerEvent('pointermove', opts(r.left + 80)));
          el.dispatchEvent(new PointerEvent('pointercancel', opts(r.left + 80)));
          return true; })()`,
    );
    await delay(300);
    const dragState = await evalJs(
      `(() => JSON.stringify({ dragging: !!document.querySelector('.tab-bar .dragging'), drop: !!document.querySelector('.tab-bar .drop-line') }))()`,
    );
    const orderAfter = JSON.stringify(await names());
    check(
      'T8 拖拽取消：顺序不变且状态清理',
      orderBefore === orderAfter &&
        JSON.parse(dragState).dragging === false &&
        JSON.parse(dragState).drop === false,
      `${orderAfter} ${dragState}`,
    );

    if (process.argv.includes('--screenshot')) {
      const shot = await client.send('Page.captureScreenshot', { format: 'png' });
      mkdirSync(resolve(process.cwd(), 'docs', 'screenshots'), { recursive: true });
      writeFileSync(
        resolve(process.cwd(), 'docs', 'screenshots', 'phase5-tabs.png'),
        Buffer.from(shot.data, 'base64'),
      );
      console.log('PASS  截图已保存  ← docs/screenshots/phase5-tabs.png');
    }
  } catch (error) {
    failed += 1;
    failures.push(`异常：${error?.message ?? error}`);
    console.log(`FAIL  执行异常  ← ${error?.message ?? error}`);
  } finally {
    killTree(child?.pid);
    await delay(400);
    try {
      rmSync(workDir, { recursive: true, force: true });
    } catch {
      // 句柄释放延迟：忽略
    }
    const total = passed + failed;
    console.log(`\n多标签冒烟：${passed}/${total} 通过`);
    if (failed > 0) {
      console.log(`失败项：${failures.join('；')}`);
      process.exitCode = 1;
    }
  }
}

await main();
