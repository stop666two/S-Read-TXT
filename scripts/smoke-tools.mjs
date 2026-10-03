#!/usr/bin/env node
// P1-7 辅助编辑 E2E（常驻套件）：时间戳插入 / 括号自动补对与配对高亮 / 回车自动缩进 /
// 清理子菜单（单项与一键）/ 设置页新分组可见性 / 截图。
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-tools.mjs [--exe <路径>] [--screenshot <路径>]

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
  join(root, 'docs', 'screenshots', 'phase-p1-tools.png'),
);
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '300000');

const workDir = join(tmpdir(), `srt-tools-${Date.now()}`);
const dataDir = join(workDir, 'data');
const testFile = join(workDir, 'tools-sample.txt');
const port = 9600 + Math.floor(Math.random() * 200);

const checks = [];
let currentStep = 'T0 启动';
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
  // 5 行：首行含括号与行首缩进；第 2 行含行尾空白；第 3/4 行为重复空行；末行无末尾换行。
  writeFileSync(testFile, '  hello(world)\ntrail   \n\n\ntail', 'utf8');

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
    currentStep = 'T0 等待 CDP';
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
    const pressKey = async (key, code, vk, modifiers = 0) => {
      await client.send('Input.dispatchKeyEvent', {
        type: 'rawKeyDown',
        key,
        code,
        windowsVirtualKeyCode: vk,
        nativeVirtualKeyCode: vk,
        modifiers,
      });
      await client.send('Input.dispatchKeyEvent', {
        type: 'keyUp',
        key,
        code,
        windowsVirtualKeyCode: vk,
        nativeVirtualKeyCode: vk,
        modifiers,
      });
    };
    const insertText = async (text) => {
      await client.send('Input.insertText', { text });
    };
    const clickByText = async (text) =>
      evalJs(
        `(() => {
          const nodes = document.querySelectorAll('button, .item, [role="menuitem"]');
          for (const node of nodes) {
            const label = node.textContent?.replace(/[▸›]/g, '').trim();
            if (label === ${JSON.stringify(text)}) {
              node.click();
              return true;
            }
          }
          return false;
        })()`,
      );
    const openMenu = async (label) => {
      await clickByText(label);
      await delay(200);
    };
    /** 打开「编辑 → 清理」子菜单（展开态可能残留在 toggle 上，失败时重试一次）。 */
    const openCleanupMenu = async () => {
      await openMenu('编辑');
      for (let i = 0; i < 2; i += 1) {
        await clickByText('清理');
        await delay(200);
        const has = await evalJs(
          `Boolean([...document.querySelectorAll('button')].find((b) => b.textContent?.trim() === '删除行尾空白'))`,
        );
        if (has) return true;
      }
      return false;
    };
    const rowText = async (row) =>
      evalJs(`document.querySelector('.row[data-row="${row}"]')?.textContent ?? null`);
    const rowsTotal = async () =>
      evalJs(
        `window.__TAURI_INTERNALS__.invoke('list_tabs').then((view) => {
          const active = view.tabs.find((tab) => tab.tabId === view.activeTabId);
          return active?.rowsTotal ?? -1;
        })`,
      );
    const focusEditor = async () => {
      await evalJs(`document.querySelector('textarea.input-proxy')?.focus() ?? null`);
      await delay(80);
    };

    await dismissOnboarding(evalJs);
    await waitFor(() => evalJs(`Boolean(window.__srt?.openPath)`), 15_000, '自动化钩子');
    await evalJs(`(void window.__srt.openPath(${JSON.stringify(testFile)}), true)`);
    currentStep = 'T1 打开并进入编辑';
    await waitFor(async () => (await rowsTotal()) === 5, 15_000, '文件打开（5 行）');
    {
      let toggleClicks = 0;
      let lastClickAt = 0;
      await waitFor(
        async () => {
          if (await evalJs(`Boolean(document.querySelector('textarea.input-proxy'))`)) return true;
          const now = Date.now();
          if (toggleClicks < 3 && (toggleClicks === 0 || now - lastClickAt > 2000)) {
            await evalJs(`document.querySelector('[aria-label="切换编辑模式"]')?.click() ?? null`);
            toggleClicks += 1;
            lastClickAt = now;
          }
          return false;
        },
        15_000,
        '编辑层就绪',
      );
    }
    await focusEditor();
    check('T1 打开文件并进入编辑模式', true);

    // T2 插入日期时间（默认本地格式）→ 撤销还原
    currentStep = 'T2 时间戳插入';
    await pressKey('Home', 'Home', 36, 2);
    await openMenu('编辑');
    const inserted = await clickByText('插入日期时间');
    await delay(350);
    const line0 = await rowText(0);
    check(
      'T2a 菜单插入日期时间',
      inserted === true && /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} {2}hello\(world\)$/.test(line0 ?? ''),
      `line0=${line0}`,
    );
    await focusEditor();
    await pressKey('z', 'KeyZ', 90, 2);
    await delay(300);
    check('T2b 撤销还原首行', (await rowText(0)) === '  hello(world)');

    // T3 自动补对：输入 ( 补全 ()；跨过右符号；空对退格删除
    currentStep = 'T3 自动补对';
    await focusEditor();
    await insertText('(');
    await delay(300);
    check('T3a 输入左括号自动补对', (await rowText(0)) === '()  hello(world)', `line0=${await rowText(0)}`);
    await insertText(')');
    await delay(250);
    check('T3b 输入右符号跳过（不重复插入）', (await rowText(0)) === '()  hello(world)');
    await pressKey('ArrowLeft', 'ArrowLeft', 37, 0);
    await delay(250);
    await pressKey('Backspace', 'Backspace', 8, 0);
    await delay(450);
    check(
      'T3c 空对退格整体删除',
      (await rowText(0)) === '  hello(world)',
      `line0=${await rowText(0)} toasts=${await evalJs(`[...document.querySelectorAll('.toast, [role="status"]')].map((n) => n.textContent).join('|')`)}`,
    );

    // T4 回车自动缩进：行尾回车继承行首空白 → 撤销
    currentStep = 'T4 自动缩进';
    await focusEditor();
    await pressKey('Home', 'Home', 36, 2);
    await pressKey('End', 'End', 35, 0);
    await pressKey('Enter', 'Enter', 13, 0);
    await delay(350);
    check('T4a 回车继承行首缩进', (await rowText(1)) === '  ', `row1=${await rowText(1)}`);
    check('T4b 行数 +1', (await rowsTotal()) === 6);
    await pressKey('z', 'KeyZ', 90, 2);
    await delay(300);
    check('T4c 撤销还原行数', (await rowsTotal()) === 5);

    // T5 括号配对高亮：光标置于 ( 之后 → 两个高亮盒
    currentStep = 'T5 括号配对高亮';
    await focusEditor();
    await pressKey('Home', 'Home', 36, 2);
    for (let i = 0; i < 8; i += 1) await pressKey('ArrowRight', 'ArrowRight', 39, 0);
    const bracketBoxes = await waitFor(
      () => evalJs(`document.querySelectorAll('.bracket-match').length`),
      5_000,
      '括号配对高亮',
    );
    check('T5 括号配对高亮两个字符盒', bracketBoxes === 2, `boxes=${bracketBoxes}`);

    // T6 清理：单项删除行尾空白 → 撤销
    currentStep = 'T6 单项清理';
    await openCleanupMenu();
    const trimmed = await clickByText('删除行尾空白');
    await delay(400);
    check(
      'T6a 删除行尾空白',
      trimmed === true && (await rowText(1)) === 'trail',
      `clicked=${trimmed} row1=${await rowText(1)} toasts=${await evalJs(`[...document.querySelectorAll('.toast, [role="status"]')].map((n) => n.textContent).join('|')`)}`,
    );
    await focusEditor();
    await pressKey('z', 'KeyZ', 90, 2);
    await delay(300);
    check('T6b 撤销还原行尾空白', (await rowText(1)) === 'trail   ');

    // T7 一键清理：合并重复空行 + 统一末尾换行 + 行尾空白（默认全选）
    currentStep = 'T7 一键清理';
    await openCleanupMenu();
    const cleanedAll = await clickByText('一键清理');
    await delay(500);
    check(
      'T7a 一键清理执行（行数 5→4，行尾空白已删）',
      cleanedAll === true && (await rowsTotal()) === 4 && (await rowText(1)) === 'trail',
      `rows=${await rowsTotal()} row1=${await rowText(1)}`,
    );
    let toastShown = false;
    try {
      toastShown = await waitFor(
        () => evalJs(`document.body.textContent?.includes('清理完成') ?? false`),
        4_000,
        '清理完成 toast',
      );
    } catch {
      toastShown = false;
    }
    check('T7b 清理完成提示', toastShown === true);

    // T8 设置页新分组可见（编辑器页：时间戳格式 + 清理开关）
    currentStep = 'T8 设置页分组';
    await openMenu('文件');
    await clickByText('设置…');
    const settingsWs = await waitFor(async () => {
      try {
        return await findTarget(port, 'settings.html');
      } catch {
        return null;
      }
    }, 10_000, '设置窗 CDP 目标');
    const settings = await createClient(settingsWs);
    const evalSettings = async (expression) => {
      const result = await settings.send('Runtime.evaluate', {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
      if (result.exceptionDetails) throw new Error(`设置窗执行异常：${result.exceptionDetails.text}`);
      return result.result?.value;
    };
    await settings.send('Page.bringToFront');
    const tabClicked = await evalSettings(
      `(() => {
        const node = [...document.querySelectorAll('.tabs [role="tab"], .tabs .tab')]
          .find((el) => el.textContent?.trim() === '编辑器');
        node?.click();
        return Boolean(node);
      })()`,
    );
    await delay(400);
    const groupsOk = await evalSettings(
      `(() => {
        const probe = (id) => Boolean(document.querySelector('[data-setting="' + id + '"]'));
        const expandAll = () => {
          for (const head of document.querySelectorAll('.group-head')) head.click();
        };
        let ok = probe('app.editor.insert.timestampFormat') && probe('app.editor.cleanup.trailingNewline');
        if (!ok) {
          expandAll();
          ok = probe('app.editor.insert.timestampFormat') && probe('app.editor.cleanup.trailingNewline');
        }
        return ok;
      })()`,
    );
    check('T8 设置页含时间戳与清理分组', tabClicked === true && groupsOk === true);
    // 关闭设置窗（点击取消 await——窗口销毁后响应不会到达）
    evalSettings(
      `document.querySelector('[aria-label="关闭"]')?.click() ?? window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'settings' }), true`,
    ).catch(() => {});
    await delay(400);

    if (screenshotPath) {
      currentStep = 'T9 截图';
      const shot = await client.send('Page.captureScreenshot', { format: 'png' });
      mkdirSync(dirname(screenshotPath), { recursive: true });
      writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'));
      check('T9 截图已保存', true, screenshotPath);
    }
  } finally {
    clearTimeout(watchdog);
    try {
      client?.close();
    } catch {
      // 忽略
    }
    try {
      if (process.platform === 'win32') {
        spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
      } else {
        child.kill('SIGKILL');
      }
    } catch {
      // 忽略
    }
    await delay(500);
    await removeWithRetry(workDir);
  }

  const failed = checks.filter((item) => !item.passed);
  console.log(`\n结果：${checks.length - failed.length}/${checks.length} 通过`);
  if (failed.length > 0) {
    process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(`执行失败：${error?.stack ?? error}`);
  process.exitCode = 3;
});
