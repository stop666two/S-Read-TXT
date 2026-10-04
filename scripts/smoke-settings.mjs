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
    // 链式全量自检下机器高负载时，首次进入可能渲染滞后；三重兼容：延长等待 → 重切页签 → 整页重载
    let rowsReady = await waitForValue(
      async () => ((await evalSet(`document.querySelectorAll('.row').length`)) === 15 ? true : null),
      15000,
    );
    if (rowsReady !== true) {
      await clickTab('快捷键');
      rowsReady = await waitForValue(
        async () => ((await evalSet(`document.querySelectorAll('.row').length`)) === 15 ? true : null),
        30000,
      );
    }
    if (rowsReady !== true) {
      await evalSet('location.reload(); true');
      await delay(1500);
      await clickTab('快捷键');
      rowsReady = await waitForValue(
        async () => ((await evalSet(`document.querySelectorAll('.row').length`)) === 15 ? true : null),
        30000,
      );
    }
    check('S1a 设置窗口打开且快捷键行完整', rowsReady === true);
    const tabCount = await evalSet(`document.querySelectorAll('.tabs [role="tab"]').length`);
    check('S1b 设置窗口六个页签', tabCount === 6, `count=${tabCount}`);
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
    /** 进入录制态（点击组合键并确认已进入录制；首击偶发丢失时重试一次） */
    const startRecording = async (label) => {
      for (let attempt = 0; attempt < 2; attempt += 1) {
        await clickCombo(label);
        const active = await waitForValue(
          async () => ((await comboText(label)).includes('按下新组合') ? true : null),
          2000,
        );
        if (active) return true;
      }
      return false;
    };
    /** 读最新 Toast 文本（无则空串） */
    const toastText = () =>
      evalSet(`document.querySelector('.toast:last-of-type .text')?.textContent?.trim() ?? ''`);

    // S2 录制 closeTab → Ctrl+Q
    currentStep = 'S2 录制 Ctrl+Q';
    await startRecording('关闭当前标签');
    await pressOn(settingsClient, 'q', 'KeyQ', 81, 2);
    const savedText = await waitForValue(async () => {
      const text = await comboText('关闭当前标签');
      return text === 'Ctrl+Q' ? text : null;
    }, 6000);
    check('S2a 录制后组合键更新为 Ctrl+Q', savedText === 'Ctrl+Q', `text=${savedText ?? ''}`);
    await waitForValue(async () => ((await toastText()) === '快捷键已保存' ? true : null), 6000);
    const saveToast = await toastText();
    check('S2b 保存提示出现', saveToast === '快捷键已保存', `toast=${saveToast}`);
    const overridesAfterRecord = readOverrides();
    check(
      'S2c 覆盖表已落盘（仅含 closeTab）',
      overridesAfterRecord.closeTab === 'Ctrl+Q' && Object.keys(overridesAfterRecord).length === 1,
      JSON.stringify(overridesAfterRecord),
    );

    // S3 冲突检测：打开文件 录制 Ctrl+Q（已占用）
    currentStep = 'S3 冲突检测';
    await startRecording('打开文件');
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
    await startRecording('关闭当前标签');
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
    await startRecording('关闭当前标签');
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
    // 激活目标：窗口刚打开时首个按键注入偶发被吞（机器负载下更明显）；bringToFront 稳定输入路由
    await settingsClient.send('Page.bringToFront');
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
    await startRecording('打开文件');
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
    // 行渲染存在性等待（重开窗口后列表可能尚未渲染，S11c 偶发失败的根因）
    const rowPresent = await waitForValue(
      async () =>
        (await evalSet(
          `[...document.querySelectorAll('.row')].some((r) => r.querySelector('.label')?.textContent?.trim() === '打开文件')`,
        )) === true
          ? true
          : null,
      8000,
    );
    if (rowPresent === true) {
      await evalSet(
        `(() => { const row = [...document.querySelectorAll('.row')].find((r) => r.querySelector('.label')?.textContent?.trim() === '打开文件'); row?.querySelector('.reset')?.click(); return true; })()`,
      );
    }
    const openRestored = await waitForValue(async () => ((await comboText('打开文件')) === 'Ctrl+O' ? true : null), 6000);
    check('S11c 打开文件单条恢复默认', openRestored === true, rowPresent === true ? '' : '行未渲染');
    await closeSettings();

    // S12 持久化跨重启：自定义绑定在应用重启后仍生效
    currentStep = 'S12 跨重启持久化';
    await reopenSettings();
    await startRecording('关闭当前标签');
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
      `(() => { const labels = [...document.querySelectorAll('.rows .label')].map((el) => el.textContent ?? ''); return labels.some((t) => t.includes('只读阈值')); })()`,
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
          const range = document.querySelector('input[data-setting="reader.typography.fontSize"]');
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

    // S13d 阅读区重排回归：先放大到 30，再降到 16，滚动高度必须无需滚动即回落
    // （历史缺陷：高度模型首轮测量拿到旧字号度量 → 滚动高度偏大、需滚动一次才修正）
    currentStep = 'S13d 阅读区重排';
    await setFontSize(30);
    await delay(600);
    const tallHeight = await evalMain(`document.querySelector('.reader')?.scrollHeight ?? -1`);
    await setFontSize(16);
    const shrank = await waitForValue(async () => {
      const height = await evalMain(`document.querySelector('.reader')?.scrollHeight ?? -1`);
      return height > 0 && height < tallHeight * 0.7 ? true : null;
    }, 6000);
    check(
      'S13d 下调字号后阅读区无需滚动即重排',
      shrank === true,
      `h30=${tallHeight} h16=${await evalMain(`document.querySelector('.reader')?.scrollHeight ?? -1`)}`,
    );

    // S14 新增排版项：段间距 / 首行缩进 / 文字对齐 / 平滑滚动（实时应用到主窗口变量）
    currentStep = 'S14 新增排版项';
    await evalSet(
      `(() => { const tab = [...document.querySelectorAll('.tabs [role="tab"]')].find((b) => b.textContent.trim() === '阅读排版'); tab?.click(); return true; })()`,
    );
    await delay(250);
    const setRangeValue = async (setting, value, expectVar, expectValue) => {
      await evalSet(
        `(() => {
          const range = document.querySelector('input[data-setting="${setting}"]');
          if (!range) return false;
          const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
          setter.call(range, '${value}');
          range.dispatchEvent(new Event('input', { bubbles: true }));
          range.dispatchEvent(new Event('change', { bubbles: true }));
          return true;
        })()`,
      );
      return waitForValue(async () => {
        const got = await evalMain(
          `getComputedStyle(document.documentElement).getPropertyValue('${expectVar}').trim()`,
        );
        return got === expectValue ? true : null;
      }, 6000);
    };
    check(
      'S14a 段间距实时生效',
      (await setRangeValue('reader.typography.paragraphSpacing', 12, '--reading-para-spacing', '12px')) === true,
    );
    check(
      'S14b 首行缩进实时生效（2 字 ×16px=32px）',
      (await setRangeValue('reader.typography.firstLineIndent', 2, '--reading-indent', '32px')) === true,
    );
    await setRangeValue('reader.typography.paragraphSpacing', 0, '--reading-para-spacing', '0px');
    await setRangeValue('reader.typography.firstLineIndent', 0, '--reading-indent', '0px');
    const applyAlign = async (value, expect) => {
      await evalSet(
        `(() => {
          const sel = document.querySelector('select[data-setting="reader.typography.textAlign"]');
          if (!sel) return false;
          const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value').set;
          setter.call(sel, '${value}');
          sel.dispatchEvent(new Event('change', { bubbles: true }));
          return true;
        })()`,
      );
      return waitForValue(async () => {
        const got = await evalMain(
          `getComputedStyle(document.documentElement).getPropertyValue('--reading-align').trim()`,
        );
        return got === expect ? true : null;
      }, 6000);
    };
    check('S14c 文字对齐切换生效', (await applyAlign('justify', 'justify')) === true);
    await applyAlign('left', 'left');
    const toggleRow = async (setting, checked) => {
      await evalSet(
        `(() => {
          const box = document.querySelector('input[data-setting="${setting}"]');
          if (!box) return false;
          if (box.checked !== ${checked}) box.click();
          return true;
        })()`,
      );
      return waitForValue(async () => {
        const now = await evalSet(
          `document.querySelector('input[data-setting="${setting}"]')?.checked`,
        );
        return now === checked ? true : null;
      }, 6000);
    };
    check('S14d 平滑滚动开关可切换', (await toggleRow('reader.typography.smoothScroll', false)) === true);
    check('S14e 平滑滚动恢复默认', (await toggleRow('reader.typography.smoothScroll', true)) === true);

    // S14f–S14i 页边距四向 × 阅读/编辑两套（S4）
    currentStep = 'S14f 页边距（阅读/编辑两套）';
    const setMargin = async (id, value) => {
      await evalSet(
        `(() => {
          const input = document.querySelector('input[data-setting="${id}"]');
          input.value = ${JSON.stringify(String(value))};
          input.dispatchEvent(new Event('input', { bubbles: true }));
          input.dispatchEvent(new Event('change', { bubbles: true }));
          return true;
        })()`,
      );
      await delay(250);
    };
    await setMargin('reader.margins.reading.top', 200);
    await delay(600);
    await setMargin('reader.margins.editing.top', 20);
    await delay(600);
    const readPad = await evalMain(
      `(() => { const page = document.querySelector('.page'); return page ? Number.parseFloat(getComputedStyle(page).paddingTop) : -1; })()`,
    );
    check('S14f 阅读上边距即时生效', Math.round(readPad) === 200, String(readPad));
    await evalMain(`(() => { document.querySelector('[aria-label="切换编辑模式"]')?.click(); return true; })()`);
    await delay(400);
    const editPad = await evalMain(
      `(() => { const page = document.querySelector('.page'); return page ? Number.parseFloat(getComputedStyle(page).paddingTop) : -1; })()`,
    );
    check('S14g 编辑上边距独立生效', Math.round(editPad) === 20, String(editPad));
    await evalMain(`(() => { document.querySelector('[aria-label="切换编辑模式"]')?.click(); return true; })()`);
    await delay(300);
    await setMargin('reader.margins.reading.top', 48);
    await delay(600);
    await setMargin('reader.margins.editing.top', 48);
    await delay(600);
    const restoredPad = await evalMain(
      `(() => { const page = document.querySelector('.page'); return page ? Number.parseFloat(getComputedStyle(page).paddingTop) : -1; })()`,
    );
    const { readFileSync } = await import('node:fs');
    const fileMargins = JSON.parse(readFileSync(join(runDataDir, 'reader.json'), 'utf8')).margins ?? {};
    check(
      'S14h 边距恢复生效',
      Math.round(restoredPad) === 48,
      `page=${restoredPad} file.reading.top=${fileMargins.reading?.top} file.editing.top=${fileMargins.editing?.top}`,
    );
    const marginsGroup = await evalSet(`!!document.querySelector('[data-setting="reader.margins.reading.top"]')`);
    check('S14i 设置页存在页边距分组', marginsGroup === true);

    // S15 自定义字体：按钮 → 原生对话框（取消）；直接链路导入 / 应用 / 删除（确认框取消）
    currentStep = 'S15 字体导入';
    await evalSet(
      `(() => { const btn = document.querySelector('button[data-setting="importFont"]'); btn?.click(); return true; })()`,
    );
    const importDialogOpened = await waitDialog(true);
    if (importDialogOpened) dialogOp('close');
    check('S15a 导入字体按钮打开原生对话框', importDialogOpened === true);
    check('S15b 取消对话框后关闭', (await waitDialog(false, 5000)) === true);
    await delay(300);
    // 系统字体目录来自环境变量（禁止硬编码目录）；候选字体取首个可用者
    const fontDir = join(process.env.WINDIR ?? process.env.SystemRoot ?? '', 'Fonts');
    const fontCandidates = ['consola.ttf', 'arial.ttf', 'segoeui.ttf'].map((name) =>
      join(fontDir, name),
    );
    let imported = null;
    for (const candidate of fontCandidates) {
      const result = await evalMain(
        `window.__TAURI_INTERNALS__.invoke('import_font', { path: ${JSON.stringify(candidate)} })
          .then((entry) => entry.fileName)
          .catch((error) => 'ERR:' + JSON.stringify(error))`,
      );
      if (typeof result === 'string' && !result.startsWith('ERR:')) {
        imported = result;
        break;
      }
    }
    check('S15c 导入字体链路成功', typeof imported === 'string', String(imported));
    if (typeof imported === 'string') {
      await closeSettings();
      await reopenSettings();
      await evalSet(
        `(() => { const tab = [...document.querySelectorAll('.tabs [role="tab"]')].find((b) => b.textContent.trim() === '阅读排版'); tab?.click(); return true; })()`,
      );
      await delay(400);
    }
    const listed = await waitForValue(async () => {
      const values = await evalSet(
        `(() => [...(document.querySelector('input[data-setting="reader.typography.fontFamily"]')?.list?.options ?? [])].map((o) => o.value))()`,
      );
      return Array.isArray(values) && values.some((value) => value.startsWith('custom:')) ? true : null;
    }, 8000);
    check('S15d 自定义字体出现在字体列表', listed === true);
    const customValue = await evalSet(
      `(() => [...(document.querySelector('input[data-setting="reader.typography.fontFamily"]')?.list?.options ?? [])].map((o) => o.value).find((value) => value.startsWith('custom:')) ?? null)()`,
    );
    if (customValue) {
      await evalSet(
        `(() => {
          const input = document.querySelector('input[data-setting="reader.typography.fontFamily"]');
          input.value = ${JSON.stringify(customValue)};
          input.dispatchEvent(new Event('change', { bubbles: true }));
          return true;
        })()`,
      );
    }
    const fontApplied = await waitForValue(async () => {
      const stack = await evalMain(
        `getComputedStyle(document.documentElement).getPropertyValue('--font-reading').trim()`,
      );
      return stack.includes('SRT Custom') ? true : null;
    }, 15000);
    check('S15e 自定义字体加载并应用', fontApplied === true);
    const fontFaceLoaded = await waitForValue(async () => {
      const status = await evalMain(
        `(() => { const face = [...document.fonts].find((f) => f.family.includes('SRT Custom')); return face ? face.status : 'missing'; })()`,
      );
      return status === 'loaded' ? true : null;
    }, 8000);
    check('S15e2 FontFace 实际加载（read_font_data 生效）', fontFaceLoaded === true);
    await evalSet(
      `(() => { const btn = document.querySelector('button.btn.danger'); btn?.click(); return true; })()`,
    );
    const deleteDialogOpened = await waitDialog(true);
    if (deleteDialogOpened) dialogOp('close');
    check('S15f 删除字体弹出原生确认框', deleteDialogOpened === true);
    await waitForValue(async () => ((await waitDialog(false, 4000)) ? true : null), 6000);
    await delay(300);
    const stillThere = await evalSet(
      `(() => [...(document.querySelector('input[data-setting="reader.typography.fontFamily"]')?.list?.options ?? [])].some((o) => o.value.startsWith('custom:')))()`,
    );
    check('S15g 取消删除后字体保留', stillThere === true);
    // 恢复默认字体，再走删除链路（确认框无法自动点「是」，此处直连命令验证）
    await evalSet(
      `(() => {
        const input = document.querySelector('input[data-setting="reader.typography.fontFamily"]');
        input.value = 'Microsoft YaHei';
        input.dispatchEvent(new Event('change', { bubbles: true }));
        return true;
      })()`,
    );
    await delay(400);
    const removed = await evalMain(
      `window.__TAURI_INTERNALS__.invoke('remove_font', { fileName: ${JSON.stringify(imported)} })
        .then(() => true)
        .catch(() => false)`,
    );
    check('S15h 删除字体链路成功', removed === true);
    const fontsLeft = await evalMain(
      `window.__TAURI_INTERNALS__.invoke('list_fonts').then((list) => list.length).catch(() => -1)`,
    );
    check('S15i 删除后自定义字体列表为空', fontsLeft === 0, `count=${fontsLeft}`);

    // S16 状态栏元素开关（阅读排版页）与启动行为开关（常规页）
    currentStep = 'S16 界面与启动开关';
    await clickTab('阅读排版');
    check('S16a 状态栏开关切换', (await toggleRow('app.status.clickableEncoding', false)) === true);
    check('S16b 状态栏开关还原', (await toggleRow('app.status.clickableEncoding', true)) === true);
    await clickTab('常规');
    check('S16c 启动恢复会话开关切换', (await toggleRow('app.startup.restoreSession', false)) === true);
    check('S16d 启动恢复会话开关还原', (await toggleRow('app.startup.restoreSession', true)) === true);
    check(
      'S16e 启动恢复窗口开关存在',
      (await evalSet(`!!document.querySelector('input[data-setting="app.startup.restoreWindow"]')`)) === true,
    );

    // ---- S17 主题编辑器（设置自定义化 S3） ----
    currentStep = 'S17 主题编辑器';
    await clickTab('阅读排版');
    await delay(250);
    const editorExists = await evalSet(`!!document.querySelector('[data-theme-editor]')`);
    check('S17a 主题编辑器卡片存在', editorExists === true);

    await evalSet(`(() => { document.querySelector('[data-theme-editor-palette]')?.click(); return true; })()`);
    const tokenValues = await evalSet(
      `(() => [...document.querySelectorAll('.token-text[data-token]')].map((el) => el.value))()`,
    );
    check(
      'S17b 智能配色生成全部 13 项合法颜色',
      Array.isArray(tokenValues) &&
        tokenValues.length === 13 &&
        tokenValues.every((value) => /^#[0-9A-Fa-f]{6}$/.test(value)),
      String(tokenValues?.[0] ?? ''),
    );

    const editorId = `custom-e2e-${Date.now().toString(36)}`;
    await evalSet(
      `(() => {
        const setValue = (selector, value) => {
          const input = document.querySelector(selector);
          input.value = value;
          input.dispatchEvent(new Event('input', { bubbles: true }));
          input.dispatchEvent(new Event('change', { bubbles: true }));
          return true;
        };
        setValue('[data-theme-editor-name]', 'E2E 主题');
        setValue('[data-theme-editor-name-en]', 'E2E theme');
        setValue('[data-theme-editor-id]', ${JSON.stringify(editorId)});
        return true;
      })()`,
    );
    await evalSet(`(() => { document.querySelector('[data-theme-editor-save]')?.click(); return true; })()`);
    const savedListed = await waitForValue(async () => {
      const ids = await evalMain(
        `window.__TAURI_INTERNALS__.invoke('list_themes').then((list) => list.map((t) => t.id)).catch(() => [])`,
      );
      return Array.isArray(ids) && ids.includes(editorId) ? true : null;
    }, 8000);
    check('S17c 保存后用户主题出现在清单', savedListed === true);

    await evalSet(`(() => { document.querySelector('[data-theme-editor-ai]')?.click(); return true; })()`);
    const aiToast = await waitForValue(async () => {
      const text = await evalSet(
        `[...document.querySelectorAll('.toast')].map((node) => node.textContent).join('|')`,
      );
      return typeof text === 'string' && text.includes('即将推出') ? true : null;
    }, 4000);
    check('S17d AI 生成占位提示', aiToast === true);

    const cleanup = await evalMain(
      `window.__TAURI_INTERNALS__.invoke('remove_theme', { id: ${JSON.stringify(editorId)} }).then(() => true).catch(() => false)`,
    );
    check('S17e 测试主题清理', savedListed === true ? cleanup === true : true, savedListed === true ? '' : '未保存，跳过');

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
