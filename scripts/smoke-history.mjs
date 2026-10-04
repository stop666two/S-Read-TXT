// S-Read-TXT 历史记录 E2E（阶段 6）：面板 / 搜索 / 进度续读 / 删除 / 清空 / 最近打开 / 快捷键。
// 用法：node scripts/smoke-history.mjs [--exe path] [--screenshot]
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
const workDir = join(tmpdir(), 'srt-smoke-history');
const dataDir = join(workDir, 'data');
const port = 9150 + Math.floor(Math.random() * 300);

let passed = 0;
let failed = 0;
const failures = [];
let child = null;
let client = null;

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
  const fileD = join(workDir, 'zd.txt');
  writeFileSync(fileA, Array.from({ length: 600 }, (_, i) => `A-${i + 1}`).join('\n') + '\n', 'utf8');
  writeFileSync(fileB, 'B1\nB2\nB3\n', 'utf8');
  writeFileSync(fileC, 'C1\nC2\nC3\n', 'utf8');
  writeFileSync(fileD, 'D1\nD2\nD3\n', 'utf8');

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

  /** 历史条目数（后端真源） */
  const historyCount = () =>
    evalJs(`window.__TAURI_INTERNALS__.invoke('get_history').then((v) => v.length)`);
  const historyRow = (name) =>
    evalJs(
      `window.__TAURI_INTERNALS__.invoke('get_history').then((v) => { const e = v.find((x) => x.name === ${JSON.stringify(name)}); return e ? JSON.stringify({ lastRow: e.lastRow, lastPercent: e.lastPercent }) : null; })`,
    );
  const tabCount = () =>
    evalJs(`window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs.length)`);
  const activeTabName = () =>
    evalJs(
      `window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs.find((t) => t.tabId === v.activeTabId)?.name ?? null)`,
    );
  const panelOpen = () => evalJs(`!!document.querySelector('.panel[role="dialog"]')`);
  const openPanelViaToolbar = () =>
    evalJs(`(document.querySelector('button[aria-label="历史记录"]')?.click(), true)`);
  const entryNames = () =>
    evalJs(
      `JSON.stringify([...document.querySelectorAll('.panel .entry .name')].map((n) => n.textContent.trim()))`,
    ).then((raw) => JSON.parse(raw ?? '[]'));

  try {
    // ---- H1 打开三个文件 → 面板与顺序 ----
    for (const path of [fileA, fileB, fileC]) await evalJs(openPathDone(path));
    await waitForValue(async () => ((await tabCount()) === 3 ? true : null), 8000);
    await openPanelViaToolbar();
    const panelShown = await waitForValue(async () => ((await panelOpen()) ? true : null), 6000);
    check('H1a 工具栏「历史记录」打开面板', panelShown === true);
    await waitForValue(async () => {
      const list = await entryNames();
      return list.length >= 3 ? true : null;
    }, 6000);
    const names = await entryNames();
    check(
      'H1b 历史按最近打开倒序（zc 在前）',
      names[0] === 'zc.txt' && names.includes('za.txt') && names.includes('zb.txt'),
      JSON.stringify(names),
    );

    // ---- H2 搜索过滤 ----
    await evalJs(
      `(() => { const input = document.querySelector('.panel input[type="search"]'); if (!input) return false; const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set; setter.call(input, 'zb'); input.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`,
    );
    const filtered = await waitForValue(async () => {
      const list = await entryNames();
      return list.length === 1 && list[0] === 'zb.txt' ? true : null;
    }, 6000);
    check('H2 搜索过滤（仅显示匹配项）', filtered === true, JSON.stringify(await entryNames()));
    // 清空搜索
    await evalJs(
      `(() => { const input = document.querySelector('.panel input[type="search"]'); if (!input) return false; const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set; setter.call(input, ''); input.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`,
    );
    await delay(200);
    // 关闭面板
    await evalJs(`(document.querySelector('.panel button[aria-label="关闭历史面板"]')?.click(), true)`);
    await waitForValue(async () => ((await panelOpen()) === false ? true : null), 4000);

    // ---- H3 进度续读：za 滚动 → 关闭标签（回写进度）→ 从历史重开 ----
    await evalJs(
      `(() => { const el = [...document.querySelectorAll('[data-tab-id]')].find((n) => n.textContent.includes('za.txt')); el?.click(); return true; })()`,
    );
    await delay(400);
    await waitForValue(async () => {
      const ready = await evalJs(`document.querySelectorAll('.reader .row').length > 0`);
      return ready ? true : null;
    }, 8000);
    await evalJs(`(document.querySelector('.reader').scrollTop = 3000, true)`);
    await waitForValue(async () => {
      const top = await evalJs(`document.querySelector('.reader').scrollTop`);
      return top > 500 ? true : null;
    }, 6000);
    await delay(400);
    // 关闭 za 标签（关闭按钮）
    await evalJs(
      `(() => { const el = [...document.querySelectorAll('[data-tab-id]')].find((n) => n.textContent.includes('za.txt')); el?.querySelector('.close')?.click(); return true; })()`,
    );
    await waitForValue(async () => ((await tabCount()) === 2 ? true : null), 6000);
    const rawHistory = await evalJs(
      `window.__TAURI_INTERNALS__.invoke('get_history').then((v) => JSON.stringify(v.map((e) => ({ n: e.name, row: e.lastRow }))))`,
    );
    console.log(`H3_DIAG=${rawHistory}`);
    const rowInfo = await waitForValue(async () => {
      const info = await historyRow('za.txt');
      if (!info) return null;
      return JSON.parse(info).lastRow > 10 ? info : null;
    }, 6000);
    check('H3a 关闭标签回写阅读进度', rowInfo !== null, rowInfo ?? 'lastRow 未更新');

    // 从历史重开 za：进度应恢复
    await openPanelViaToolbar();
    await waitForValue(async () => ((await panelOpen()) ? true : null), 6000);
    // 面板打开时异步刷新历史：必须等到 za 条目显示进度（非陈旧数据），否则
    // 点击到 lastRow=0 的旧条目会跳过进度 seed（曾致 H3b 稳定失败）。
    await waitForValue(async () => {
      const text = await evalJs(
        `(() => { const e = [...document.querySelectorAll('.panel .entry')].find((n) => n.querySelector('.name')?.textContent.trim() === 'za.txt'); return e ? (e.querySelector('.row3')?.textContent ?? '') : ''; })()`,
      );
      return String(text).includes('%') ? true : null;
    }, 6000);
    const zaEntryHint = await evalJs(
      `(() => { const e = [...document.querySelectorAll('.panel .entry')].find((n) => n.querySelector('.name')?.textContent.trim() === 'za.txt'); return e ? (e.querySelector('.row3')?.textContent ?? '') : null; })()`,
    );
    console.log(`H3b_DIAG entry3=${JSON.stringify(zaEntryHint)}`);
    await evalJs(
      `(() => { const entry = [...document.querySelectorAll('.panel .entry')].find((n) => n.querySelector('.name')?.textContent.trim() === 'za.txt'); entry?.click(); return true; })()`,
    );
    await waitForValue(async () => ((await tabCount()) === 3 ? true : null), 12000);
    const restored = await waitForValue(async () => {
      const top = await evalJs(`document.querySelector('.reader')?.scrollTop ?? 0`);
      return top > 500 ? top : null;
    }, 12000);
    check('H3b 从历史重开恢复阅读进度', restored !== null, `scrollTop=${restored}`);
    check(
      'H3c 打开后面板自动关闭且活动标签正确',
      (await panelOpen()) === false && (await activeTabName()) === 'za.txt',
      `active=${await activeTabName()}`,
    );

    // ---- H4 单条删除 ----
    await openPanelViaToolbar();
    await waitForValue(async () => ((await panelOpen()) ? true : null), 6000);
    await evalJs(
      `(() => { const entry = [...document.querySelectorAll('.panel .entry')].find((n) => n.querySelector('.name')?.textContent.trim() === 'zc.txt'); entry?.querySelector('.del')?.click(); return true; })()`,
    );
    const deleted = await waitForValue(async () => {
      const list = await entryNames();
      return !list.includes('zc.txt') ? true : null;
    }, 6000);
    check('H4 单条删除生效', deleted === true, JSON.stringify(await entryNames()));

    // ---- H5 清空全部（二次确认） ----
    await evalJs(
      `(() => { const b = [...document.querySelectorAll('.panel button')].find((x) => x.textContent.trim() === '清空历史'); b?.click(); return true; })()`,
    );
    await waitForValue(async () => {
      const shown = await evalJs(
        `[...document.querySelectorAll('[role="alertdialog"] button')].some((b) => b.textContent.trim() === '清空')`,
      );
      return shown === true ? true : null;
    }, 6000);
    await evalJs(
      `(() => { const b = [...document.querySelectorAll('[role="alertdialog"] button')].find((x) => x.textContent.trim() === '清空'); b?.click(); return true; })()`,
    );
    const cleared = await waitForValue(async () => ((await historyCount()) === 0 ? true : null), 6000);
    const emptyText = await evalJs(`document.querySelector('.panel .empty')?.textContent?.trim() ?? ''`);
    check('H5 清空历史（二次确认）后为空', cleared === true && emptyText === '暂无历史记录', emptyText);
    await evalJs(`(document.querySelector('.panel button[aria-label="关闭历史面板"]')?.click(), true)`);
    await waitForValue(async () => ((await panelOpen()) === false ? true : null), 4000);

    // ---- H6 菜单「最近打开」子菜单 ----
    // 打开一个“全新”文件（复用打开不会写历史，无法产生新记录），随后关闭它——
    // 菜单点击才能重新打开（产生新标签）。
    await evalJs(openPathDone(fileD));
    await waitForValue(async () => ((await historyCount()) >= 1 ? true : null), 6000);
    await evalJs(
      `(() => { const el = [...document.querySelectorAll('[data-tab-id]')].find((n) => n.textContent.includes('zd.txt')); el?.querySelector('.close')?.click(); return true; })()`,
    );
    await waitForValue(async () => ((await tabCount()) === 3 ? true : null), 6000);
    // 等待前端历史 store 刷新（800ms 防抖）后子菜单才有数据
    await delay(1200);
    await evalJs(
      `(() => { const b = [...document.querySelectorAll('.menu-bar .title')].find((x) => x.textContent.trim() === '文件'); b?.click(); return true; })()`,
    );
    await delay(300);
    await evalJs(
      `(() => { const b = [...document.querySelectorAll('.menu-bar .submenu-wrap button')].find((x) => x.textContent.includes('最近打开')); b?.click(); return true; })()`,
    );
    const flyoutCount = await waitForValue(async () => {
      const n = await evalJs(`document.querySelectorAll('.flyout button').length`);
      return n >= 1 ? n : null;
    }, 6000);
    check('H6a 「最近打开」子菜单显示最近条目', flyoutCount !== null, `count=${flyoutCount}`);
    const tabsBefore = await tabCount();
    await evalJs(`(document.querySelector('.flyout button')?.click(), true)`);
    const openedViaMenu = await waitForValue(async () => {
      const count = await tabCount();
      return count === tabsBefore + 1 ? true : null;
    }, 6000);
    check('H6b 点击最近条目打开新标签', openedViaMenu === true);

    // ---- H7 快捷键 Ctrl+Shift+H ----
    const pressWithMods = async (key, code, vk, modifiers) => {
      const base = { key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
      await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...base });
      await client.send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
      await delay(200);
    };
    await pressWithMods('H', 'KeyH', 72, 2 | 8);
    const shortcutPanel = await waitForValue(async () => ((await panelOpen()) ? true : null), 6000);
    check('H7 Ctrl+Shift+H 打开历史面板', shortcutPanel === true);
    await evalJs(`(document.querySelector('.panel button[aria-label="关闭历史面板"]')?.click(), true)`);

    // ---- H8/H9 长列表虚拟滚动（种子 60 条历史，走真实 UI 刷新与渲染） ----
    const seedLines = Array.from({ length: 60 }, (_, i) => {
      const name = `seed-${String(i).padStart(3, '0')}.txt`;
      return JSON.stringify({
        path: join(workDir, name),
        name,
        size: 1000 + i,
        encoding: 'UTF-8',
        openedAt: new Date(Date.now() - i * 60_000).toISOString(),
        lastRow: i,
        lastPercent: (i / 60) * 100,
      });
    }).join('\n');
    writeFileSync(join(dataDir, 'history.jsonl'), `${seedLines}\n`, 'utf8');
    // 触发前端历史 store 刷新：打开新文件（活动标签变化 → 800ms 防抖刷新）
    await evalJs(openPathDone(fileD));
    await delay(1400);
    await openPanelViaToolbar();
    await waitForValue(async () => ((await panelOpen()) ? true : null), 6000);
    const totalEntries = await historyCount();
    const renderedCount = await evalJs(`document.querySelectorAll('.panel .entry').length`);
    check(
      'H8 长列表虚拟滚动（渲染数远小于总数）',
      totalEntries >= 60 && renderedCount >= 5 && renderedCount < totalEntries / 2,
      `total=${totalEntries} rendered=${renderedCount}`,
    );
    const firstRendered = () =>
      evalJs(`document.querySelector('.panel .entry .name')?.textContent?.trim() ?? null`);
    const beforeScroll = await firstRendered();
    const scrolled = await evalJs(
      `(() => { const el = [...document.querySelectorAll('.panel *')].find((n) => n.scrollHeight > n.clientHeight + 200); if (!el) return false; el.scrollTop = el.scrollHeight; return true; })()`,
    );
    await delay(400);
    const afterScroll = await firstRendered();
    check(
      'H9 滚动后渲染窗口移动（首条变化）',
      scrolled === true && beforeScroll !== null && afterScroll !== beforeScroll,
      `${beforeScroll} → ${afterScroll}`,
    );
    await evalJs(`(document.querySelector('.panel button[aria-label="关闭历史面板"]')?.click(), true)`);
    await waitForValue(async () => ((await panelOpen()) === false ? true : null), 4000);

    if (process.argv.includes('--screenshot')) {
      await openPanelViaToolbar();
      await waitForValue(async () => ((await panelOpen()) ? true : null), 6000);
      const shot = await client.send('Page.captureScreenshot', { format: 'png' });
      mkdirSync(resolve(process.cwd(), 'docs', 'screenshots'), { recursive: true });
      writeFileSync(
        resolve(process.cwd(), 'docs', 'screenshots', 'phase6-history.png'),
        Buffer.from(shot.data, 'base64'),
      );
      console.log('PASS  截图已保存  ← docs/screenshots/phase6-history.png');
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
    console.log(`\n历史记录冒烟：${passed}/${total} 通过`);
    if (failed > 0) {
      console.log(`失败项：${failures.join('；')}`);
      process.exitCode = 1;
    }
  }
}

await main();
