#!/usr/bin/env node
// 查找/替换与编辑菜单冒烟：真实应用 + CDP 驱动。
// 覆盖：查找条开关与聚焦、连续替换（大小写不敏感）、全部替换（单撤销步）、
//       三次撤销全还原、大小写敏感未找到、菜单撤销、另存为链路（IPC 直调）、
//       脏态重新加载（确认后丢弃）。
//
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-find.mjs [--exe <路径>] [--port 9223] [--screenshot <路径>]
// 说明：系统原生「另存为」文件选择框无法脚本化，此处经 IPC 直调验证保存链路
//       （对话框点选由人工核验）；键盘输入经 CDP Input.insertText 投递到焦点元素。

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget, openPathDone, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const port = Number(argValue('--port', String(9200 + Math.floor(Math.random() * 600))));
const screenshotPath = argValue('--screenshot', join(root, 'docs', 'screenshots', 'phase4c-find.png'));
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '300000');

const workDir = join(process.env.TEMP ?? '.', 'srt-smoke-find');
const testFile = join(workDir, 'find-sample.txt');
const saveAsFile = join(workDir, 'renamed.txt');
const originalText = 'alpha needle beta\nNeedle here\nno match x\nneedle again\n';
const runDataDir = join(workDir, `data-${Date.now()}`);

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
  for (const entry of ['find-sample.txt', 'renamed.txt']) {
    rmSync(join(workDir, entry), { force: true });
  }
  writeFileSync(testFile, originalText, 'utf8');

  const child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: runDataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });

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
    /** CDP 组合键（modifiers=2 即 Ctrl）。 */
    const ctrlKey = async (key, code, vk, modifiers = 2) => {
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
    const focusProxy = () => evalJs(`(document.querySelector('textarea.input-proxy')?.focus(), true)`);
    const focusFindInput = () =>
      evalJs(`(() => { const i = document.querySelector('.find-bar input'); i?.focus(); i?.select(); return !!i; })()`);
    const clickBarButton = (text) =>
      evalJs(
        `(() => { const b = [...document.querySelectorAll('.find-bar button')].find((n) => n.textContent.trim() === '${text}'); b?.click(); return !!b; })()`,
      );
    const menuClick = async (title, itemText) => {
      await evalJs(
        `(() => { const t = [...document.querySelectorAll('.menu-bar .title')].find((n) => n.textContent.trim() === '${title}'); t?.click(); return !!t; })()`,
      );
      await delay(150);
      return evalJs(
        `(() => { const it = [...document.querySelectorAll('.menu-bar .item')].find((n) => n.textContent.trim().startsWith('${itemText}')); it?.click(); return !!it; })()`,
      );
    };
    const toastText = () =>
      evalJs(`[...document.querySelectorAll('.toast')].map((n) => n.textContent).join('|')`);

    // 就绪护栏
    let ready = false;
    for (let attempt = 0; attempt < 180; attempt += 1) {
      ready = (await evalJs('!!window.__srt?.openPath && !!window.__TAURI_INTERNALS__')) === true;
      if (ready) break;
      await delay(250);
    }
    if (!ready) throw new Error('前端未就绪（window.__srt 未注入）');
    await dismissOnboarding(evalJs);

    // F1：打开 + 进入编辑
    currentStep = 'F1 打开并进入编辑';
    await evalJs(openPathDone(testFile));
    const firstRow = await waitForValue(async () => {
      const text = await rowText(0);
      return text === 'alpha needle beta' ? text : null;
    }, 8000);
    await evalJs(`(document.querySelector('[aria-label="切换编辑模式"]')?.click(), true)`);
    const editing = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.editing ? tab : null;
    }, 8000);
    check('F1 打开文件并进入编辑', firstRow === 'alpha needle beta' && editing !== null);

    // F2：Ctrl+F 打开查找条并聚焦输入框
    currentStep = 'F2 Ctrl+F 打开查找条';
    await focusProxy();
    await ctrlKey('f', 'KeyF', 70);
    const barOpen = await waitForValue(
      async () => (await evalJs(`document.querySelector('.find-bar') !== null`)) || null,
      5000,
    );
    const focused = await waitForValue(
      async () =>
        ((await evalJs(
          `document.activeElement?.closest('.find-bar') !== null && document.activeElement?.tagName === 'INPUT'`,
        )) === true
          ? true
          : null),
      5000,
    );
    check('F2 Ctrl+F 打开查找条（输入框聚焦）', barOpen === true && focused === true);

    // F3：经菜单打开替换行
    currentStep = 'F3 菜单打开替换行';
    const replaceItem = await menuClick('编辑', '替换…');
    const twoInputs = await waitForValue(async () => {
      const count = await evalJs(`document.querySelectorAll('.find-bar input').length`);
      return count === 2 ? count : null;
    }, 5000);
    check('F3 菜单打开替换行（两个输入框）', replaceItem === true && twoInputs === 2);
    // 在替换输入框（第二个）输入替换文本
    await evalJs(
      `(() => { const inputs = document.querySelectorAll('.find-bar input'); inputs[1]?.focus(); inputs[1]?.select(); return inputs.length; })()`,
    );
    await client.send('Input.insertText', { text: 'REPLACED' });

    // F4：输入查询并替换第一个命中（大小写不敏感命中 row0）
    currentStep = 'F4 替换第一个命中';
    await focusFindInput();
    await client.send('Input.insertText', { text: 'needle' });
    await clickBarButton('替换');
    await delay(800);
    const r4 = await waitForValue(async () => {
      const text = await rowText(0);
      const tab = await activeTab();
      return text === 'alpha REPLACED beta' && tab?.dirty === true ? text : null;
    }, 8000);
    const diag4 = `row0=${await rowText(0)} toast=${await toastText()}`;
    check('F4 替换命中 row0（脏态）', r4 === 'alpha REPLACED beta', r4 ?? diag4);

    // F5：连续替换（大小写不敏感命中 row1 的 Needle）
    currentStep = 'F5 连续替换（大小写不敏感）';
    await clickBarButton('替换');
    const r5 = await waitForValue(async () => {
      const text = await rowText(1);
      return text === 'REPLACED here' ? text : null;
    }, 8000);
    check('F5 大小写不敏感命中并替换 row1', r5 === 'REPLACED here', r5 ?? '(超时)');

    // F6：全部替换（row3 剩余 1 处；命中 =1 直接执行，不弹预览）
    currentStep = 'F6 全部替换';
    await clickBarButton('全部替换');
    const r6 = await waitForValue(async () => {
      const text = await rowText(3);
      const toast = await toastText();
      return text === 'REPLACED again' && toast.includes('已替换 1 处') ? text : null;
    }, 8000);
    check('F6 全部替换（1 处 + 提示）', r6 === 'REPLACED again', r6 ?? '(超时)');

    // 截图：查找条（替换态）+ 替换结果
    try {
      const shot = await client.send('Page.captureScreenshot', { format: 'png' });
      mkdirSync(dirname(screenshotPath), { recursive: true });
      writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'));
      check('F7 截图已保存', true, screenshotPath);
    } catch (error) {
      check('F7 截图已保存', false, String(error));
    }

    // F8：三次撤销全还原（2 次单替换 + 1 次全部替换）
    currentStep = 'F8 撤销全还原';
    await focusProxy();
    await ctrlKey('z', 'KeyZ', 90);
    await delay(150);
    await focusProxy();
    await ctrlKey('z', 'KeyZ', 90);
    await delay(150);
    await focusProxy();
    await ctrlKey('z', 'KeyZ', 90);
    const r8 = await waitForValue(async () => {
      const tab = await activeTab();
      const ok =
        (await rowText(0)) === 'alpha needle beta' &&
        (await rowText(1)) === 'Needle here' &&
        (await rowText(3)) === 'needle again';
      return ok && tab?.dirty === false ? true : null;
    }, 8000);
    check('F8 三次撤销全部还原且干净', r8 === true);

    // F9：开启大小写 → NEEDLE 未找到
    currentStep = 'F9 大小写敏感未找到';
    await clickBarButton('Aa');
    await focusFindInput();
    await client.send('Input.insertText', { text: 'NEEDLE' });
    await clickBarButton('下一个');
    const r9 = await waitForValue(async () => {
      const toast = await toastText();
      return toast.includes('未找到') ? toast : null;
    }, 6000);
    check('F9 大小写敏感：未找到提示', r9 !== null, r9 ?? '(超时)');

    // F10：关闭大小写 → 命中（selection 渲染）
    currentStep = 'F10 关闭大小写后命中';
    await clickBarButton('Aa');
    await clickBarButton('下一个');
    const r10 = await waitForValue(async () => {
      const count = await evalJs(`document.querySelectorAll('.selection').length`);
      return count > 0 ? count : null;
    }, 6000);
    check('F10 关闭大小写后命中并选中', r10 !== null, `selection=${r10}`);

    // F11：Esc 关闭查找条
    currentStep = 'F11 Esc 关闭查找条';
    await focusFindInput();
    await ctrlKey('Escape', 'Escape', 27, 0);
    const r11 = await waitForValue(
      async () => ((await evalJs(`document.querySelector('.find-bar') === null`)) ? true : null),
      5000,
    );
    check('F11 Esc 关闭查找条', r11 === true);

    // F12：菜单撤销（输入 Q 后经 编辑→撤销）
    currentStep = 'F12 菜单撤销';
    await focusProxy();
    await client.send('Input.insertText', { text: 'Q' });
    const qAdded = await waitForValue(async () => {
      const text = await rowText(0);
      return text === 'alpha Q beta' ? text : null;
    }, 6000);
    const undoItem = await menuClick('编辑', '撤销');
    const qUndone = await waitForValue(async () => {
      const text = await rowText(0);
      const tab = await activeTab();
      return text === 'alpha needle beta' && tab?.dirty === false ? true : null;
    }, 8000);
    check('F12 菜单撤销生效', qAdded !== null && undoItem === true && qUndone === true, `q=${qAdded} menu=${undoItem} undone=${qUndone}`);

    // F13：另存为链路（IPC 直调；原生对话框人工核验）
    currentStep = 'F13 另存为链路';
    const tabId = (await activeTab())?.tabId;
    const saveAsResult = await evalJs(
      `(async () => { try { const v = await window.__TAURI_INTERNALS__.invoke('save_tab_as', { tabId: ${tabId}, newPath: ${JSON.stringify(saveAsFile)}, targetEncoding: null, makeBackup: false }); return { ok: true, name: v.tab.name, path: v.tab.path }; } catch (e) { return { ok: false, err: String(e && e.message ? e.message : e) }; } })()`,
    );
    const savedContent = existsSync(saveAsFile) ? readFileSync(saveAsFile, 'utf8') : null;
    check(
      'F13 另存为（新文件 + 标签重定向）',
      saveAsResult?.ok === true &&
        saveAsResult.name === 'renamed.txt' &&
        savedContent === originalText,
      JSON.stringify(saveAsResult ?? {}),
    );

    // F14：脏态重新加载（确认后丢弃修改）
    currentStep = 'F14 脏态重新加载';
    await focusProxy();
    await client.send('Input.insertText', { text: 'W' });
    const wAdded = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty === true ? true : null;
    }, 6000);
    await menuClick('文件', '重新加载');
    const dialogSeen = await waitForValue(
      async () =>
        (await evalJs(
          `document.querySelector('[role="alertdialog"]')?.textContent?.includes('重新加载') === true`,
        )) || null,
      5000,
    );
    await evalJs(
      `(() => { const dlg = document.querySelector('[role="alertdialog"]'); const btn = [...dlg.querySelectorAll('button')].find((b) => b.textContent.trim() === '重新加载'); btn?.click(); return true; })()`,
    );
    const r14 = await waitForValue(async () => {
      const tab = await activeTab();
      const text = await rowText(0);
      return tab?.dirty === false && tab?.editing === false && text === 'alpha needle beta'
        ? true
        : null;
    }, 8000);
    check(
      'F14 脏态重新加载（确认丢弃）',
      wAdded !== null && dialogSeen === true && r14 === true,
    );

    // F15：正则模式（切换 `.*` → ne+dle 命中并产生文档高亮）
    currentStep = 'F15 正则查找与高亮';
    await evalJs(`(document.querySelector('[aria-label="切换编辑模式"]')?.click(), true)`);
    await waitForValue(async () => ((await activeTab())?.editing ? true : null), 8000);
    await focusProxy();
    await ctrlKey('f', 'KeyF', 70);
    await waitForValue(
      async () => (await evalJs(`document.querySelector('.find-bar') !== null`)) || null,
      5000,
    );
    await evalJs(
      `(() => { const b = [...document.querySelectorAll('.find-bar button')].find((n) => n.textContent.trim() === '.*'); b?.click(); return !!b; })()`,
    );
    await focusFindInput();
    await client.send('Input.insertText', { text: 'ne+dle' });
    await delay(500); // 高亮防抖 150ms
    const highlightCount = await waitForValue(async () => {
      const count = await evalJs(`document.querySelectorAll('.match').length`);
      return count > 0 ? count : null;
    }, 6000);
    await clickBarButton('下一个');
    const regexSel = await waitForValue(async () => {
      const rects = await evalJs(`document.querySelectorAll('.selection').length`);
      return rects > 0 ? rects : null;
    }, 6000);
    check(
      'F15 正则模式（ne+dle 命中 + 文档高亮）',
      highlightCount !== null && regexSel !== null,
      `match=${highlightCount} sel=${regexSel}`,
    );

    // F16：无效正则 → 明确提示
    currentStep = 'F16 无效正则提示';
    await focusFindInput();
    await client.send('Input.insertText', { text: '(' });
    await clickBarButton('下一个');
    const invalidToast = await waitForValue(async () => {
      const t = await toastText();
      return t.includes('正则表达式无效') ? t : null;
    }, 6000);
    check('F16 无效正则提示', invalidToast !== null, invalidToast ?? '(超时)');

    // F17：全部替换 ≥2 命中 → 预览弹窗（行号/原文高亮/替换后文本）；剔除一条后仅替换勾选项
    currentStep = 'F17 预览确认与剔除';
    await focusFindInput();
    await client.send('Input.insertText', { text: 'needle' });
    await evalJs(
      `(() => { const b = [...document.querySelectorAll('.find-bar button')].find((n) => n.textContent.trim() === '.*'); b?.click(); return !!b; })()`,
    );
    // 查找模式（Ctrl+F）打开时没有替换输入框：经编辑菜单切到替换形态
    await menuClick('编辑', '替换…');
    await waitForValue(async () => {
      const count = await evalJs(`document.querySelectorAll('.find-bar input').length`);
      return count === 2 ? count : null;
    }, 5000);
    await evalJs(
      `(() => { const inputs = document.querySelectorAll('.find-bar input'); inputs[1]?.focus(); inputs[1]?.select(); return true; })()`,
    );
    await client.send('Input.insertText', { text: 'FIXED' });
    await clickBarButton('全部替换');
    const previewSeen = await waitForValue(async () => {
      const n = await evalJs(
        `document.querySelector('[aria-label="全部替换预览"]') !== null`,
      );
      return n === true ? true : null;
    }, 8000);
    const previewHighlight = await evalJs(
      `document.querySelectorAll('[aria-label="全部替换预览"] mark.old').length + document.querySelectorAll('[aria-label="全部替换预览"] mark.new').length`,
    );
    // 取消勾选第一条（从待替换集合中“删除”）
    await evalJs(
      `(() => { const boxes = document.querySelectorAll('[aria-label="全部替换预览"] input[type=checkbox]'); boxes[0]?.click(); return boxes.length; })()`,
    );
    const pickedLabel = await evalJs(
      `(() => { const b = [...document.querySelectorAll('[aria-label="全部替换预览"] button')].find((n) => n.textContent.includes('替换已选')); return b ? b.textContent.trim() : ''; })()`,
    );
    await evalJs(
      `(() => { const b = [...document.querySelectorAll('[aria-label="全部替换预览"] button')].find((n) => n.textContent.includes('替换已选')); b?.click(); return !!b; })()`,
    );
    const r17 = await waitForValue(async () => {
      const t0 = await rowText(0);
      const t1 = await rowText(1);
      const t3 = await rowText(3);
      // 剔除的 row0 保持 'needle'；row1 的 'Needle' 与 row3 的 'needle' 被替换
      return t0 === 'alpha needle beta' && t1 === 'FIXED here' && t3 === 'FIXED again'
        ? true
        : null;
    }, 8000);
    const toast17 = await toastText();
    check(
      'F17 预览剔除后仅替换勾选项',
      previewSeen === true &&
        previewHighlight >= 6 &&
        pickedLabel.includes('替换已选') &&
        r17 === true &&
        toast17.includes('已替换'),
      `marks=${previewHighlight} label=${pickedLabel}`,
    );

    // F18：单步撤销还原全部替换（两处）
    currentStep = 'F18 撤销恢复';
    await focusProxy();
    await ctrlKey('z', 'KeyZ', 90);
    const r18 = await waitForValue(async () => {
      const t1 = await rowText(1);
      const t3 = await rowText(3);
      return t1 === 'Needle here' && t3 === 'needle again' ? true : null;
    }, 8000);
    check('F18 单步撤销还原全部替换', r18 === true);

    // F20：匹配计数（大小写不敏感 → 3 处）
    currentStep = 'F20 匹配计数';
    await focusFindInput();
    await client.send('Input.insertText', { text: 'needle' });
    // 确保大小写不敏感（Aa 关闭）
    await evalJs(
      `(() => { const b = document.querySelector('.find-bar button[aria-label="区分大小写"]'); if (b?.getAttribute('aria-pressed') === 'true') b.click(); return true; })()`,
    );
    const count3 = await waitForValue(async () => {
      const text = await evalJs(`document.querySelector('[data-find-count]')?.textContent ?? ''`);
      return text.includes('3 处匹配') ? text : null;
    }, 8000);
    check('F20 匹配计数显示 3 处', typeof count3 === 'string', String(count3));

    // F21：全词匹配（开 W：'need' 不命中 'needle'；关 W：命中并选中）
    currentStep = 'F21 全词匹配';
    await evalJs(`(document.querySelector('[data-find-whole-word]')?.click(), true)`);
    await focusFindInput();
    await client.send('Input.insertText', { text: 'need' });
    await clickBarButton('下一个');
    const wwToast = await waitForValue(async () => {
      const toast = await toastText();
      return toast.includes('未找到') ? toast : null;
    }, 8000);
    await evalJs(`(document.querySelector('[data-find-whole-word]')?.click(), true)`);
    await clickBarButton('下一个');
    const wwHit = await waitForValue(async () => {
      const boxes = await evalJs(`document.querySelectorAll('.selection').length`);
      return boxes > 0 ? boxes : null;
    }, 8000);
    check('F21 全词开关生效（关闭后命中选中）', wwToast !== null && wwHit !== null, `toast=${wwToast}`);

    // F22：查找历史（记录 + 下拉选取 + 清空）
    currentStep = 'F22 查找历史';
    await evalJs(`(document.querySelector('[data-find-history]')?.click(), true)`);
    const historyItems = await waitForValue(async () => {
      const items = await evalJs(
        `[...document.querySelectorAll('[data-find-history-item]')].map((n) => n.textContent.trim())`,
      );
      return Array.isArray(items) && items.includes('need') && items.includes('needle') ? items : null;
    }, 8000);
    check('F22a 历史记录包含已查词', Array.isArray(historyItems), JSON.stringify(historyItems));
    await evalJs(
      `(() => { const it = [...document.querySelectorAll('[data-find-history-item]')].find((n) => n.textContent.trim() === 'need'); it?.click(); return true; })()`,
    );
    const picked = await waitForValue(async () => {
      const value = await evalJs(`document.querySelector('.find-bar input')?.value ?? ''`);
      return value === 'need' ? value : null;
    }, 5000);
    check('F22b 选取历史回填查询', picked === 'need', String(picked));
    // F22b 选取历史项后面板可能已自动收起：先按需打开，并等「清空」按钮渲染就绪再点击
    // （避免开关竞态：面板已开时再点开关会把它关上，导致清空点击落空——曾致 F22c 偶发失败）
    const panelOpen = await evalJs(`document.querySelector('[data-find-history-clear]') !== null`);
    if (!panelOpen) {
      await evalJs(`(document.querySelector('[data-find-history]')?.click(), true)`);
      await waitForValue(
        async () => ((await evalJs(`document.querySelector('[data-find-history-clear]') !== null`)) ? true : null),
        3000,
      );
    }
    await evalJs(`(document.querySelector('[data-find-history-clear]')?.click(), true)`);
    const historyEmpty = await waitForValue(async () => {
      const text = await evalJs(`document.querySelector('.history-panel')?.textContent ?? ''`);
      return text.includes('暂无历史') ? text : null;
    }, 5000);
    check('F22c 清空历史显示空态', historyEmpty !== null);
    await evalJs(`(document.querySelector('[data-find-history]')?.click(), true)`);

    // F23：范围=指定行（计数过滤为 1）+ 范围内命中可选中
    currentStep = 'F23 行范围';
    await focusFindInput();
    await client.send('Input.insertText', { text: 'needle' });
    await evalJs(
      `(() => { const s = document.querySelector('[data-find-scope]'); s.value = 'rowRange'; s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`,
    );
    // 等待行范围输入框渲染（Svelte 更新异步）
    await waitForValue(
      async () => ((await evalJs(`document.querySelector('[data-find-range-from]') !== null`)) || null),
      5000,
    );
    await evalJs(
      `(() => { const from = document.querySelector('[data-find-range-from]'); from.value = '2'; from.dispatchEvent(new Event('input', { bubbles: true })); const to = document.querySelector('[data-find-range-to]'); to.value = '2'; to.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`,
    );
    const count1 = await waitForValue(async () => {
      const text = await evalJs(`document.querySelector('[data-find-count]')?.textContent ?? ''`);
      return text.includes('1 处匹配') ? text : null;
    }, 8000);
    check('F23a 行范围计数过滤为 1 处', typeof count1 === 'string', String(count1));
    await clickBarButton('下一个');
    const rangeHit = await waitForValue(async () => {
      const selected = await evalJs(`document.querySelectorAll('.selection').length`);
      return selected > 0 ? selected : null;
    }, 8000);
    check('F23b 范围内命中选中', rangeHit !== null);
    // 恢复整文档范围，避免影响后续关闭态
    await evalJs(
      `(() => { const s = document.querySelector('[data-find-scope]'); s.value = 'document'; s.dispatchEvent(new Event('change', { bubbles: true })); return true; })()`,
    );

    // F24：设置页「查找/正则」新控件可见（颜色行 + 字符串列表行）
    currentStep = 'F24 设置页查找控件';
    await evalJs(`window.__TAURI_INTERNALS__.invoke('open_settings', { tab: 'editor' })`);
    const settingsWs = await findTarget(port, 'settings.html');
    const settingsClient = await createClient(settingsWs);
    const evalSettings = async (expression) => {
      const result = await settingsClient.send('Runtime.evaluate', {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
      if (result.exceptionDetails) {
        throw new Error(`设置页执行异常：${result.exceptionDetails.text}`);
      }
      return result.result?.value;
    };
    const colorRow = await waitForValue(async () => {
      const found = await evalSettings(
        `document.querySelector('.color-text[data-setting="app.find.highlightColor"]') !== null`,
      );
      return found === true ? true : null;
    }, 10000);
    const listRow = await evalSettings(
      `document.querySelector('[data-stringlist-add]') !== null || document.querySelector('[data-setting="app.regex.library"]') !== null`,
    );
    check('F24 设置页颜色行与列表行可见', colorRow === true && listRow === true, `color=${colorRow} list=${listRow}`);
    // 关闭设置窗（fire-and-forget：窗口销毁后响应不会到达）
    await evalSettings(
      `(window.__TAURI_INTERNALS__.invoke('plugin:window|close', { label: 'settings' }), true)`,
    );
    await delay(300);
    settingsClient.close();

    // F19：关闭查找条（清理交互态）
    currentStep = 'F19 关闭查找条';
    await evalJs(
      `(() => { const b = [...document.querySelectorAll('.find-bar button')].find((n) => n.textContent.trim() === '×'); b?.click(); return !!b; })()`,
    );
    const findClosed = await waitForValue(
      async () => ((await evalJs(`document.querySelector('.find-bar') === null`)) || null),
      5000,
    );
    const matchCleared = await evalJs(`document.querySelectorAll('.match').length`);
    check('F19 关闭查找条并清除高亮', findClosed === true && matchCleared === 0);
  } finally {
    client?.close();
    if (child.pid) {
      // 仅回收本应用进程树（/T 连带其 WebView2 子进程）；严禁按 msedgewebview2 名称杀进程
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    }
    for (const entry of ['find-sample.txt', 'renamed.txt', 'find-sample.txt.bak']) {
      await removeWithRetry(join(workDir, entry));
    }
    await removeWithRetry(runDataDir);
  }

  clearTimeout(watchdog);
  const failed = checks.filter((item) => !item.passed);
  console.log(`\n查找/替换冒烟结果：${checks.length - failed.length}/${checks.length} 通过`);
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((error) => {
  clearTimeout(watchdog);
  console.error(`查找/替换冒烟失败（${currentStep}）：${error?.message ?? error}`);
  process.exit(3);
});
