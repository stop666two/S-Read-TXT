#!/usr/bin/env node
// P1-1 批量插入序号 E2E（常驻套件）。
// 场景（B1–B15）：菜单入口 / 默认预览 / 零填充与前后缀 / 行范围（跳空行）/ 模板 /
//   应用（单撤销步）+ 撤销还原 / 圆圈数字超限错误就地展示 / BATCH_INVALID 错误码 / Esc 关闭 /
//   大范围应用进度事件与完成 / 运行中取消（完整回滚、无撤销步骤）/ 截图。
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-batch.mjs [--exe <路径>] [--screenshot <路径>]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const screenshotPath = argValue('--screenshot', join(root, 'docs', 'screenshots', 'phase-p1-batch.png'));
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '300000');

const workDir = join(tmpdir(), `srt-batch-${Date.now()}`);
const dataDir = join(workDir, 'data');
const testFile = join(workDir, 'batch-sample.txt');
const largeFile = join(workDir, 'batch-large.txt');
const port = 9700 + Math.floor(Math.random() * 250);

const checks = [];
let currentStep = 'B0 启动';
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
  writeFileSync(testFile, 'l1\nl2\n\nl4\nl5\nl6', 'utf8');
  writeFileSync(
    largeFile,
    Array.from({ length: 8000 }, (_, index) => `x${index}`).join('\n'),
    'utf8',
  );

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
    currentStep = 'B0 等待 CDP';
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

    /** 轮询直到 fn 返回真值或超时。 */
    const waitFor = async (fn, timeoutMs, label) => {
      const started = Date.now();
      for (;;) {
        const value = await fn();
        if (value) return value;
        if (Date.now() - started > timeoutMs) throw new Error(`等待超时：${label}`);
        await delay(120);
      }
    };

    /** 按可见文本点击按钮（菜单/工具栏通用）。 */
    const clickByText = (text) =>
      evalJs(
        `(() => {
          const el = [...document.querySelectorAll('button')].find((b) => b.textContent.trim().includes(${JSON.stringify(text)}));
          if (!el) return false;
          el.click();
          return true;
        })()`,
      );

    /** 设置控件值（select/text/number 通用；同时派发 input/change 触发绑定）。 */
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

    const previewBadges = () =>
      evalJs(
        `[...document.querySelectorAll('[data-batch-item] .badge')].map((el) => el.textContent)`,
      );

    const previewRows = () =>
      evalJs(
        `[...document.querySelectorAll('[data-batch-item] .no')].map((el) => Number(el.textContent))`,
      );

    currentStep = 'B0 就绪等待';
    await waitFor(() => evalJs(`!!(window.__srt && window.__TAURI_INTERNALS__)`), 20_000, '前端桥接');
    await dismissOnboarding(evalJs);
    await waitFor(() => evalJs(`!!document.querySelector('.empty .open-btn')`), 8_000, '欢迎页');

    currentStep = 'B1 打开文件';
    await evalJs(
      `(async () => { await window.__srt.openPath(${JSON.stringify(testFile)}); return true; })()`,
    );
    const tab1 = await waitFor(activeTab, 10_000, '标签出现');
    check('B1 打开示样文件', tab1 && tab1.name === 'batch-sample.txt', JSON.stringify(tab1?.name));
    const tabId = tab1.tabId;

    currentStep = 'B2 进入编辑';
    await evalJs(`(document.querySelector('[aria-label="切换编辑模式"]')?.click(), true)`);
    await waitFor(async () => ((await activeTab())?.editing ? true : null), 8_000, '编辑态');
    await waitFor(() => evalJs(`!!document.querySelector('textarea.input-proxy')`), 8_000, '编辑层');
    check('B2 编辑层挂载', (await activeTab())?.editing === true);

    currentStep = 'B3 菜单入口';
    await clickByText('编辑');
    await delay(200);
    const menuClicked = await clickByText('批量插入');
    await waitFor(() => evalJs(`!!document.querySelector('[data-batch-dialog]')`), 5_000, '批量弹窗');
    check('B3 菜单打开批量弹窗', menuClicked === true);

    currentStep = 'B4 默认预览';
    await evalJs(`document.querySelector('[data-setting="batch.preview"]').click()`);
    await waitFor(() => evalJs(`!!document.querySelector('[data-batch-preview]')`), 5_000, '预览区块');
    let badges = await previewBadges();
    let rows = await previewRows();
    check('B4a 默认首条为 1.', badges?.[0] === '1.', JSON.stringify(badges?.slice(0, 3)));
    check(
      'B4b 默认跳空行：6 行中 5 行待编号',
      badges?.length === 5 && rows?.[0] === 0 && rows?.[2] === 3,
      `len=${badges?.length} rows=${JSON.stringify(rows)}`,
    );

    currentStep = 'B5 零填充';
    await setControl('[data-setting="batch.format"]', 'zeroPad');
    await delay(150);
    await setControl('[data-setting="batch.start"]', '5');
    await delay(150);
    await evalJs(`document.querySelector('[data-setting="batch.preview"]').click()`);
    await delay(400);
    badges = await previewBadges();
    check('B5 零填充 05.', badges?.[0] === '05.', JSON.stringify(badges?.[0]));

    currentStep = 'B6 分隔符与后缀';
    await setControl('[data-setting="batch.separator"]', '、');
    await setControl('[data-setting="batch.suffix"]', '）');
    await delay(150);
    await evalJs(`document.querySelector('[data-setting="batch.preview"]').click()`);
    await delay(400);
    badges = await previewBadges();
    check('B6 前缀后缀拼接', badges?.[0] === '05、）', JSON.stringify(badges?.[0]));

    currentStep = 'B7 行范围与跳空行';
    await setControl('[data-setting="batch.scope"]', 'rowRange');
    await delay(250);
    await setControl('[data-setting="batch.rangeFrom"]', '1');
    await setControl('[data-setting="batch.rangeTo"]', '3');
    await delay(150);
    await evalJs(`document.querySelector('[data-setting="batch.preview"]').click()`);
    await delay(400);
    rows = await previewRows();
    badges = await previewBadges();
    check(
      'B7 范围 1-3 跳过空行（2 条）',
      rows?.length === 2 && rows?.[0] === 1 && rows?.[1] === 3,
      JSON.stringify(rows),
    );

    currentStep = 'B8 模板变量';
    await setControl('[data-setting="batch.template"]', '{n}-{line}');
    await delay(150);
    await evalJs(`document.querySelector('[data-setting="batch.preview"]').click()`);
    await delay(400);
    badges = await previewBadges();
    check('B8 模板 {n}-{line}（{line} 为 1 基行号）', badges?.[0] === '5-2', JSON.stringify(badges?.[0]));

    currentStep = 'B9 应用（单撤销步）';
    await setControl('[data-setting="batch.template"]', '');
    await setControl('[data-setting="batch.scope"]', 'all');
    await setControl('[data-setting="batch.format"]', 'arabic');
    await setControl('[data-setting="batch.start"]', '1');
    await setControl('[data-setting="batch.separator"]', '.');
    await setControl('[data-setting="batch.suffix"]', '');
    await delay(200);
    await evalJs(`document.querySelector('[data-setting="batch.apply"]').click()`);
    await waitFor(() => evalJs(`!document.querySelector('[data-batch-dialog]')`), 5_000, '弹窗关闭');
    const rowsAfter = await evalJs(
      `window.__TAURI_INTERNALS__.invoke('get_rows', { tabId: ${tabId}, startRow: 0, count: 6 })`,
    );
    const firstText = rowsAfter?.rows?.[0]?.text ?? '';
    check('B9a 首行已编号', firstText.startsWith('1.l1'), JSON.stringify(firstText));
    check('B9b 出现脏标记', (await activeTab())?.dirty === true);

    currentStep = 'B10 单步撤销';
    await evalJs(`window.__TAURI_INTERNALS__.invoke('undo_edit', { tabId: ${tabId} })`);
    await delay(500);
    const rowsUndo = await evalJs(
      `window.__TAURI_INTERNALS__.invoke('get_rows', { tabId: ${tabId}, startRow: 0, count: 6 })`,
    );
    check(
      'B10 撤销完全还原',
      (rowsUndo?.rows?.[0]?.text ?? '') === 'l1' && (await activeTab())?.dirty === false,
    );

    currentStep = 'B11 超限错误就地展示';
    await clickByText('编辑');
    await delay(200);
    await clickByText('批量插入');
    await waitFor(() => evalJs(`!!document.querySelector('[data-batch-dialog]')`), 5_000, '批量弹窗');
    await setControl('[data-setting="batch.format"]', 'circled');
    await setControl('[data-setting="batch.start"]', '25');
    await delay(200);
    await evalJs(`document.querySelector('[data-setting="batch.preview"]').click()`);
    const errText = await waitFor(
      () => evalJs(`document.querySelector('[data-batch-error]')?.textContent ?? ''`),
      5_000,
      '错误提示',
    );
    check('B11 圆圈数字超限提示', typeof errText === 'string' && errText.length > 0, errText.slice(0, 60));

    currentStep = 'B11b Esc 关闭';
    await client.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape' });
    await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape' });
    await waitFor(() => evalJs(`!document.querySelector('[data-batch-dialog]')`), 5_000, 'Esc 关闭');
    check('B11b Esc 关闭弹窗', true);

    currentStep = 'B12 错误码 BATCH_INVALID';
    const invalid = await evalJs(
      `(async () => {
        try {
          const v = await window.__TAURI_INTERNALS__.invoke('list_tabs');
          await window.__TAURI_INTERNALS__.invoke('apply_batch_numbering', {
            tabId: v.activeTabId,
            config: { format: 'circled', start: 25, step: 1, zeroPadWidth: 2, separator: '.', suffix: '', position: 'lineStart', scope: { kind: 'all' }, skipEmpty: true, template: null, previewLines: 10 },
          });
          return 'OK';
        } catch (error) {
          return JSON.stringify(error);
        }
      })()`,
    );
    check('B12 IPC 错误码', typeof invalid === 'string' && invalid.includes('BATCH_INVALID'), invalid.slice(0, 80));

    currentStep = 'B14 大范围应用（进度事件）';
    await evalJs(
      `(async () => { await window.__srt.openPath(${JSON.stringify(largeFile)}); return true; })()`,
    );
    const largeTab = await waitFor(async () => {
      const tab = await activeTab();
      return tab && tab.name === 'batch-large.txt' ? tab : null;
    }, 10_000, '大文件标签');
    const largeTabId = largeTab.tabId;
    if (!(await activeTab())?.editing) {
      await evalJs(`(document.querySelector('[aria-label="切换编辑模式"]')?.click(), true)`);
      await waitFor(async () => ((await activeTab())?.editing ? true : null), 8_000, '大文件编辑态');
    }
    await waitFor(() => evalJs(`!!document.querySelector('textarea.input-proxy')`), 8_000, '大文件编辑层');
    await clickByText('编辑');
    await delay(200);
    await clickByText('批量插入');
    await waitFor(() => evalJs(`!!document.querySelector('[data-batch-dialog]')`), 5_000, '批量弹窗');
    await setControl('[data-setting="batch.scope"]', 'all');
    await setControl('[data-setting="batch.format"]', 'arabic');
    await delay(150);
    await evalJs(`document.querySelector('[data-setting="batch.apply"]').click()`);
    let sawProgress = false;
    try {
      await waitFor(() => evalJs(`!!document.querySelector('[data-batch-progress]')`), 6_000, '进度元素');
      sawProgress = true;
    } catch {
      // 进度未出现：由 B14a 断言报失败
    }
    await waitFor(() => evalJs(`!document.querySelector('[data-batch-dialog]')`), 60_000, '批量完成关闭');
    const rowsLarge = await evalJs(
      `window.__TAURI_INTERNALS__.invoke('get_rows', { tabId: ${largeTabId}, startRow: 0, count: 1 })`,
    );
    const rowsLargeTail = await evalJs(
      `window.__TAURI_INTERNALS__.invoke('get_rows', { tabId: ${largeTabId}, startRow: 7999, count: 1 })`,
    );
    check('B14a 大范围应用显示进度元素', sawProgress === true);
    check(
      'B14b 8000 行全部编号',
      (rowsLarge?.rows?.[0]?.text ?? '').startsWith('1.x0') &&
        (rowsLargeTail?.rows?.[0]?.text ?? '').startsWith('8000.x7999'),
      `${rowsLarge?.rows?.[0]?.text} | ${rowsLargeTail?.rows?.[0]?.text}`,
    );
    await evalJs(`window.__TAURI_INTERNALS__.invoke('undo_edit', { tabId: ${largeTabId} })`);
    await delay(400);
    const restored = await evalJs(
      `window.__TAURI_INTERNALS__.invoke('get_rows', { tabId: ${largeTabId}, startRow: 0, count: 1 })`,
    );
    check(
      'B14c 撤销恢复原状',
      (restored?.rows?.[0]?.text ?? '') === 'x0' && (await activeTab())?.dirty === false,
    );

    currentStep = 'B15 运行中取消';
    await clickByText('编辑');
    await delay(200);
    await clickByText('批量插入');
    await waitFor(() => evalJs(`!!document.querySelector('[data-batch-dialog]')`), 5_000, '批量弹窗');
    await setControl('[data-setting="batch.scope"]', 'all');
    await delay(150);
    await evalJs(`document.querySelector('[data-setting="batch.apply"]').click()`);
    await waitFor(() => evalJs(`!!document.querySelector('[data-batch-cancel]')`), 6_000, '取消按钮');
    await evalJs(`document.querySelector('[data-batch-cancel]').click()`);
    await waitFor(() => evalJs(`!document.querySelector('[data-batch-cancel]')`), 30_000, '取消完成');
    const rowsCancelled = await evalJs(
      `window.__TAURI_INTERNALS__.invoke('get_rows', { tabId: ${largeTabId}, startRow: 0, count: 1 })`,
    );
    const undoAfterCancel = await evalJs(
      `window.__TAURI_INTERNALS__.invoke('undo_edit', { tabId: ${largeTabId} })`,
    );
    check(
      'B15a 取消后文档不变',
      (rowsCancelled?.rows?.[0]?.text ?? '') === 'x0' && (await activeTab())?.dirty === false,
      JSON.stringify(rowsCancelled?.rows?.[0]?.text),
    );
    check('B15b 取消无撤销步骤', undoAfterCancel === null, JSON.stringify(undoAfterCancel));
    const dialogStillOpen = await evalJs(`!!document.querySelector('[data-batch-dialog]')`);
    check('B15c 取消后弹窗保持打开', dialogStillOpen === true);
    await evalJs(`document.querySelector('[data-setting="batch.close"]').click()`);
    await delay(300);

    currentStep = 'B13 截图';
    const shot = await client.send('Page.captureScreenshot', { format: 'png' });
    mkdirSync(dirname(screenshotPath), { recursive: true });
    writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'));
    check('B13 截图保存', existsSync(screenshotPath), screenshotPath);
  } finally {
    clearTimeout(watchdog);
    try {
      if (child.pid) {
        spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
      }
    } catch {
      // 忽略清理失败
    }
    await removeWithRetry(workDir);
  }

  const failed = checks.filter((item) => !item.passed);
  console.log(`\n结果：${checks.length - failed.length}/${checks.length} 通过`);
  if (failed.length > 0) {
    for (const item of failed) console.log(`FAILED: ${item.name} ${item.detail}`);
    process.exitCode = 1;
  }
}

main().catch((error) => {
  clearTimeout(watchdog);
  console.error(`未捕获错误（${currentStep}）：${error.message}`);
  process.exitCode = 1;
});
