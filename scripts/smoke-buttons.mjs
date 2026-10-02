#!/usr/bin/env node
// 全按钮审计（阶段 5 前置）：逐个点击所有可见且已实现的按钮/菜单项，断言其真实效果。
// 覆盖：空状态、标题栏（见 smoke-titlebar）、工具栏、菜单栏（文件/编辑/查看/帮助全部项）、
//       标签栏关闭、查找条关闭、状态栏编码菜单、退出流（含确认与退出）。
// 规则：未实现的功能保持灰态（本套件断言语义），绝不测试灰按钮的点击效果。
// 原生对话框（打开/另存为）：经 user32 EnumWindows 检测 #32770 窗口出现并 WM_CLOSE 关闭。
//
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-buttons.mjs [--exe <路径>] [--port 9226]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDialogOps } from './lib/dialog.mjs';
import { argValue, createClient, delay, findTarget, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const port = Number(argValue('--port', String(9200 + Math.floor(Math.random() * 600))));
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '480000');

const workDir = join(process.env.TEMP ?? '.', 'srt-smoke-buttons');
const fileA = join(workDir, 'buttons-a.txt');
const fileB = join(workDir, 'buttons-b.txt');
const originalA = 'alpha\nbeta\ngamma\n';
const runDataDir = join(workDir, `data-${Date.now()}`);

