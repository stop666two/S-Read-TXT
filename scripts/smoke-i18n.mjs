#!/usr/bin/env node
// 多语言 E2E（smoke-i18n）：阅读器面对“全世界的文本”时的核心链路。
// 覆盖：8 语言内容（英/中/日/韩/俄/阿/希伯来/emoji ZWJ）的取行与渲染；
//       西里尔大小写不敏感查找；中/繁替换与撤销；UTF-8 全选替换后保存字节一致；
//       Shift_JIS / EUC-KR / Big5 / windows-1252 自动检测、切换编码、显示还原；
//       windows-1252 编辑后保存字节与“预期文件”逐字节一致。
//
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-i18n.mjs [--exe <路径>] [--port 9229]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createDialogOps } from './lib/dialog.mjs';
import { argValue, createClient, delay, findTarget, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const port = Number(argValue('--port', String(9200 + Math.floor(Math.random() * 600))));
const watchdogMs = Number(process.env.SRT_SMOKE_WATCHDOG_MS ?? '240000');

const workDir = join(process.env.TEMP ?? '.', 'srt-smoke-i18n');
const utf8File = join(workDir, 'utf8-langs.txt');
const sjisFile = join(workDir, 'sjis.txt');
const euckrFile = join(workDir, 'euckr.txt');
const big5File = join(workDir, 'big5.txt');
const cp1252File = join(workDir, 'cp1252.txt');
const cp1252ExpectedFile = join(workDir, 'cp1252-expected.bin');
const runDataDir = join(workDir, `data-${Date.now()}`);

// ---- 多语言数据 ----

/** 8 语言内容（UTF-8；含 RTL 与 ZWJ emoji） */
const UTF8_LINES = [
  'English: The quick brown fox jumps over the lazy dog.',
  '中文：这是一个用于多语言测试的中文句子。',
  '日本語：これは多言語テストの日本語文です。',
  '한국어: 이것은 다국어 테스트 한국어 문장입니다.',
  'Русский: Это русское предложение для теста.',
  'العربية: هذه جملة عربية لاختبار متعدد اللغات.',
  'עברית: זהו משפט עברי לבדיקת רב-לשוניות.',
  'Emoji: 👨‍👩‍👧‍👦 🎌 ✨',
];
const UTF8_CONTENT = `${UTF8_LINES.join('\n')}\n`;
const MIXED = '混合Mixed混合';
const MIXED_EXPECTED = `${MIXED}\n`;

const SJIS_CONTENT = '吾輩は猫である。名前はまだ無い。これは日本語のテスト文章です。\n'.repeat(6);
const EUCKR_CONTENT = '이것은 한국어 인코딩 테스트 문장입니다. 안녕하세요.\n'.repeat(6);
const BIG5_CONTENT = '這是一個繁體中文編碼測試句子。你好，世界。\n'.repeat(6);
const CP1252_CONTENT = 'café naïve résumé — déjà vu — 2 €\n'.repeat(6);
/** cp1252 编辑预期：Ctrl+End（末行行末、尾换行之前）插入 " é" */
const CP1252_EXPECTED = `${CP1252_CONTENT.slice(0, -1)} é\n`;

const checks = [];
let currentStep = 'Q0 启动';
function check(name, passed, detail = '') {
  checks.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

const watchdog = setTimeout(() => {
  console.error(`看门狗超时（${watchdogMs}ms），最后步骤：${currentStep}`);
  process.exit(4);
}, watchdogMs);

/** PowerShell 生成各编码样本（内容经 base64 环境变量传入，规避引号/编码问题） */
const PS_GEN = `
$dir = $env:SRT_I18N_DIR
function Write-Enc([string]$name, [int]$cp, [string]$b64) {
  if ([string]::IsNullOrEmpty($b64)) { return }
  $text = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($b64))
  [IO.File]::WriteAllText((Join-Path $dir $name), $text, [Text.Encoding]::GetEncoding($cp))
}
Write-Enc 'sjis.txt' 932 $env:SRT_I18N_SJIS
Write-Enc 'euckr.txt' 949 $env:SRT_I18N_EUCKR
Write-Enc 'big5.txt' 950 $env:SRT_I18N_BIG5
Write-Enc 'cp1252.txt' 1252 $env:SRT_I18N_CP1252
Write-Enc 'cp1252-expected.bin' 1252 $env:SRT_I18N_EXPECTED
'OK'
`;

function generateEncodedSamples() {
  const result = spawnSync('powershell', ['-NoProfile', '-Command', PS_GEN], {
    encoding: 'utf8',
    timeout: 30000,
    env: {
      ...process.env,
      SRT_I18N_DIR: workDir,
      SRT_I18N_SJIS: Buffer.from(SJIS_CONTENT, 'utf8').toString('base64'),
      SRT_I18N_EUCKR: Buffer.from(EUCKR_CONTENT, 'utf8').toString('base64'),
      SRT_I18N_BIG5: Buffer.from(BIG5_CONTENT, 'utf8').toString('base64'),
      SRT_I18N_CP1252: Buffer.from(CP1252_CONTENT, 'utf8').toString('base64'),
      SRT_I18N_EXPECTED: Buffer.from(CP1252_EXPECTED, 'utf8').toString('base64'),
    },
  });
  if (!(result.stdout ?? '').includes('OK')) {
    throw new Error(`编码样本生成失败：${result.stderr ?? result.stdout ?? '未知错误'}`);
  }
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

async function main() {
  if (!existsSync(exePath)) {
    console.error(`可执行文件不存在：${exePath}（先运行 npm run tauri build -- --debug --no-bundle）`);
    process.exit(2);
  }
  mkdirSync(workDir, { recursive: true });
  writeFileSync(utf8File, UTF8_CONTENT, 'utf8');
  generateEncodedSamples();

  const child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: runDataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });
  const { dialogOp: _dialogOp, waitDialog: _waitDialog } = createDialogOps(child.pid); // 预留：本套件当前不涉及原生对话框

  let client;
  try {
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
      const ready = await evalJs(
        '(() => !!(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke && window.__srt && window.__srt.openPath))()',
      );
      return ready ? true : null;
    }, 30000);

    /** CDP 键盘注入（modifiers：Ctrl=2 Shift=8） */
    const press = async (key, code, vk, modifiers = 0) => {
      const base = { key, code, windowsVirtualKeyCode: vk, nativeVirtualKeyCode: vk, modifiers };
      await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...base });
      await client.send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
      await delay(150);
    };
    const activeTab = () =>
      evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return v.tabs.find((t) => t.tabId === v.activeTabId) ?? null; })()`,
      );
    const rowsOf = async (tabId) => {
      const payload = await evalJs(
        `(async () => await window.__TAURI_INTERNALS__.invoke('get_rows', { tabId: ${tabId}, startRow: 0, count: 200 }))()`,
      );
      return payload.rows.map((row) => row.text);
    };
    const openAndWait = async (path) => {
      await evalJs(`window.__srt.openPath(${JSON.stringify(path)})`);
      await delay(500);
      return activeTab();
    };
    const enterEdit = async () => {
      await press('e', 'KeyE', 69, 2);
      await waitForValue(async () => ((await activeTab())?.editing === true ? true : null), 5000);
      await waitForValue(
        async () => ((await evalJs(`!!document.querySelector('textarea.input-proxy')`)) ? true : null),
        5000,
      );
      await evalJs(`(document.querySelector('textarea.input-proxy')?.focus(), true)`);
    };
    /** 通过保存弹窗保存（保持当前编码） */
    const saveViaDialog = async () => {
      await evalJs(`document.querySelector('[aria-label="保存"]')?.click() ?? true`);
      const opened = await waitForValue(
        async () => ((await evalJs(`document.querySelector('[role="dialog"]') !== null`)) ? true : null),
        4000,
      );
      if (!opened) return false;
      await evalJs(
        `(() => { const dlg = document.querySelector('[role="dialog"]'); const btn = [...dlg.querySelectorAll('button')].find((b) => b.textContent.trim() === '保存'); btn?.click(); return !!btn; })()`,
      );
      return true;
    };
    const setFindInputs = (query, replacement) =>
      evalJs(
        `(() => { const inputs = [...document.querySelectorAll('.find-bar input')]; if (inputs[0]) { inputs[0].value = ${JSON.stringify(query)}; inputs[0].dispatchEvent(new Event('input', { bubbles: true })); } if (${JSON.stringify(replacement ?? null)} !== null && inputs[1]) { inputs[1].value = ${JSON.stringify(replacement ?? '')}; inputs[1].dispatchEvent(new Event('input', { bubbles: true })); } return inputs.length; })()`,
      );

    // ---- I1/I2：UTF-8 多语言文件 ----
    currentStep = 'I1 打开 UTF-8 多语言文件';
    const tabU = await openAndWait(utf8File);
    check('I1a 编码识别为 UTF-8', tabU?.encoding === 'UTF-8', tabU?.encoding);
    const rowsU = await rowsOf(tabU.tabId);
    check('I1b 8 行全部取回', rowsU.length === 8, `rows=${rowsU.length}`);
    check(
      'I1c 逐行内容与原文一致（含 RTL 与 emoji）',
      JSON.stringify(rowsU) === JSON.stringify(UTF8_LINES),
      rowsU[5] ?? '',
    );
    const domRow2 = await evalJs(`(document.querySelector('.row[data-row="2"]')?.textContent ?? '').includes('日本語')`);
    check('I1d 日文行已渲染到界面', domRow2 === true);
    check(
      'I2 状态栏显示 UTF-8',
      await evalJs(`(document.querySelector('.status-bar')?.textContent ?? '').includes('UTF-8')`),
    );

    // ---- I4：西里尔大小写不敏感查找 ----
    currentStep = 'I4 西里尔查找';
    await enterEdit();
    await press('f', 'KeyF', 70, 2);
    await waitForValue(async () => ((await evalJs(`!!document.querySelector('.find-bar')`)) ? true : null), 5000);
    await setFindInputs('русское', null);
    await press('Enter', 'Enter', 13);
    const cyrLower = await waitForValue(
      async () => ((await evalJs(`document.querySelectorAll('.selection').length`)) > 0 ? true : null),
      5000,
    );
    check('I4a 小写西里尔查询命中并渲染选区', cyrLower === true);
    await setFindInputs('РУССКОЕ', null);
    await press('Enter', 'Enter', 13);
    const cyrUpper = await waitForValue(
      async () => ((await evalJs(`document.querySelectorAll('.selection').length`)) > 0 ? true : null),
      5000,
    );
    check('I4b 大写西里尔查询同样命中（大小写不敏感）', cyrUpper === true);

    // ---- I5：中文替换与撤销 ----
    currentStep = 'I5 替换与撤销';
    await press('h', 'KeyH', 72, 2); // Ctrl+H 替换模式
    await waitForValue(async () => ((await evalJs(`document.querySelectorAll('.find-bar input').length >= 2`)) ? true : null), 5000);
    await setFindInputs('测试', '測試');
    await evalJs(
      `(() => { const bar = document.querySelector('.find-bar'); const btn = [...bar.querySelectorAll('button')].find((b) => b.textContent.trim() === '全部替换'); btn?.click(); return !!btn; })()`,
    );
    const replaced = await waitForValue(async () => {
      const rows = await rowsOf(tabU.tabId);
      return rows.some((text) => text.includes('測試')) ? true : null;
    }, 6000);
    check('I5a 全部替换生效（简→繁）', replaced === true);
    check('I5b 替换后为脏态', (await activeTab())?.dirty === true);
    await press('z', 'KeyZ', 90, 2); // Ctrl+Z 撤销（单撤销步）
    const undone = await waitForValue(async () => {
      const rows = await rowsOf(tabU.tabId);
      return rows.some((text) => text.includes('测试')) && !(await activeTab())?.dirty ? true : null;
    }, 6000);
    check('I5c 撤销后还原且回到干净态', undone === true);
    await press('Escape', 'Escape', 27);

    // ---- I3：全选替换（混合语言）→ 保存 → 字节级一致 ----
    currentStep = 'I3 UTF-8 保存字节一致';
    await press('a', 'KeyA', 65, 2); // Ctrl+A（编辑层全选）
    await client.send('Input.insertText', { text: MIXED });
    const mixedDirty = await waitForValue(async () => ((await activeTab())?.dirty === true ? true : null), 5000);
    check('I3a 全选替换后为脏态', mixedDirty === true);
    check('I3b 保存弹窗确认', await saveViaDialog());
    await waitForValue(async () => ((await activeTab())?.dirty === false ? true : null), 6000);
    const savedBytes = readFileSync(utf8File);
    check(
      'I3c 磁盘字节与预期 UTF-8 完全一致',
      savedBytes.equals(Buffer.from(MIXED_EXPECTED, 'utf8')),
      savedBytes.toString('hex').slice(0, 40),
    );
    await press('e', 'KeyE', 69, 2); // 退出编辑，保持干净
    await waitForValue(async () => ((await activeTab())?.editing === false ? true : null), 5000);

    // ---- I6/I7：Shift_JIS ----
    currentStep = 'I6 Shift_JIS 检测与显示';
    const tabJ = await openAndWait(sjisFile);
    check('I6a 编码识别为 Shift_JIS', tabJ?.encoding === 'Shift_JIS', tabJ?.encoding);
    const rowsJ = await rowsOf(tabJ.tabId);
    check(
      'I6b 日文内容正确显示',
      rowsJ[0] === SJIS_CONTENT.split('\n')[0],
      (rowsJ[0] ?? '').slice(0, 24),
    );
    await evalJs(
      `(async () => await window.__TAURI_INTERNALS__.invoke('set_encoding', { tabId: ${tabJ.tabId}, encoding: 'GB18030' }))()`,
    );
    await delay(400);
    const rowsGarbled = await rowsOf(tabJ.tabId);
    check('I7a 切换 GB18030 后内容乱码（与原文字不同）', rowsGarbled[0] !== SJIS_CONTENT.split('\n')[0]);
    await evalJs(
      `(async () => await window.__TAURI_INTERNALS__.invoke('set_encoding', { tabId: ${tabJ.tabId}, encoding: 'Shift_JIS' }))()`,
    );
    await delay(400);
    const rowsRestored = await rowsOf(tabJ.tabId);
    check('I7b 切回 Shift_JIS 后内容还原', rowsRestored[0] === SJIS_CONTENT.split('\n')[0]);

    // ---- I8：EUC-KR ----
    currentStep = 'I8 EUC-KR';
    const tabK = await openAndWait(euckrFile);
    check('I8a 编码识别为 EUC-KR', tabK?.encoding === 'EUC-KR', tabK?.encoding);
    const rowsK = await rowsOf(tabK.tabId);
    check('I8b 韩文内容正确显示', rowsK[0] === EUCKR_CONTENT.split('\n')[0], (rowsK[0] ?? '').slice(0, 20));

    // ---- I9：Big5 ----
    currentStep = 'I9 Big5';
    const tabB = await openAndWait(big5File);
    check('I9a 编码识别为 Big5', tabB?.encoding === 'Big5', tabB?.encoding);
    const rowsB = await rowsOf(tabB.tabId);
    check('I9b 繁体内容正确显示', rowsB[0] === BIG5_CONTENT.split('\n')[0], (rowsB[0] ?? '').slice(0, 20));

    // ---- I10：windows-1252 编辑 + 保存字节一致 ----
    currentStep = 'I10 windows-1252 编辑保存';
    const tabW = await openAndWait(cp1252File);
    check('I10a 编码识别为 windows-1252', tabW?.encoding === 'windows-1252', tabW?.encoding);
    const rowsW = await rowsOf(tabW.tabId);
    check('I10b 西欧文本正确显示', rowsW[0] === CP1252_CONTENT.split('\n')[0], rowsW[0] ?? '');
    await enterEdit();
    await press('End', 'End', 35, 2); // Ctrl+End → 末行行末
    await client.send('Input.insertText', { text: ' é' });
    const wDirty = await waitForValue(async () => ((await activeTab())?.dirty === true ? true : null), 5000);
    check('I10c 编辑后为脏态', wDirty === true);
    check('I10d 保存弹窗确认', await saveViaDialog());
    await waitForValue(async () => ((await activeTab())?.dirty === false ? true : null), 6000);
    const savedW = readFileSync(cp1252File);
    const expectedW = readFileSync(cp1252ExpectedFile);
    check('I10e 保存字节与预期 cp1252 文件逐字节一致', savedW.equals(expectedW), `${savedW.length}B vs ${expectedW.length}B`);
    const rowsWFinal = await rowsOf(tabW.tabId);
    check(
      'I10f 重读内容包含追加字符',
      (rowsWFinal[rowsWFinal.length - 1] ?? '').endsWith(' é'),
      rowsWFinal[rowsWFinal.length - 1] ?? '',
    );

    // 汇总
    const failed = checks.filter((item) => !item.passed);
    console.log(`\n多语言冒烟：${checks.length - failed.length}/${checks.length} 通过`);
    if (failed.length > 0) {
      console.error(`失败项：${failed.map((item) => item.name).join('；')}`);
      process.exitCode = 1;
    }
  } catch (error) {
    console.error(`多语言冒烟异常（步骤：${currentStep}）：${error?.message ?? error}`);
    process.exitCode = 1;
  } finally {
    clearTimeout(watchdog);
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    await delay(500);
    await removeWithRetry(runDataDir);
    await removeWithRetry(workDir);
    client?.close?.();
  }
}

void main();
