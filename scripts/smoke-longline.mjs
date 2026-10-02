#!/usr/bin/env node
// 超长行对抗验收：100MB 单行文件（无任何换行）在编辑态按 8KB 显示分段，
// 验证「打开 → 滚动 → 进入编辑 → 文档末输入 → 保存」全链路与内存红线口径。
//
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-longline.mjs [--exe <路径>] [--size-mb 100] [--screenshot <路径>]
// 说明：文件生成于系统临时目录（非仓库）；每次运行使用独立数据目录；结束整树回收。

import { spawn, spawnSync } from 'node:child_process';
import { closeSync, existsSync, mkdirSync, openSync, readSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget, openPathDone, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const port = Number(argValue('--port', String(9200 + Math.floor(Math.random() * 600))));
const sizeMb = Number(argValue('--size-mb', '100'));
const screenshotPath = argValue('--screenshot', join(root, 'docs', 'screenshots', 'phase4c-longline.png'));

const workDir = join(process.env.TEMP ?? '.', 'srt-smoke-longline');
const testFile = join(workDir, 'single-line.txt');
const runDataDir = join(workDir, `data-${Date.now()}`);
/** 看门狗：超过该秒数未完成则打印最后步骤并退出（绝不无限期挂起） */
const watchdogSec = Number(argValue('--watchdog', '240'));
let currentStep = '初始化';
const watchdog = setTimeout(() => {
  console.error(`看门狗超时（${watchdogSec}s）：最后步骤 = ${currentStep}`);
  process.exit(4);
}, watchdogSec * 1000);
watchdog.unref();
/** 8KB 显示段常量（与后端 DISPLAY_SEGMENT_BYTES 对齐） */
const SEGMENT = 8192;

const checks = [];
function check(name, passed, detail = '') {
  checks.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

/** 读取文件末尾 n 字节（100MB 文件不能用 readFileSync 全读）。 */
function readTail(path, bytes) {
  const fd = openSync(path, 'r');
  try {
    const size = statSync(path).size;
    const length = Math.min(bytes, size);
    const buffer = Buffer.alloc(length);
    readSync(fd, buffer, 0, length, size - length);
    return buffer.toString('utf8');
  } finally {
    closeSync(fd);
  }
}

/** 带重试的删除：应用进程退出后文件句柄释放/杀软扫描可能短暂锁定，直接删会静默失败。 */
async function removeWithRetry(path, options, attempts = 12) {
  for (let i = 0; i < attempts; i += 1) {
    try {
      rmSync(path, options);
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
  // 自愈：清掉历史遗留的运行数据目录
  for (const entry of readdirSync(workDir)) {
    if (entry.startsWith('data-')) {
      rmSync(join(workDir, entry), { recursive: true, force: true });
    }
  }
  rmSync(testFile, { force: true });
  rmSync(`${testFile}.bak`, { force: true });
  const totalBytes = sizeMb * 1024 * 1024;
  currentStep = '生成单行文件';
  console.log(`生成单行文件：${totalBytes} 字节（约 ${sizeMb}MB，无换行）…`);
  writeFileSync(testFile, Buffer.alloc(totalBytes, 'a'));
  const expectedRows = Math.ceil(totalBytes / SEGMENT);

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

    // 就绪（明确护栏：未就绪直接失败）
    currentStep = '等待 CDP 前端就绪';
    let ready = false;
    for (let attempt = 0; attempt < 180; attempt += 1) {
      ready = (await evalJs('!!window.__srt?.openPath && !!window.__TAURI_INTERNALS__')) === true;
      if (ready) break;
      await delay(250);
    }
    if (!ready) throw new Error('前端未就绪（window.__srt 未注入）');
    await dismissOnboarding(evalJs);

    // C1/C2：打开并核对显示分段行数与前段文本
    currentStep = 'C1 打开文件';
    await evalJs(openPathDone(testFile));
    const opened = await waitForValue(async () => {
      const state = await evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); const t = v.tabs[0]; return { rows: t?.rowsTotal ?? 0, encoding: t?.encoding ?? '' }; })()`,
      );
      return state?.rows === expectedRows ? state : null;
    }, 20000);
    check(
      'C1 打开 100MB 单行（显示分段行数精确）',
      opened !== null,
      `rows=${opened?.rows ?? '(超时)'} 期望=${expectedRows} 编码=${opened?.encoding ?? ''}`,
    );
    const firstRow = await waitForValue(async () => {
      const text = await evalJs(`document.querySelector('.row')?.textContent ?? ''`);
      return text.length === SEGMENT ? text : null;
    }, 10000);
    check(
      'C2 首段文本恰为 8KB',
      firstRow !== null,
      firstRow ? `len=${firstRow.length}` : '(超时)',
    );

    // C3：滚动到中部仍能渲染（虚拟滚动 + 分段取窗）
    await evalJs(
      `(() => { const el = document.querySelector('.reader'); el.scrollTop = el.scrollHeight / 2; return true; })()`,
    );
    const middle = await waitForValue(async () => {
      const state = await evalJs(
        `(() => { const rows = [...document.querySelectorAll('.row[data-row]')]; const hit = rows.find((n) => n.textContent.length === ${SEGMENT} && Number(n.dataset.row) > 100); return hit ? { row: Number(hit.dataset.row), len: hit.textContent.length } : null; })()`,
      );
      return state;
    }, 10000);
    check('C3 滚动中部渲染正常', middle !== null, JSON.stringify(middle ?? {}));

    // C4：状态栏显示编码（编码文本 = UTF-8）
    const status = await evalJs(`document.querySelector('.status-bar')?.textContent ?? ''`);
    check('C4 状态栏显示编码', typeof status === 'string' && status.includes('UTF-8'), status.slice(0, 60));

    // C5/C6：进入编辑，Ctrl+End 到文档末并输入
    currentStep = 'C5 进入编辑';
    await evalJs(`(document.querySelector('[aria-label="切换编辑模式"]')?.click(), true)`);
    const editing = await waitForValue(async () => {
      const tab = await evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return v.tabs[0]; })()`,
      );
      return tab?.editing ? tab : null;
    }, 8000);
    check('C5 进入编辑模式', editing !== null);
    await client.send('Input.dispatchKeyEvent', {
      type: 'rawKeyDown',
      key: 'End',
      code: 'End',
      windowsVirtualKeyCode: 35,
      nativeVirtualKeyCode: 35,
      modifiers: 2,
    });
    await client.send('Input.dispatchKeyEvent', {
      type: 'keyUp',
      key: 'End',
      code: 'End',
      windowsVirtualKeyCode: 35,
      nativeVirtualKeyCode: 35,
      modifiers: 2,
    });
    await delay(300);
    currentStep = 'C6 文档末输入';
    await client.send('Input.insertText', { text: 'X' });
    const dirty = await waitForValue(async () => {
      const tab = await evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return v.tabs[0]; })()`,
      );
      return tab?.dirty ? tab : null;
    }, 8000);
    check('C6 文档末输入变脏', dirty !== null, `byteLen=${dirty?.byteLen ?? '?'}`);

    // C7：保存（编码询问弹窗 → 保存）并核对磁盘增量
    currentStep = 'C7 保存';
    await evalJs(`(document.querySelector('[aria-label="保存"]')?.click(), true)`);
    const dialogSeen = await waitForValue(
      async () => (await evalJs(`document.querySelector('[role="dialog"]') !== null`)) || null,
      5000,
    );
    check('C7a 保存弹窗打开', dialogSeen === true);
    await evalJs(
      `(() => { const dlg = document.querySelector('[role="dialog"]'); const btn = [...dlg.querySelectorAll('button')].find((b) => b.textContent.trim() === '保存'); btn?.click(); return true; })()`,
    );
    const saved = await waitForValue(async () => {
      const tab = await evalJs(
        `(async () => { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return v.tabs[0]; })()`,
      );
      if (tab?.dirty) return null;
      const size = statSync(testFile).size;
      return { size, tail: readTail(testFile, 4) };
    }, 120000);
    check(
      'C7b 保存后磁盘 = 原大小 + 1 且尾部 aX',
      saved !== null && saved.size === totalBytes + 1 && saved.tail.endsWith('aX'),
      JSON.stringify(saved ?? {}),
    );

    // 截图（编辑态、文档末）
    currentStep = 'C8 截图';
    try {
      const shot = await client.send('Page.captureScreenshot', { format: 'png' });
      mkdirSync(dirname(screenshotPath), { recursive: true });
      writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'));
      check('C8 截图已保存', true, screenshotPath);
    } catch (error) {
      check('C8 截图已保存', false, String(error));
    }
  } finally {
    clearTimeout(watchdog);
    client?.close();
    if (child.pid) {
      // 仅回收本应用进程树（/T 连带其 WebView2 子进程）；严禁按 msedgewebview2 名称杀进程
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    }
    // 带重试清理：taskkill 后句柄释放有延迟，直接删会静默失败留下大文件
    await removeWithRetry(runDataDir, { recursive: true, force: true });
    await removeWithRetry(testFile, { force: true });
    await removeWithRetry(`${testFile}.bak`, { force: true });
  }

  const failed = checks.filter((item) => !item.passed);
  console.log(`\n超长行验收结果：${checks.length - failed.length}/${checks.length} 通过`);
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((error) => {
  console.error(`超长行验收失败：${error?.message ?? error}`);
  process.exit(3);
});