const checks = [];
let currentStep = 'B0 启动';
function check(name, passed, detail = '') {
  checks.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

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
  rmSync(fileA, { force: true });
  rmSync(fileB, { force: true });
  writeFileSync(fileA, originalA, 'utf8');
  writeFileSync(fileB, 'second\n', 'utf8');

  const child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: runDataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });

  /** 原生对话框探测/关闭（共享实现：scripts/lib/dialog.mjs） */
  const { dialogOp, waitDialog } = createDialogOps(child.pid);

  let client;
  try {
    const wsUrl = await findTarget(port);
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
    const rowText = (row) => evalJs(`document.querySelector('.row[data-row="${row}"]')?.textContent ?? ''`);
    const activeTab = async () =>
      evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return v.tabs.find((t) => t.tabId === v.activeTabId); })()`,
      );
    const click = (selector) => evalJs(`(() => { const el = document.querySelector('${selector}'); el?.click(); return !!el; })()`);
    const clickByText = (selector, text) =>
      evalJs(
        `(() => { const el = [...document.querySelectorAll('${selector}')].find((n) => n.textContent.trim() === '${text}' || n.textContent.trim().startsWith('${text}')); el?.click(); return !!el; })()`,
      );
    const menuClick = async (title, itemText) => {
      await clickByText('.menu-bar .title', title);
      await delay(150);
      return clickByText('.menu-bar .item', itemText);
    };
    const isDisabled = (selector) =>
      evalJs(`document.querySelector('${selector}')?.disabled === true`);
    const menuItemDisabled = async (title, itemText) => {
      await clickByText('.menu-bar .title', title);
      await delay(150);
      const value = await evalJs(
        `(() => { const it = [...document.querySelectorAll('.menu-bar .item')].find((n) => n.textContent.trim().startsWith('${itemText}')); return it ? it.disabled === true : null; })()`,
      );
      await evalJs(`(document.querySelector('.menu-bar') && document.body.click(), true)`);
      await delay(100);
      return value;
    };
    const focusProxy = () => evalJs(`(document.querySelector('textarea.input-proxy')?.focus(), true)`);
    const ctrlKey = async (key, code, vk) => {
      await client.send('Input.dispatchKeyEvent', {
        type: 'rawKeyDown',
        key,
        code,
        windowsVirtualKeyCode: vk,
        nativeVirtualKeyCode: vk,
        modifiers: 2,
      });
      await client.send('Input.dispatchKeyEvent', {
        type: 'keyUp',
        key,
        code,
        windowsVirtualKeyCode: vk,
        nativeVirtualKeyCode: vk,
        modifiers: 2,
      });
    };
    const typeText = async (text) => {
      await focusProxy();
      await ctrlKey('Home', 'Home', 36); // 光标归位到文档首（Ctrl+Home），保证插入位置确定
      await delay(120);
      await client.send('Input.insertText', { text });
    };
    /** 确保处于编辑模式（重载会丢弃编辑文档，需重新进入）。 */
    const ensureEditing = async () => {
      const tab = await activeTab();
      if (tab && !tab.editing) {
        await click('[aria-label="切换编辑模式"]');
        await waitForValue(async () => {
          const next = await activeTab();
          return next?.editing ? true : null;
        }, 5000);
      }
      await delay(150);
    };
    const dialogButton = (label) =>
      evalJs(
        `(() => { const dlg = document.querySelector('[role="dialog"],[role="alertdialog"]'); if (!dlg) return false; const btn = [...dlg.querySelectorAll('button')].find((b) => b.textContent.trim() === '${label}'); btn?.click(); return !!btn; })()`,
      );

    // 就绪护栏
    let ready = false;
    for (let attempt = 0; attempt < 180; attempt += 1) {
      ready = (await evalJs('!!window.__srt?.openPath && !!window.__TAURI_INTERNALS__')) === true;
      if (ready) break;
      await delay(250);
    }
    if (!ready) throw new Error('前端未就绪（window.__srt 未注入）');

    // ---- B. 空状态 ----
    currentStep = 'B1 空状态打开按钮';
    await click('.open-btn');
    const dialogShown = await waitDialog(true);
    if (dialogShown) dialogOp('close');
    const dialogClosed = dialogShown ? await waitDialog(false) : false;
    check('B1 空状态「打开文件」→ 原生对话框出现并可关闭', dialogShown && dialogClosed);

    // ---- C. 工具栏 ----
    currentStep = 'C1 打开样本文件';
    await evalJs(`window.__srt.openPath(${JSON.stringify(fileA)})`);
    const opened = await waitForValue(async () => {
      const text = await rowText(0);
      return text === 'alpha' ? text : null;
    }, 8000);
    check('C1 打开样本文件', opened === 'alpha');

    currentStep = 'C2 工具栏打开按钮';
    await click('[aria-label="打开文件"]');
    const openDialog = await waitDialog(true);
    if (openDialog) dialogOp('close');
    const openDialogClosed = openDialog ? await waitDialog(false) : false;
    check('C2 工具栏「打开文件」→ 原生对话框出现并可关闭', openDialog && openDialogClosed);

    currentStep = 'C3 工具栏历史按钮（灰态断言）';
    const historyDisabled = await isDisabled('[aria-label="历史记录"]');
    check('C3 工具栏「历史记录」未实现 → 按要求为灰态', historyDisabled === true);

    currentStep = 'C4 编辑模式切换';
    await click('[aria-label="切换编辑模式"]');
    const editingOn = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.editing ? true : null;
    }, 5000);
    const pressedOn = await evalJs(
      `document.querySelector('[aria-label="切换编辑模式"]')?.getAttribute('aria-pressed') === 'true'`,
    );
    await click('[aria-label="切换编辑模式"]');
    const editingOff = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.editing === false ? true : null;
    }, 5000);
    await click('[aria-label="切换编辑模式"]');
    const editingBack = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.editing === true ? true : null;
    }, 5000);
    check('C4 「切换编辑模式」三连击（开→关→开，aria-pressed 同步）', editingOn && pressedOn === true && editingOff && editingBack);

    currentStep = 'C5 工具栏保存（禁用/取消/确认）';
    const saveDisabledClean = await isDisabled('[aria-label="保存"]');
    await typeText('SAV');
    const dirtyOn = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty ? true : null;
    }, 5000);
    await click('[aria-label="保存"]');
    const saveDialog = await waitForValue(
      async () => ((await evalJs(`document.querySelector('[role="dialog"]') !== null`)) ? true : null),
      5000,
    );
    await dialogButton('取消');
    const diskAfterCancel = readFileSync(fileA, 'utf8');
    const stillDirty = await evalJs(`(async () => (await window.__TAURI_INTERNALS__.invoke('list_tabs')).tabs[0].dirty)()`);
    await click('[aria-label="保存"]');
    await waitForValue(
      async () => ((await evalJs(`document.querySelector('[role="dialog"]') !== null`)) ? true : null),
      5000,
    );
    await dialogButton('保存');
    const savedOk = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty === false && readFileSync(fileA, 'utf8') === 'SAV' + originalA ? true : null;
    }, 8000);
    check(
      'C5 工具栏「保存」：干净禁用 / 脏启用 / 取消不写盘 / 确认写盘',
      saveDisabledClean === true && dirtyOn && saveDialog === true && diskAfterCancel === originalA && stillDirty === true && savedOk === true,
      `cleanDisabled=${saveDisabledClean} diskCancel=${JSON.stringify(diskAfterCancel.slice(0, 12))}`,
    );

    currentStep = 'C6 工具栏编码下拉';
    await evalJs(
      `(() => { const wraps = document.querySelectorAll('.encoding-wrap'); wraps[0].querySelector('.enc-btn').click(); return true; })()`,
    );
    await waitForValue(
      async () => ((await evalJs(`document.querySelectorAll('.dropdown[role="menu"]').length > 0`)) ? true : null),
      4000,
    );
    await clickByText('.dropdown .item', 'GB18030');
    const encOverride = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.encodingOverride === 'GB18030' ? tab : null;
    }, 5000);
    await evalJs(
      `(() => { const wraps = document.querySelectorAll('.encoding-wrap'); wraps[0].querySelector('.enc-btn').click(); return true; })()`,
    );
    await delay(200);
    await clickByText('.dropdown .item', '自动检测');
    const encAuto = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.encodingOverride === null ? tab : null;
    }, 5000);
    check('C6 工具栏编码下拉（覆盖/恢复自动）', encOverride !== null && encAuto !== null);
    await ensureEditing(); // 编码切换会丢弃编辑文档（set_encoding 语义）：后续编辑项需重新进入

    currentStep = 'C7 工具栏主题循环';
    const themeSeq = [];
    // 初始为「跟随系统」（解析浅色）：点击循环 light→dark→eye→system
    for (let i = 0; i < 3; i += 1) {
      await click('[aria-label="切换主题"]');
      await delay(250);
      themeSeq.push(await evalJs(`document.documentElement.dataset.theme`));
    }
    await click('[aria-label="切换主题"]');
    await delay(250);
    const systemTheme = await evalJs(`document.documentElement.dataset.theme`);
    check(
      'C7 工具栏「切换主题」循环（light→dark→eye→system）',
      themeSeq[0] === 'light' && themeSeq[1] === 'dark' && themeSeq[2] === 'eye' && (systemTheme === 'light' || systemTheme === 'dark'),
      `seq=${themeSeq.join(',')} system=${systemTheme}`,
    );

    currentStep = 'C8 工具栏设置按钮（灰态断言）';
    const settingsDisabled = await isDisabled('[aria-label="设置"]');
    check('C8 工具栏「设置」未实现 → 按要求为灰态', settingsDisabled === true);

    // ---- D. 菜单栏 ----
    currentStep = 'D1 菜单标题开合';
    const titles = ['文件', '编辑', '查看', '帮助'];
    let d1 = true;
    for (const title of titles) {
      await clickByText('.menu-bar .title', title);
      await delay(150);
      const open = await evalJs(`document.querySelector('.menu-bar .dropdown') !== null`);
      if (open !== true) d1 = false;
      await evalJs(`(document.body.click(), true)`);
      await delay(100);
      const closed = await evalJs(`document.querySelector('.menu-bar .dropdown') === null`);
      if (closed !== true) d1 = false;
    }
    check('D1 四个菜单标题：点击展开、点击外部收起', d1);

    currentStep = 'D2 文件→重新加载（确认流）';
    await typeText('R');
    await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty ? true : null;
    }, 5000);
    await menuClick('文件', '重新加载');
    const reloadDialog = await waitForValue(
      async () => ((await evalJs(`document.querySelector('[role="alertdialog"]') !== null`)) ? true : null),
      5000,
    );
    await dialogButton('取消');
    const stillDirty2 = await evalJs(`(async () => (await window.__TAURI_INTERNALS__.invoke('list_tabs')).tabs[0].dirty)()`);
    await menuClick('文件', '重新加载');
    await waitForValue(
      async () => ((await evalJs(`document.querySelector('[role="alertdialog"]') !== null`)) ? true : null),
      5000,
    );
    await dialogButton('重新加载');
    const reloadedClean = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty === false && (await rowText(0)) === 'SAValpha' ? true : null;
    }, 8000);
    await ensureEditing(); // 重载会丢弃编辑文档（editing=false）：后续编辑项需重新进入
    check(
      'D2 文件→「重新加载」：取消保留修改 / 确认丢弃并重载',
      reloadDialog === true && stillDirty2 === true && reloadedClean === true,
      `dialog=${reloadDialog} stillDirty=${stillDirty2} reloaded=${reloadedClean} row0=${await rowText(0)}`,
    );

    currentStep = 'D3 编辑→撤销/重做';
    await typeText('X');
    await waitForValue(async () => ((await rowText(0)) === 'XSAValpha' ? true : null), 5000);
    await menuClick('编辑', '撤销');
    const undone = await waitForValue(async () => {
      const tab = await activeTab();
      return (await rowText(0)) === 'SAValpha' && tab?.dirty === false ? true : null;
    }, 6000);
    await menuClick('编辑', '重做');
    const redone = await waitForValue(async () => ((await rowText(0)) === 'XSAValpha' ? true : null), 6000);
    await menuClick('编辑', '撤销');
    await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty === false ? true : null;
    }, 6000);
    check('D3 编辑→「撤销」「重做」', undone === true && redone === true);

    currentStep = 'D4 编辑→全选/复制/粘贴';
    await menuClick('编辑', '全选');
    const selected = await waitForValue(async () => {
      const count = await evalJs(`document.querySelectorAll('.selection').length`);
      return count > 0 ? count : null;
    }, 5000);
    await menuClick('编辑', '复制');
    await delay(300);
    const clipAfterCopy = spawnSync('powershell', ['-NoProfile', '-Command', 'Get-Clipboard -Raw'], {
      encoding: 'utf8',
      timeout: 15000,
    }).stdout;
    spawnSync('powershell', ['-NoProfile', '-Command', `Set-Clipboard -Value 'PASTED-SRT'`], {
      timeout: 15000,
    });
    await menuClick('编辑', '粘贴');
    const pasted = await waitForValue(async () => {
      const tab = await activeTab();
      return (await rowText(0)) === 'PASTED-SRT' && tab?.dirty === true ? true : null;
    }, 6000);
    await menuClick('编辑', '撤销');
    await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty === false ? true : null;
    }, 6000);
    const copyOk = typeof clipAfterCopy === 'string' && clipAfterCopy.includes('SAValpha');
    check(
      'D4 编辑→「全选」「复制」「粘贴」（写/读剪贴板）',
      selected !== null && copyOk && pasted === true,
      `selected=${selected} copyHasText=${copyOk} pasted=${pasted}`,
    );

    currentStep = 'D5 编辑→查找/替换（面板开合）';
    await menuClick('编辑', '查找…');
    const findBar1 = await waitForValue(
      async () => ((await evalJs(`document.querySelectorAll('.find-bar input').length`)) || null),
      4000,
    );
    await evalJs(`(document.querySelector('.find-bar button[aria-label="关闭查找"]')?.click(), true)`);
    await waitForValue(async () => ((await evalJs(`document.querySelector('.find-bar') === null`)) ? true : null), 4000);
    await menuClick('编辑', '替换…');
    const findBar2 = await waitForValue(
      async () => ((await evalJs(`document.querySelectorAll('.find-bar input').length`)) || null),
      4000,
    );
    await evalJs(`(document.querySelector('.find-bar button[aria-label="关闭查找"]')?.click(), true)`);
    await waitForValue(async () => ((await evalJs(`document.querySelector('.find-bar') === null`)) ? true : null), 4000);
    check('D5 编辑→「查找…」「替换…」面板开合并可关闭', findBar1 === 1 && findBar2 === 2);

    currentStep = 'D6 编辑→编辑模式开关（菜单路径）';
    await menuClick('编辑', '退出编辑模式');
    const offByMenu = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.editing === false ? true : null;
    }, 5000);
    await menuClick('编辑', '启用编辑模式');
    const onByMenu = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.editing === true ? true : null;
    }, 5000);
    check('D6 编辑→「退出/启用编辑模式」', offByMenu === true && onByMenu === true);

    currentStep = 'D7 编辑→保存（菜单路径）';
    await typeText('M');
    await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty ? true : null;
    }, 5000);
    await menuClick('编辑', '保存');
    await waitForValue(
      async () => ((await evalJs(`document.querySelector('[role="dialog"]') !== null`)) ? true : null),
      5000,
    );
    await dialogButton('保存');
    const savedByMenu = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty === false && readFileSync(fileA, 'utf8') === 'MSAValpha\nbeta\ngamma\n' ? true : null;
    }, 8000);
    check('D7 编辑→「保存」', savedByMenu === true, `disk=${JSON.stringify(readFileSync(fileA, 'utf8').slice(0, 12))}`);

    currentStep = 'D8 编辑→另存为（原生对话框开合）';
    await menuClick('编辑', '另存为…');
    const saveAsDialog = await waitDialog(true);
    if (saveAsDialog) dialogOp('close');
    const saveAsClosed = saveAsDialog ? await waitDialog(false) : false;
    const tabPathUnchanged = await evalJs(
      `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return v.tabs[0].name === 'buttons-a.txt'; })()`,
    );
    check('D8 编辑→「另存为…」→ 原生对话框出现 / 取消后标签不变', saveAsDialog && saveAsClosed && tabPathUnchanged === true);

    currentStep = 'D9 编辑→字号/查看菜单灰态';
    const sizeUpDisabled = await menuItemDisabled('查看', '字号增大');
    const sizeDownDisabled = await menuItemDisabled('查看', '字号减小');
    check('D9 查看→「字号增大/减小」未实现 → 灰态', sizeUpDisabled === true && sizeDownDisabled === true);

    currentStep = 'D10 查看→主题四项';
    let d10 = true;
    for (const [label, expected] of [
      ['深色', 'dark'],
      ['护眼', 'eye'],
      ['浅色', 'light'],
      ['跟随系统', null],
    ]) {
      await menuClick('查看', label);
      await delay(300);
      const theme = await evalJs(`document.documentElement.dataset.theme`);
      if (expected === null ? !(theme === 'light' || theme === 'dark') : theme !== expected) d10 = false;
    }
    check('D10 查看→主题四项切换生效', d10);

    currentStep = 'D11 查看→全屏';
    const fsInvoke = `window.__TAURI_INTERNALS__.invoke('plugin:window|is_fullscreen', { label: 'main' })`;
    await menuClick('查看', '全屏');
    const fullscreened = await waitForValue(async () => {
      const on = await evalJs(fsInvoke);
      return on === true ? true : null;
    }, 6000);
    await menuClick('查看', '全屏');
    const windowed = await waitForValue(async () => {
      const on = await evalJs(fsInvoke);
      return on === false ? true : null;
    }, 6000);
    check('D11 查看→「全屏」开合（含 F11 同款逻辑）', fullscreened === true && windowed === true);

    currentStep = 'D12 帮助菜单灰态';
    const helpShortcut = await menuItemDisabled('帮助', '快捷键…');
    const helpAbout = await menuItemDisabled('帮助', '关于 S-Read-TXT');
    check(
      'D12 帮助→「快捷键…」「关于」未实现 → 灰态',
      helpShortcut === true && helpAbout === true,
      `shortcut=${helpShortcut} about=${helpAbout}`,
    );

    // ---- G. 状态栏 ----
    currentStep = 'G1 状态栏编码菜单';
    await evalJs(
      `(() => { const wraps = document.querySelectorAll('.encoding-wrap'); wraps[1].querySelector('.enc-btn').click(); return true; })()`,
    );
    await waitForValue(
      async () => ((await evalJs(`document.querySelectorAll('.dropdown[role="menu"]').length > 0`)) ? true : null),
      4000,
    );
    const statusShot = await client.send('Page.captureScreenshot', { format: 'png' });
    mkdirSync(join(root, 'docs', 'screenshots'), { recursive: true });
    writeFileSync(join(root, 'docs', 'screenshots', 'phase5-buttons-statusbar.png'), Buffer.from(statusShot.data, 'base64'));
    await clickByText('.dropdown .item', 'GB18030');
    const statusEnc = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.encodingOverride === 'GB18030' ? true : null;
    }, 5000);
    await evalJs(
      `(() => { const wraps = document.querySelectorAll('.encoding-wrap'); wraps[1].querySelector('.enc-btn').click(); return true; })()`,
    );
    await delay(200);
    await clickByText('.dropdown .item', '自动检测');
    const statusEncAuto = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.encodingOverride === null ? true : null;
    }, 5000);
    check('G1 状态栏编码按钮（上弹菜单：覆盖/恢复自动）', statusEnc === true && statusEncAuto === true);
    await ensureEditing(); // 同上：编码切换丢弃编辑文档

    // ---- E. 标签栏 ----
    currentStep = 'E1 关闭干净标签';
    await evalJs(`window.__srt.openPath(${JSON.stringify(fileB)})`);
    await waitForValue(async () => {
      const count = await evalJs(`document.querySelectorAll('.tab').length`);
      return count === 2 ? true : null;
    }, 6000);
    await click('[aria-label="关闭 buttons-b.txt"]');
    const oneTab = await waitForValue(async () => {
      const count = await evalJs(`document.querySelectorAll('.tab').length`);
      return count === 1 ? true : null;
    }, 6000);
    check('E1 标签栏关闭按钮（干净标签直接关闭）', oneTab === true);

    currentStep = 'E2 脏标签关闭（取消/不保存）';
    await typeText('Z');
    await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty ? true : null;
    }, 5000);
    await click('[aria-label="关闭 buttons-a.txt"]');
    const closeDialog = await waitForValue(
      async () => ((await evalJs(`document.querySelector('[role="alertdialog"]') !== null`)) ? true : null),
      5000,
    );
    await dialogButton('取消');
    const tabKept = await waitForValue(async () => {
      const count = await evalJs(`document.querySelectorAll('.tab').length`);
      return count === 1 ? true : null;
    }, 5000);
    await click('[aria-label="关闭 buttons-a.txt"]');
    await waitForValue(
      async () => ((await evalJs(`document.querySelector('[role="alertdialog"]') !== null`)) ? true : null),
      5000,
    );
    await dialogButton('不保存');
    const backToEmpty = await waitForValue(
      async () => ((await evalJs(`document.querySelector('.open-btn') !== null`)) ? true : null),
      6000,
    );
    check('E2 脏标签关闭：取消保留 / 不保存丢弃并回到空状态', closeDialog === true && tabKept === true && backToEmpty === true);

    // ---- 菜单展开截图（人工核验菜单样式） ----
    currentStep = 'S1 菜单截图';
    await clickByText('.menu-bar .title', '查看');
    await delay(250);
    const menuShot = await client.send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(join(root, 'docs', 'screenshots', 'phase5-buttons-menu.png'), Buffer.from(menuShot.data, 'base64'));
    await evalJs(`(document.body.click(), true)`);
    check('S1 菜单展开截图已保存', true);

    // ---- H. 退出流 ----
    currentStep = 'H1 退出（脏标签取消）';
    await evalJs(`window.__srt.openPath(${JSON.stringify(fileA)})`);
    await waitForValue(async () => ((await rowText(0)) !== '' ? true : null), 6000);
    await click('[aria-label="切换编辑模式"]');
    await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.editing ? true : null;
    }, 5000);
    await typeText('Q');
    await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty ? true : null;
    }, 5000);
    await menuClick('文件', '退出');
    const quitDialog = await waitForValue(
      async () => ((await evalJs(`document.querySelector('[role="alertdialog"]') !== null`)) ? true : null),
      5000,
    );
    await dialogButton('取消');
    await delay(400);
    const stillAlive = await evalJs(`!!window.__TAURI_INTERNALS__`);
    check(
      'H1 文件→「退出」（脏标签）→ 取消后应用存活',
      quitDialog === true && stillAlive === true,
      `dialog=${quitDialog} alive=${stillAlive}`,
    );

    currentStep = 'H2 退出（不保存 → 应用退出）';
    await menuClick('文件', '退出');
    const quitDialog2 = await waitForValue(
      async () => ((await evalJs(`document.querySelector('[role="alertdialog"]') !== null`)) ? true : null),
      5000,
    );
    const discardClicked = await dialogButton('不保存');
    // 应用退出：CDP 连接断开 / 进程结束（用进程列表轮询）
    const exited = await waitForValue(async () => {
      const probe = spawnSync('powershell', [
        '-NoProfile',
        '-Command',
        `if (Get-Process -Id ${child.pid} -ErrorAction SilentlyContinue) { 'ALIVE' } else { 'GONE' }`,
      ], { encoding: 'utf8', timeout: 15000 });
      return (probe.stdout ?? '').trim() === 'GONE' ? true : null;
    }, 12000);
    check(
      'H2 文件→「退出」→「不保存」→ 应用退出',
      exited === true,
      `dialog=${quitDialog2} discard=${discardClicked} exited=${exited}`,
    );
  } finally {
    client?.close();
    if (child.pid) {
      // 仅回收本应用进程树；严禁按 msedgewebview2 名称杀进程（已退出时无副作用）
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    }
    for (const entry of ['buttons-a.txt', 'buttons-b.txt', 'buttons-a.txt.bak', 'buttons-b.txt.bak']) {
      await removeWithRetry(join(workDir, entry));
    }
    await removeWithRetry(runDataDir);
  }

  clearTimeout(watchdog);
  const failed = checks.filter((item) => !item.passed);
  console.log(`\n全按钮审计结果：${checks.length - failed.length}/${checks.length} 通过`);
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((error) => {
  clearTimeout(watchdog);
  console.error(`全按钮审计失败（${currentStep}）：${error?.message ?? error}`);
  process.exit(3);
});
