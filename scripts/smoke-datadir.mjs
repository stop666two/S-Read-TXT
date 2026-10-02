// S-Read-TXT「数据目录不可写」引导 E2E（阶段 6d）
// 原理：`SRT_DATA_DIR` 指向一个「文件」路径——create_dir_all 必然失败，等价于程序目录不可写。
// 断言：
//   D1 启动后出现引导弹窗（含不可写路径与失败原因）
//   D2 选择可写目录（自动化钩子等价于用户选择后应用）→ 弹窗消失、状态可写、来源 runtimeOverride
//   D3 数据真实写入新目录（打开文件 → history.jsonl；日志重定向 → logs/app.log）
//   D4 会话级语义：重启仍不可写 → 弹窗再次出现；「仅本次只读运行」→ 弹窗关闭且应用可用
//
// 用法：node scripts/smoke-datadir.mjs [--exe path]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { argValue, createClient, delay, findTarget, waitForValue } from './lib/smoke-cdp.mjs';

const exePath = argValue(
  '--exe',
  join(process.cwd(), 'src-tauri', 'target', 'debug', 's-read-txt.exe'),
);
const workDir = join(tmpdir(), 'srt-smoke-datadir');
/** 被当作「数据目录」的文件：探测目录时必然失败（等价不可写） */
const blockedPath = join(workDir, 'blocked');
/** 用户在引导中选择的可写目录 */
const okDir = join(workDir, 'ok-data');
const sampleFile = join(workDir, 'sample.txt');
const port = 9300 + Math.floor(Math.random() * 400);

let passed = 0;
let failed = 0;
const failures = [];
/** 应用进程与 CDP 客户端（阶段 1/2） */
let first = null;
let second = null;

/** 断言与结果输出（中文）。 */
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

/** 结束本应用进程树（仅按 PID：严禁按进程名杀共享的 WebView2 运行时）。 */
function killTree(pid) {
  if (!pid) return;
  spawnSync('taskkill', ['/PID', String(pid), '/T', '/F'], { stdio: 'ignore' });
}

/** 等待进程退出。 */
function waitGone(pid, timeoutMs = 20000) {
  return waitForValue(async () => {
    const probe = spawnSync(
      'powershell',
      [
        '-NoProfile',
        '-Command',
        `if (Get-Process -Id ${pid} -ErrorAction SilentlyContinue) { 'ALIVE' } else { 'GONE' }`,
      ],
      { encoding: 'utf8', timeout: 15000 },
    );
    return (probe.stdout ?? '').trim() === 'GONE' ? true : null;
  }, timeoutMs);
}

