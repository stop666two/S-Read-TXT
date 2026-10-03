#!/usr/bin/env node
// P1-4 过滤视图 E2E（常驻套件）。
// 场景（F1–F10）：入口可见 / 字面量过滤与计数 / 隐藏空行 / 大小写开关与无匹配 /
//   非法正则就地报错 / 清除恢复 / 切标签自动清空 / 编辑模式隐藏入口 / 截图。
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-filter.mjs [--exe <路径>] [--screenshot <路径>]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(
  argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')),
);
const screenshotPath = argValue(
  '--screenshot',
  join(root, 'docs', 'screenshots', 'phase-p1-filter.png'),
);
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '300000');

const workDir = join(tmpdir(), `srt-filter-${Date.now()}`);
const dataDir = join(workDir, 'data');
const testFileA = join(workDir, 'filter-a.txt');
const testFileB = join(workDir, 'filter-b.txt');
const port = 9450 + Math.floor(Math.random() * 200);

const checks = [];
let currentStep = 'F0 启动';
function check(name, passed, detail = '') {
  checks.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

/** 带重试的删除（进程句柄释放/杀软扫描可能短暂锁定）。 */
async function removeWithRetry(path, attempts = 12) {
  for (let i = 0; i < attempts; i += 1) {
    try {
      rmSync(path, { recursive: true, force: true });
    } catch {
      // 忽略并重试
    }
    if (!existsSync(path)) return;
    await delay(250);
  }
}

const watchdog = setTimeout(() => {
  console.error(`看门狗超时（${watchdogMs}ms），最后步骤：${currentStep}`);
  process.exit(4);
}, watchdogMs);

async function main() {
  if (!existsSync(exePath)) {
    console.error(`可执行文件不存在：${exePath}（先运行 npm run tauri build -- --debug --no-bundle）`);
    process.exit(2);
  }
  mkdirSync(workDir, { recursive: true });
  mkdirSync(dataDir, { recursive: true });
  // A：6 行（第 5 行为空行）；B：另一标签，用于切换清空验证。
  writeFileSync(testFileA, 'alpha\nbeta\ngamma\nalpha two\n\nomega', 'utf8');
  writeFileSync(testFileB, 'xyz', 'utf8');

  const child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });

  let client;
  try {
    currentStep = 'F0 等待 CDP';
    const wsUrl = await (async () => {
      const started = Date.now();
      for (;;) {
        try {
          return await findTarget(port);
        } catch {
          if (Date.now() - started > 20_000) throw new Error('CDP 目标未出现');
          await delay(250);
        }
      }
    })();
    client = await createClient(wsUrl);
    const evalJs = async (expression) => {
      const result = await client.send('Runtime.evaluate', {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
      if (result.exceptionDetails) {
        throw new Error(`页面执行异常：${result.exceptionDetails.text}`);
      }
      return result.result?.value;
    };
    const waitFor = async (fn, timeoutMs, label) => {
      const started = Date.now();
      for (;;) {
        const value = await fn();
        if (value) return value;
        if (Date.now() - started > timeoutMs) throw new Error(`等待超时：${label}`);
        await delay(120);
      }
    };
    const clickByText = (text) =>
      evalJs(
        `(() => {
          const el = [...document.querySelectorAll('button')].find((b) => b.textContent.trim().includes(${JSON.stringify(text)}));
          if (!el) return false;
          el.click();
          return true;
        })()`,
      );
    const setControl = (selector, value) =>
      evalJs(
        `(() => {
          const el = document.querySelector(${JSON.stringify(selector)});
          if (!el) return false;
          el.value = ${JSON.stringify(value)};
          el.dispatchEvent(new Event('input', { bubbles: true }));
          el.dispatchEvent(new Event('change', { bubbles: true }));
          return true;
        })()`,
      );
    const activeTab = async () =>
      evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return v.tabs.find((t) => t.tabId === v.activeTabId); })()`,
      );
    const filterCount = () =>
      evalJs(`document.querySelector('[data-filter-count]')?.textContent ?? null`);
    const renderedRowTexts = () =>
      evalJs(`[...document.querySelectorAll('.row')].map((el) => el.textContent)`);
    const applyFilter = async () => {
      await evalJs(`(document.querySelector('[data-filter-apply]')?.click(), true)`);
      await delay(260);
    };

    currentStep = 'F0 就绪等待';
    await waitFor(() => evalJs(`!!(window.__srt && window.__TAURI_INTERNALS__)`), 20_000, '前端桥接');
    await dismissOnboarding(evalJs);
    await waitFor(() => evalJs(`!!document.querySelector('.empty .open-btn')`), 8_000, '欢迎页');

    currentStep = 'F1 打开示样文件';
    await evalJs(
      `(async () => { await window.__srt.openPath(${JSON.stringify(testFileA)}); return true; })()`,
    );
    const tabA = await waitFor(activeTab, 10_000, '标签出现');
    check('F1 打开示样文件', tabA && tabA.name === 'filter-a.txt', JSON.stringify(tabA?.name));

    currentStep = 'F2 过滤入口可见';
    const toggleVisible = await evalJs(`!!document.querySelector('[data-filter-toggle]')`);
    check('F2 阅读态过滤入口可见', toggleVisible === true);

    currentStep = 'F3 字面量过滤';
    await evalJs(`(document.querySelector('[data-filter-toggle]')?.click(), true)`);
    await waitFor(() => evalJs(`!!document.querySelector('[data-filter-bar]')`), 5_000, '过滤条');
    await setControl('[data-filter-input]', 'alpha');
    await applyFilter();
    const countF3 = await filterCount();
    let rowsF3 = [];
    try {
      await waitFor(async () => ((await renderedRowTexts())?.length === 2 ? true : null), 8_000, '命中行渲染');
      rowsF3 = await renderedRowTexts();
    } catch (error) {
      const diag = await evalJs(`JSON.stringify({
        count: document.querySelector('[data-filter-count]')?.textContent ?? null,
        rows: [...document.querySelectorAll('.row')].map((el) => el.textContent),
        toasts: [...document.querySelectorAll('.toast')].map((el) => el.textContent),
        bar: !!document.querySelector('[data-filter-bar]'),
        err: document.querySelector('[data-filter-error]')?.textContent ?? null,
      })`);
      console.error('F3 诊断：', diag);
      throw error;
    }
    check('F3a 计数 2 / 6', typeof countF3 === 'string' && countF3.includes('2 / 6'), String(countF3));
    check('F3b 仅渲染命中行', JSON.stringify(rowsF3) === JSON.stringify(['alpha', 'alpha two']), JSON.stringify(rowsF3));

    currentStep = 'F4 隐藏空行';
    await setControl('[data-filter-input]', '');
    await evalJs(`(document.querySelector('[data-filter-hide-empty]')?.click(), true)`);
    await applyFilter();
    const countF4 = await filterCount();
    const rowsF4 = await renderedRowTexts();
    check('F4a 隐藏空行计数 5 / 6', typeof countF4 === 'string' && countF4.includes('5 / 6'), String(countF4));
    check('F4b 空行被滤除', rowsF4?.length === 5, JSON.stringify(rowsF4));

    currentStep = 'F5 大小写开关';
    await evalJs(`(document.querySelector('[data-filter-hide-empty]')?.click(), true)`);
    await setControl('[data-filter-input]', 'ALPHA');
    await applyFilter();
    const countF5 = await filterCount();
    check('F5a 默认不区分大小写命中 2', typeof countF5 === 'string' && countF5.includes('2 / 6'), String(countF5));
    await evalJs(`(document.querySelector('[data-filter-case]')?.click(), true)`);
    await applyFilter();
    const noMatch = await waitFor(
      () => evalJs(`!!document.querySelector('[data-filter-no-match]')`),
      5_000,
      '无匹配提示',
    );
    check('F5b 区分大小写后无匹配', noMatch === true);

    currentStep = 'F6 非法正则就地报错';
    await evalJs(`(document.querySelector('[data-filter-case]')?.click(), true)`);
    await evalJs(`(document.querySelector('[data-filter-regex]')?.click(), true)`);
    await setControl('[data-filter-input]', '(');
    await applyFilter();
    const errText = await evalJs(`document.querySelector('[data-filter-error]')?.textContent ?? null`);
    check('F6 非法正则就地报错', typeof errText === 'string' && errText.length > 0, String(errText));
    await evalJs(`(document.querySelector('[data-filter-regex]')?.click(), true)`);

    currentStep = 'F7 清除筛选';
    await evalJs(`(document.querySelector('[data-filter-clear]')?.click(), true)`);
    await delay(300);
    const countGone = await evalJs(`!document.querySelector('[data-filter-count]')`);
    const tabAfterClear = await activeTab();
    check(
      'F7 清除后恢复全量',
      countGone === true && tabAfterClear?.rowsTotal === 6,
      JSON.stringify({ countGone, rowsTotal: tabAfterClear?.rowsTotal }),
    );

    currentStep = 'F8 切标签自动清空';
    await setControl('[data-filter-input]', 'alpha');
    await applyFilter();
    const beforeSwitch = await filterCount();
    await evalJs(
      `(async () => { await window.__srt.openPath(${JSON.stringify(testFileB)}); return true; })()`,
    );
    await waitFor(async () => ((await activeTab())?.name === 'filter-b.txt' ? true : null), 10_000, 'B 标签激活');
    const countOnB = await filterCount();
    await evalJs(
      `(() => { const tab = [...document.querySelectorAll('[role="tab"]')].find((el) => el.textContent.includes('filter-a.txt')); if (tab) tab.click(); return true; })()`,
    );
    await waitFor(async () => ((await activeTab())?.name === 'filter-a.txt' ? true : null), 10_000, '回切 A');
    const countBack = await filterCount();
    check(
      'F8 切走/切回后过滤状态清空',
      typeof beforeSwitch === 'string' && countOnB === null && countBack === null,
      JSON.stringify({ beforeSwitch, countOnB, countBack }),
    );

    currentStep = 'F9 编辑模式隐藏入口';
    await evalJs(`(document.querySelector('[aria-label="切换编辑模式"]')?.click(), true)`);
    await waitFor(async () => ((await activeTab())?.editing ? true : null), 8_000, '编辑态');
    const hiddenInEdit = await evalJs(`!document.querySelector('[data-filter-toggle]')`);
    check('F9 编辑态隐藏过滤入口', hiddenInEdit === true);

    currentStep = 'F10 截图';
    await evalJs(`(document.querySelector('[aria-label="切换编辑模式"]')?.click(), true)`);
    await delay(250);
    await evalJs(`(document.querySelector('[data-filter-toggle]')?.click(), true)`);
    await setControl('[data-filter-input]', 'alpha');
    await applyFilter();
    const shot = await client.send('Page.captureScreenshot', { format: 'png' });
    const { writeFileSync: writeShot } = await import('node:fs');
    writeShot(screenshotPath, Buffer.from(shot.data, 'base64'));
    check('F10 截图保存', existsSync(screenshotPath), screenshotPath);
  } finally {
    clearTimeout(watchdog);
    try {
      if (client) client.close();
    } catch {
      // 忽略
    }
    try {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    } catch {
      // 忽略
    }
    await removeWithRetry(workDir);
  }

  const passed = checks.filter((item) => item.passed).length;
  console.log(`\n结果：${passed}/${checks.length} 通过`);
  if (passed !== checks.length) {
    for (const item of checks.filter((entry) => !entry.passed)) {
      console.log(`  失败：${item.name}${item.detail ? ` ← ${item.detail}` : ''}`);
    }
    process.exit(1);
  }
}

main().catch((error) => {
  console.error(`执行失败（${currentStep}）：${error.message}`);
  process.exit(3);
});
