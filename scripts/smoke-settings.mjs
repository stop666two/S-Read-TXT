#!/usr/bin/env node
// 设置窗口 E2E（阶段 5b）：工具栏打开设置 → 快捷键录制/冲突/保留键/取消/单条与全部恢复默认
// → 自定义组合在主窗口生效 → 恢复默认复原。
//
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-settings.mjs [--exe <路径>] [--port 9228]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, findTarget, waitForValue } from './lib/smoke-cdp.mjs';
import { createDialogOps } from './lib/dialog.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const port = Number(argValue('--port', String(9200 + Math.floor(Math.random() * 600))));
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '240000');

const workDir = join(process.env.TEMP ?? '.', 'srt-smoke-settings');
const firstFile = join(workDir, 'settings-a.txt');
const secondFile = join(workDir, 'settings-b.txt');
const runDataDir = join(workDir, `data-${Date.now()}`);
const shortcutsFile = join(runDataDir, 'shortcuts.json');

const checks = [];
let currentStep = 'S0 启动';
function check(name, passed, detail = '') {
  checks.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

const watchdog = setTimeout(() => {
  console.error(`看门狗超时（${watchdogMs}ms），最后步骤：${currentStep}`);
  process.exit(4);
}, watchdogMs);

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

/** 读取 shortcuts.json 覆盖表（文件缺失返回 {}）。 */
function readOverrides() {
  if (!existsSync(shortcutsFile)) return {};
  try {
    return JSON.parse(readFileSync(shortcutsFile, 'utf8')).bindings ?? {};
  } catch {
    return {};
  }
}

async function main() {
  if (!existsSync(exePath)) {
    console.error(`可执行文件不存在：${exePath}（先运行 npm run tauri build -- --debug --no-bundle）`);
    process.exit(2);
  }
  mkdirSync(workDir, { recursive: true });
  writeFileSync(firstFile, 'first-line\n'.repeat(30), 'utf8');
  writeFileSync(secondFile, 'second\n', 'utf8');

  let child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: runDataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });

  let mainClient;
  let settingsClient;
  try {
    mainClient = await createClient(await findTarget(port));
    const evalMain = (expression) => evalIn(mainClient, expression);
    await waitForValue(async () => {
      const ready = await evalMain(
        '(() => !!(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke && window.__srt && window.__srt.openPath))()',
      );
      return ready ? true : null;
    }, 30000);

    /** CDP 键盘注入（modifiers：Ctrl=2 Shift=8） */
    const pressOn = async (client, key, code, vk, modifiers = 0) => {
      const base = { key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
      await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...base });
      await client.send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
      await delay(160);
    };

    /** 原生对话框探测/关闭（懒创建：S12 重启应用后 PID 变化，闭包内读当前 child.pid） */
    const dialogOp = (action) => createDialogOps(child.pid).dialogOp(action);
    const waitDialog = (expectFound, timeoutMs) => createDialogOps(child.pid).waitDialog(expectFound, timeoutMs);

    /** 关闭首启引导（模态遮罩会挂起全局快捷键；勾选「不再显示」后点「开始使用」） */
    const dismissOnboarding = async () => {
      await waitForValue(async () => {
        const present = await evalMain(
          `(() => { const overlay = document.querySelector('.overlay[aria-label="使用向导"]');
            if (!overlay) return false;
            const check = overlay.querySelector('.dont-show input');
            if (check && !check.checked) check.click();
            const button = [...overlay.querySelectorAll('button')].find((b) => b.textContent.includes('开始使用'));
            button?.click();
            return true; })()`,
        );
        return present ? true : null;
      }, 10000);
    };
    await dismissOnboarding();

    // S1 打开设置窗口（工具栏「设置」按钮）
    currentStep = 'S1 打开设置窗口';
    await evalMain(`document.querySelector('button[title="设置"]')?.click() ?? true`);
    settingsClient = await createClient(await findTarget(port, 'settings.html'));
    const evalSet = (expression) => evalIn(settingsClient, expression);
    /** 按名称切换设置页签（默认页签为「常规」，快捷键操作前必须先切换） */
    const clickTab = (name) =>
      evalSet(
        `(() => { const tab = [...document.querySelectorAll('.tabs [role="tab"]')].find((b) => b.textContent.trim() === '${name}'); tab?.click(); return !!tab; })()`,
      );
    await clickTab('快捷键');
    const rowsReady = await waitForValue(
      async () => ((await evalSet(`document.querySelectorAll('.row').length`)) === 15 ? true : null),
      15000,
    );
    check('S1a 设置窗口打开且快捷键行完整', rowsReady === true);
    const tabCount = await evalSet(`document.querySelectorAll('.tabs [role="tab"]').length`);
    check('S1b 设置窗口五个页签', tabCount === 5, `count=${tabCount}`);
    if (process.argv.includes('--screenshot')) {
      const shot = await settingsClient.send('Page.captureScreenshot', { format: 'png' });
      writeFileSync(resolve(root, 'docs/screenshots/phase5-settings.png'), Buffer.from(shot.data, 'base64'));
    }

    /** 依标签点击组合键按钮，返回是否找到 */
    const clickCombo = (label) =>
      evalSet(
        `(() => { const row = [...document.querySelectorAll('.row')].find((r) => r.querySelector('.label')?.textContent?.trim() === '${label}'); row?.querySelector('.combo')?.click(); return !!row; })()`,
      );
    /** 读某行组合键文本 */
    const comboText = (label) =>
      evalSet(
        `(() => { const row = [...document.querySelectorAll('.row')].find((r) => r.querySelector('.label')?.textContent?.trim() === '${label}'); return row?.querySelector('.combo')?.textContent?.trim() ?? ''; })()`,
      );
    /** 读最新 Toast 文本（无则空串） */
    const toastText = () =>
      evalSet(`document.querySelector('.toast:last-of-type .text')?.textContent?.trim() ?? ''`);

    // S2 录制 closeTab → Ctrl+Q
    currentStep = 'S2 录制 Ctrl+Q';
    await clickCombo('关闭当前标签');
    await pressOn(settingsClient, 'q', 'KeyQ', 81, 2);
    const savedText = await waitForValue(async () => {
      const text = await comboText('关闭当前标签');
      return text === 'Ctrl+Q' ? text : null;
    }, 6000);
    check('S2a 录制后组合键更新为 Ctrl+Q', savedText === 'Ctrl+Q');
    await waitForValue(async () => ((await toastText()) === '快捷键已保存' ? true : null), 6000);
    check('S2b 保存提示出现', (await toastText()) === '快捷键已保存');
    const overridesAfterRecord = readOverrides();
    check(
      'S2c 覆盖表已落盘（仅含 closeTab）',
      overridesAfterRecord.closeTab === 'Ctrl+Q' && Object.keys(overridesAfterRecord).length === 1,
      JSON.stringify(overridesAfterRecord),
    );

    // S3 冲突检测：打开文件 录制 Ctrl+Q（已占用）
    currentStep = 'S3 冲突检测';
    await clickCombo('打开文件');
    await pressOn(settingsClient, 'q', 'KeyQ', 81, 2);
    await delay(400);
    check('S3a 冲突提示出现', (await toastText()) === '该组合已被其他动作使用', await toastText());
    check('S3b 冲突后仍在录制态', (await comboText('打开文件')).includes('按下新组合'));

    // S4 保留键：Ctrl+1
    currentStep = 'S4 保留键检测';
    await pressOn(settingsClient, '1', 'Digit1', 49, 2);
    await delay(400);
    check('S4 保留键提示出现', (await toastText()).includes('Ctrl+1~9'), await toastText());

    // S5 Esc 取消录制
    currentStep = 'S5 取消录制';
    await pressOn(settingsClient, 'Escape', 'Escape', 27);
    await delay(300);
    check('S5 取消后恢复显示原绑定', (await comboText('打开文件')) === 'Ctrl+O');

    // S6 单条恢复默认
    currentStep = 'S6 单条恢复默认';
    await evalSet(
      `(() => { const row = [...document.querySelectorAll('.row')].find((r) => r.querySelector('.label')?.textContent?.trim() === '关闭当前标签'); row?.querySelector('.reset')?.click(); return true; })()`,
    );
    const restored = await waitForValue(async () => {
      const text = await comboText('关闭当前标签');
      return text === 'Ctrl+W' ? text : null;
    }, 6000);
    check('S6a 单条恢复默认后组合键回到 Ctrl+W', restored === 'Ctrl+W');
    check('S6b 覆盖表清空', Object.keys(readOverrides()).length === 0, JSON.stringify(readOverrides()));

    // S7 自定义并全部恢复默认
    currentStep = 'S7 全部恢复默认';
    await clickCombo('关闭当前标签');
    await pressOn(settingsClient, 'q', 'KeyQ', 81, 2);
    await waitForValue(async () => ((await comboText('关闭当前标签')) === 'Ctrl+Q' ? true : null), 6000);
    await evalSet(`document.querySelector('.reset-all')?.click() ?? true`);
    const resetAll = await waitForValue(async () => {
      const text = await comboText('关闭当前标签');
      return text === 'Ctrl+W' ? text : null;
    }, 6000);
    check('S7a 全部恢复默认生效', resetAll === 'Ctrl+W');
    check('S7b 覆盖表为空', Object.keys(readOverrides()).length === 0, JSON.stringify(readOverrides()));

    // S8 自定义在主窗口生效：设置 Ctrl+Q 关闭 → 关设置窗 → 主窗口 Ctrl+Q 关闭标签、Ctrl+W 无效
    currentStep = 'S8 自定义在主窗口生效';
    await clickCombo('关闭当前标签');
    await pressOn(settingsClient, 'q', 'KeyQ', 81, 2);
    await waitForValue(async () => ((await comboText('关闭当前标签')) === 'Ctrl+Q' ? true : null), 6000);
    await evalSet(`document.querySelector('.title-bar button[aria-label="关闭"]')?.click() ?? true`);
    await delay(800);
    // 注意：openPath 的原生 Promise 不交给 CDP await（WebView2 下会偶发 “Promise was collected”），
    // 改为页面内触发 + 轮询后端状态确认（与 smoke-session 同一处理）。
    await evalMain(`(void window.__srt.openPath(${JSON.stringify(firstFile)}), true)`);
    await delay(450);
    await evalMain(`(void window.__srt.openPath(${JSON.stringify(secondFile)}), true)`);
    await delay(450);
    const countAll = () =>
      evalMain(`(async () => (await window.__TAURI_INTERNALS__.invoke('list_tabs')).tabs.length)()`);
    const twoTabsS8 = await waitForValue(async () => ((await countAll()) === 2 ? true : null), 8000);
    check('S8a 主窗口两个标签', twoTabsS8 === true);
    await pressOn(mainClient, 'q', 'KeyQ', 81, 2);
    const afterQ = await waitForValue(async () => ((await countAll()) === 1 ? 1 : null), 6000);
    check('S8b 自定义 Ctrl+Q 关闭标签生效', afterQ === 1);
    await pressOn(mainClient, 'w', 'KeyW', 87, 2);
    await delay(500);
    check('S8c 旧键 Ctrl+W 已解绑（不再关闭）', (await countAll()) === 1);

    // S9 恢复默认后旧键复原
    currentStep = 'S9 恢复默认后旧键复原';
    await evalMain(`document.querySelector('button[title="设置"]')?.click() ?? true`);
    await delay(900);
    settingsClient?.close?.();
    settingsClient = await createClient(await findTarget(port, 'settings.html'));
    await clickTab('快捷键');
    await waitForValue(async () => ((await evalSet(`document.querySelectorAll('.row').length`)) === 15 ? true : null), 15000);
    await evalSet(`document.querySelector('.reset-all')?.click() ?? true`);
    await waitForValue(async () => ((await comboText('关闭当前标签')) === 'Ctrl+W' ? true : null), 6000);
    await evalSet(`document.querySelector('.title-bar button[aria-label="关闭"]')?.click() ?? true`);
    await delay(800);
    await pressOn(mainClient, 'w', 'KeyW', 87, 2);
    const afterW = await waitForValue(async () => ((await countAll()) === 0 ? 0 : null), 6000);
    check('S9 恢复默认后 Ctrl+W 重新生效', afterW === 0);

    // ---- S10/S11/S12/S13：扩展场景 ----

    /** 打开设置窗口并等待列表就绪（复用同一 CDP 目标） */
    const waitRows = () =>
      waitForValue(async () => ((await evalSet(`document.querySelectorAll('.row').length`)) === 15 ? true : null), 15000);
    const reopenSettings = async () => {
      await evalMain(`document.querySelector('button[title="设置"]')?.click() ?? true`);
      settingsClient?.close?.();
      settingsClient = await createClient(await findTarget(port, 'settings.html'));
      await clickTab('快捷键');
      return waitRows();
    };
    const closeSettings = async () => {
      await evalSet(`document.querySelector('.title-bar button[aria-label="关闭"]')?.click() ?? true`);
      await delay(800);
    };

    // S10 裸字母被拒绝（录制校验）
    currentStep = 'S10 裸字母键校验';
    await reopenSettings();
    await clickCombo('打开文件');
    await pressOn(settingsClient, 'a', 'KeyA', 65, 0);
    await delay(400);
    check('S10a 裸字母被拒绝并提示', (await toastText()).includes('干扰'), await toastText());
    check('S10b 拒绝后仍在录制态', (await comboText('打开文件')).includes('按下新组合'));

    // S11 功能键可单键：录制 F5 → 主窗生效（原生对话框）→ 单条恢复默认
    currentStep = 'S11 功能键单键绑定';
    await pressOn(settingsClient, 'F5', 'F5', 116, 0);
    await waitForValue(async () => ((await comboText('打开文件')) === 'F5' ? true : null), 6000);
    check('S11a F5 单键录制成功', (await comboText('打开文件')) === 'F5');
    await closeSettings();
    currentStep = 'S11b F5 主窗生效';
    await pressOn(mainClient, 'F5', 'F5', 116, 0);
    const f5Dialog = await waitDialog(true);
    check('S11b 主窗口 F5 打开对话框', f5Dialog === true);
    if (f5Dialog) dialogOp('close');
    await waitDialog(false);
    await reopenSettings();
    await evalSet(
      `(() => { const row = [...document.querySelectorAll('.row')].find((r) => r.querySelector('.label')?.textContent?.trim() === '打开文件'); row?.querySelector('.reset')?.click(); return true; })()`,
    );
    const openRestored = await waitForValue(async () => ((await comboText('打开文件')) === 'Ctrl+O' ? true : null), 6000);
    check('S11c 打开文件单条恢复默认', openRestored === true);
    await closeSettings();

    // S12 持久化跨重启：自定义绑定在应用重启后仍生效
    currentStep = 'S12 跨重启持久化';
    await reopenSettings();
    await clickCombo('关闭当前标签');
    await pressOn(settingsClient, 'q', 'KeyQ', 81, 2);
    await waitForValue(async () => ((await comboText('关闭当前标签')) === 'Ctrl+Q' ? true : null), 6000);
    await closeSettings();
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    await delay(1200);
    child = spawn(exePath, [], {
      env: {
        ...process.env,
        SRT_DATA_DIR: runDataDir,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
      },
      stdio: 'ignore',
    });
    mainClient?.close?.();
    mainClient = await createClient(await findTarget(port));
    await waitForValue(async () => {
      const ready = await evalMain(
        '(() => !!(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke && window.__srt && window.__srt.openPath))()',
      );
      return ready ? true : null;
    }, 30000);
    await evalMain(`(void window.__srt.openPath(${JSON.stringify(firstFile)}), true)`);
    await delay(450);
    await evalMain(`(void window.__srt.openPath(${JSON.stringify(secondFile)}), true)`);
    await delay(450);
    const twoTabsS12 = await waitForValue(async () => ((await countAll()) === 2 ? true : null), 8000);
    check('S12a 重启后两个标签', twoTabsS12 === true);
    await pressOn(mainClient, 'q', 'KeyQ', 81, 2);
    const afterRestartQ = await waitForValue(async () => ((await countAll()) === 1 ? 1 : null), 6000);
    check('S12b 重启后自定义 Ctrl+Q 仍生效', afterRestartQ === 1);

    // S13 设置页签：常规真实字段 / 快捷键行完整 / 排版实时应用到主窗口
    currentStep = 'S13 页签与实时应用';
    await reopenSettings();
    await evalSet(
      `(() => { const tab = [...document.querySelectorAll('.tabs [role="tab"]')].find((b) => b.textContent.trim() === '常规'); tab?.click(); return true; })()`,
    );
    await delay(300);
    const generalHasField = await evalSet(
      `(() => { const labels = [...document.querySelectorAll('.rows .label')].map((el) => el.textContent ?? ''); return labels.some((t) => t.includes('可打开文件大小上限')); })()`,
    );
    check('S13a 常规页签显示真实字段', generalHasField === true);
    await evalSet(
      `(() => { const tab = [...document.querySelectorAll('.tabs [role="tab"]')].find((b) => b.textContent.trim() === '快捷键'); tab?.click(); return true; })()`,
    );
    await delay(300);
    check('S13b 切回快捷键页签行完整', (await evalSet(`document.querySelectorAll('.row').length`)) === 15);

    // S13c 阅读排版：改字号 → 主窗口 CSS 变量实时生效（并还原）
    currentStep = 'S13c 排版实时应用';
    await evalSet(
      `(() => { const tab = [...document.querySelectorAll('.tabs [role="tab"]')].find((b) => b.textContent.trim() === '阅读排版'); tab?.click(); return true; })()`,
    );
    await delay(300);
    const setFontSize = async (size) => {
      await evalSet(
        `(() => {
          const range = document.querySelector('.rows input[type="range"]');
          if (!range) return false;
          const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
          setter.call(range, '${size}');
          range.dispatchEvent(new Event('input', { bubbles: true }));
          range.dispatchEvent(new Event('change', { bubbles: true }));
          return true;
        })()`,
      );
      // 注意：此处必须用模板字符串（`${size}px`）比较；单引号会变成字面量导致永不匹配
      return waitForValue(async () => {
        const value = await evalMain(
          `getComputedStyle(document.documentElement).getPropertyValue('--reading-size').trim()`,
        );
        return value === `${size}px` ? true : null;
      }, 8000);
    };
    check('S13c 字号修改实时应用到主窗口', (await setFontSize(20)) === true);
    await setFontSize(16);

    // 汇总
    const failed = checks.filter((item) => !item.passed);
    console.log(`\n设置窗口冒烟：${checks.length - failed.length}/${checks.length} 通过`);
    if (failed.length > 0) {
      console.error(`失败项：${failed.map((item) => item.name).join('；')}`);
      process.exitCode = 1;
    }
  } catch (error) {
    console.error(`设置窗口冒烟异常（步骤：${currentStep}）：${error?.message ?? error}`);
    process.exitCode = 1;
  } finally {
    clearTimeout(watchdog);
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    await delay(500);
    await removeWithRetry(runDataDir);
    await removeWithRetry(workDir);
    mainClient?.close?.();
    settingsClient?.close?.();
  }
}

/** 在指定 CDP 客户端求值（Promise/返回体直出） */
async function evalIn(client, expression) {
  const result = await client.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(`页面脚本异常：${result.exceptionDetails.text}`);
  return result.result?.value;
}

void main();