/** 启动应用（SRT_DATA_DIR=不可写路径）并建立 CDP 客户端（等待自动化钩子就绪）。 */
async function launchAndConnect() {
    const proc = spawn(exePath, [], {
      env: {
        ...process.env,
        SRT_DATA_DIR: blockedPath,
        // 自动化场景显式禁用「按需提权」：本套件用「数据目录指向文件」模拟不可写，
        // 若不禁用会触发 UAC 提权重启（交互无法自动化且与所测引导流程冲突）。
        SRT_NO_ELEVATION: '1',
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
      },
    stdio: 'ignore',
  });
  const wsUrl = await findTarget(port);
  const client = await createClient(wsUrl);
  const evalJs = async (expression) => {
    let result;
    try {
      result = await client.send('Runtime.evaluate', {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
    } catch (error) {
      throw new Error(`求值失败（${expression.slice(0, 90)}…）：${error?.message ?? error}`);
    }
    if (result.exceptionDetails) throw new Error(`页面脚本异常：${result.exceptionDetails.text}`);
    return result.result?.value;
  };
  await waitForValue(async () => {
    const ready = await evalJs(
      '(() => !!(window.__TAURI_INTERNALS__ && window.__srt && window.__srt.setDataDir))()',
    );
    return ready ? true : null;
  }, 30000);
  return { proc, client, evalJs };
}

/** 读取引导弹窗状态：不存在返回 null；存在返回 JSON（dir/msg 文本）。 */
function dialogState(evalJs) {
  return evalJs(
    `(() => { const el = document.querySelector('.ddl'); if (!el) return null;
      return JSON.stringify({ dir: el.querySelector('.ddl-dir')?.textContent ?? '', msg: el.querySelector('.ddl-msg')?.textContent ?? '' }); })()`,
  );
}

/** 重试删除（杀进程后句柄释放存在延迟）。 */
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
  rmSync(workDir, { recursive: true, force: true });
  mkdirSync(workDir, { recursive: true });
  // 关键：数据目录指向一个「文件」——目录创建必然失败
  writeFileSync(blockedPath, 'not a directory', 'utf8');
  writeFileSync(sampleFile, 'D1\nD2\nD3\n', 'utf8');

  try {
    // ---- D1：不可写引导弹窗 ----
    first = await launchAndConnect();
    const dialog = await waitForValue(async () => (await dialogState(first.evalJs)) ?? null, 15000);
    check('D1a 不可写时弹出引导', dialog !== null, dialog ?? '未出现');
    if (dialog !== null) {
      const parsed = JSON.parse(dialog);
      check('D1b 弹窗展示不可写路径', parsed.dir.includes('blocked'), parsed.dir);
      check('D1c 弹窗展示失败原因', parsed.msg.length > 0, parsed.msg);
    }

    // ---- D2：选择可写目录（钩子等价于用户经系统选择器选定后应用） ----
    await first.evalJs(`(void window.__srt.setDataDir(${JSON.stringify(okDir)}), true)`);
    const statusText = await waitForValue(async () => {
      const raw = await first.evalJs(
        `window.__TAURI_INTERNALS__.invoke('data_dir_status').then((s) => JSON.stringify(s))`,
      );
      if (typeof raw !== 'string') return null;
      return JSON.parse(raw).writable ? raw : null;
    }, 10000);
    const status = statusText ? JSON.parse(statusText) : { writable: false, origin: '?' };
    check('D2a 切换后可写', status.writable === true, `writable=${status.writable}`);
    check('D2b 来源为会话级覆盖', status.origin === 'runtimeOverride', `origin=${status.origin}`);
    const dialogGone = await waitForValue(
      async () => ((await dialogState(first.evalJs)) === null ? true : null),
      8000,
    );
    check('D2c 引导弹窗消失', dialogGone === true);

    // ---- D3：数据真实落到新目录 ----
    await first.evalJs(`(void window.__srt.openPath(${JSON.stringify(sampleFile)}), true)`);
    await waitForValue(async () => {
      const count = await first.evalJs(
        `window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs.length)`,
      );
      return count === 1 ? true : null;
    }, 10000);
    const historyLanded = await waitForValue(
      async () => (existsSync(join(okDir, 'history.jsonl')) ? true : null),
      10000,
    );
    check('D3a 历史记录写入新目录', historyLanded === true, join(okDir, 'history.jsonl'));
    check('D3b 日志重定向到新目录', existsSync(join(okDir, 'logs', 'app.log')), join(okDir, 'logs', 'app.log'));
    check('D3c 原路径仍为文件（未被误建目录）', existsSync(blockedPath));

    // ---- 阶段 1 收尾 ----
    killTree(first.proc.pid);
    await waitGone(first.proc.pid);
    first.client.close();
    first = null;

    // ---- D4：重启仍不可写（会话级语义）→ 仅本次只读运行 ----
    second = await launchAndConnect();
    const dialog2 = await waitForValue(async () => (await dialogState(second.evalJs)) ?? null, 15000);
    check('D4a 重启再次弹出（会话级语义）', dialog2 !== null, dialog2 ? '已出现' : '未出现');
    await second.evalJs(
      `(() => { const button = document.querySelector('.ddl-secondary'); if (button) button.click(); return true; })()`,
    );
    const skipped = await waitForValue(
      async () => ((await dialogState(second.evalJs)) === null ? true : null),
      8000,
    );
    check('D4b 「仅本次只读运行」关闭弹窗', skipped === true);
    const version = await second.evalJs(
      `window.__TAURI_INTERNALS__.invoke('get_app_info').then((info) => info.version)`,
    );
    check('D4c 应用仍可用（IPC 正常）', typeof version === 'string' && version.length > 0, version);

    // ---- D5：提权助手模式（真实 exe：--prepare-data-dir / --grant-sid） ----
    // 目的：验证「一次性授权」在真实 ACL 下生效——先制造只读目录，再用助手模式修复。
    const helperData = join(workDir, 'helper-data');
    mkdirSync(helperData, { recursive: true });
    const sidProbe = spawnSync(
      'powershell',
      [
        '-NoProfile',
        '-Command',
        '[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value',
      ],
      { encoding: 'utf8', timeout: 15000 },
    );
    const sid = (sidProbe.stdout ?? '').trim();
    if (!/^S-1-\d+/.test(sid)) {
      check('D5a 获取当前用户 SID', false, sid || '（空）');
    } else {
      const readonly = spawnSync(
        'icacls',
        [helperData, '/inheritance:r', '/grant:r', `*${sid}:(RX)`],
        { stdio: 'ignore', timeout: 20000 },
      );
      let writableBefore = true;
      try {
        writeFileSync(join(helperData, 'probe.txt'), 'x', 'utf8');
      } catch {
        writableBefore = false;
      }
      check('D5a 预置只读 ACL 后不可写', readonly.status === 0 && !writableBefore);

      const helperRun = spawnSync(
        exePath,
        ['--prepare-data-dir', helperData, '--grant-sid', sid],
        { encoding: 'utf8', timeout: 60000 },
      );
      check('D5b 助手模式退出码 0', helperRun.status === 0, `status=${helperRun.status}`);

      let writableAfter = true;
      try {
        writeFileSync(join(helperData, 'probe2.txt'), 'x', 'utf8');
      } catch {
        writableAfter = false;
      }
      check('D5c 授权后当前用户可写', writableAfter);
    }
  } catch (error) {
    failed += 1;
    failures.push(`异常：${error?.message ?? error}`);
    console.log(`FAIL  执行异常  ← ${error?.message ?? error}`);
  } finally {
    if (first?.proc?.pid) {
      killTree(first.proc.pid);
      await waitGone(first.proc.pid);
    }
    if (second?.proc?.pid) {
      killTree(second.proc.pid);
      await waitGone(second.proc.pid);
    }
    await removeWithRetry(workDir);
    const total = passed + failed;
    console.log(`\n数据目录引导冒烟：${passed}/${total} 通过`);
    if (failed > 0) {
      console.log(`失败项：${failures.join('；')}`);
      process.exitCode = 1;
    }
  }
}

await main();
