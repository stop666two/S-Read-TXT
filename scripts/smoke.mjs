// S-Read-TXT 运行冒烟脚本（零依赖；Node ≥22 内置 fetch/WebSocket）。
//
// 作用：启动已构建的应用（CLI 产物），通过 WebView2 远程调试端口驱动 IPC，
// 断言核心命令行为，结束时清理进程树。作为每个阶段「运行时验证」的固定工具。
//
// 用法：node scripts/smoke.mjs [--exe <路径>] [--port 9222]
// 前置：npm run tauri build -- --debug --no-bundle
// 退出码：0 = 全部通过；1 = 存在失败。

import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

const args = process.argv.slice(2);
const exeArg = args.indexOf('--exe');
const portArg = args.indexOf('--port');
const PORT = portArg >= 0 ? Number(args[portArg + 1]) : 9222;
const EXE = resolve(
  exeArg >= 0 ? args[exeArg + 1] : join('src-tauri', 'target', 'debug', 's-read-txt.exe'),
);

const results = [];
const check = (name, ok, detail = '') => {
  results.push({ name, ok });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`);
};

// 冒烟样本：UTF-8 无 BOM、混合换行、三行中文（明确为测试数据，放系统临时目录）
const sampleDir = mkdtempSync(join(tmpdir(), 'srt-smoke-'));
const samplePath = join(sampleDir, 'sample-utf8.txt');
writeFileSync(samplePath, '第一行：你好，S-Read-TXT\n第二行：mixed\r\n第三行：结束', 'utf8');

if (!existsSync(EXE)) {
  console.error(`未找到应用产物：${EXE}\n请先执行 npm run tauri build -- --debug --no-bundle`);
  process.exit(1);
}

const child = spawn(EXE, [], {
  env: {
    ...process.env,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
  },
  stdio: 'ignore',
});

// 等待 CDP 页面目标出现（应用启动 + WebView 就绪）
async function waitTarget(timeoutMs = 15000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}/json/list`);
      const targets = await res.json();
      const page = targets.find((t) => t.type === 'page' && t.webSocketDebuggerUrl);
      if (page) return page;
    } catch {
      // 端口未就绪：继续轮询
    }
    await delay(250);
  }
  throw new Error('等待 CDP 目标超时');
}

// 极简 CDP 客户端：Runtime.evaluate（awaitPromise + returnByValue）
let msgId = 0;
const pending = new Map();
let ws;
// 通用 CDP 调用（按消息 id 匹配应答；返回原始 result）
function sendCdp(method, params = {}, timeoutMs = 10000) {
  const id = ++msgId;
  return new Promise((resolvePromise, reject) => {
    pending.set(id, { resolve: resolvePromise, reject });
    ws.send(JSON.stringify({ id, method, params }));
    setTimeout(() => {
      if (pending.delete(id)) reject(new Error('CDP 请求超时'));
    }, timeoutMs);
  });
}

// 在页面执行表达式（awaitPromise + returnByValue），返回 JSON 值
function evalJs(expression, timeoutMs = 10000) {
  return sendCdp(
    'Runtime.evaluate',
    { expression, awaitPromise: true, returnByValue: true },
    timeoutMs,
  ).then((result) => result?.result?.value);
}

// 通过 Tauri 内部 API 调用 IPC 命令（页面上下文，与前端同路径）
const invoke = (cmd, payload = {}) =>
  evalJs(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(payload)})`);

// 调用 IPC 并把「拒绝」捕获为普通返回值（结构化错误载荷得以原样取回；
// 否则 CDP 会把 Promise 拒绝转成异常，丢失 code/message 结构）。
const invokeCaught = (cmd, payload = {}) =>
  evalJs(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(payload)})` +
      `.then((value) => ({ ok: true, value })).catch((error) => ({ ok: false, error }))`,
  );

// 等待页面注入 Tauri 内部 API（CDP 目标出现可能早于页面脚本执行完毕）
async function waitForReady(timeoutMs = 10000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const ok = await evalJs(
        "typeof window.__TAURI_INTERNALS__?.invoke === 'function' && typeof window.__srt?.openPath === 'function'",
        2000,
      );
      if (ok) return;
    } catch {
      // 页面尚未就绪：继续轮询
    }
    await delay(200);
  }
  throw new Error('等待页面 Tauri API 就绪超时');
}

async function runScenarios() {
  const target = await waitTarget();
  ws = new WebSocket(target.webSocketDebuggerUrl);
  ws.addEventListener('message', (event) => {
    const msg = JSON.parse(event.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve: res, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(msg.error.message));
      else if (msg.result?.exceptionDetails)
        reject(
          new Error(
            msg.result.exceptionDetails.exception?.description ??
              msg.result.exceptionDetails.text ??
              '页面异常',
          ),
        );
      else res(msg.result);
    }
  });
  await new Promise((res) => ws.addEventListener('open', res));
  await waitForReady();

  const info = await invoke('get_app_info');
  check('get_app_info 返回版本', info.version === '0.0.1-beta', String(info.version));
  check('数据目录为便携目录', String(info.dataDir).endsWith('data'), String(info.dataDir));

  const opened = await invoke('open_file', { path: samplePath });
  check(
    'open_file 返回标签信息',
    opened.name === 'sample-utf8.txt' && opened.rowsTotal === 3,
    `tabId=${opened.tabId} rows=${opened.rowsTotal} enc=${opened.encoding}`,
  );

  const rows = await invoke('get_rows', { tabId: opened.tabId, startRow: 0, count: 10 });
  check(
    'get_rows 文本正确',
    rows.rows.length === 3 && rows.rows[1].text.includes('mixed'),
    `rows=${rows.rows.length} startPercent=${Number(rows.startPercent).toFixed(1)}`,
  );

  const reopened = await invoke('open_file', { path: samplePath });
  check('重复打开复用标签', reopened.tabId === opened.tabId, `tabId=${reopened.tabId}`);

  const encodings = await invoke('list_encodings');
  check('list_encodings 返回 8 项', encodings.length === 8, encodings.join('/'));

  const label1252 = encodings.find((label) => /1252/.test(label));
  const switched = await invoke('set_encoding', { tabId: opened.tabId, encoding: label1252 ?? null });
  check('set_encoding 切换生效', switched.encoding === label1252, String(switched.encoding));

  const restored = await invoke('set_encoding', { tabId: opened.tabId, encoding: null });
  check('set_encoding 恢复自动检测', /UTF/i.test(String(restored.encoding)), String(restored.encoding));

  // —— 前端（store → UI）端到端 ——
  const emptyText = await evalJs(
    "(() => { const el = document.querySelector('.empty'); return el ? (el.querySelector('.open-btn')?.textContent ?? '') : ''; })()",
  );
  check('空状态显示', emptyText.includes('打开'), `空状态="${emptyText}"`);

  // WebView2：直接对函数返回的 Promise 做 CDP awaitPromise 会报 “Promise was collected”，
  // 统一经 IIFE 包裹（见 scripts/lib/smoke-cdp.mjs 的 openPathDone 说明）。
  await evalJs(`(async () => { await window.__srt.openPath(${JSON.stringify(samplePath)}); return true; })()`);
  await delay(300);
  const tabName = await evalJs("document.querySelector('.tab-bar .tab .name')?.textContent ?? ''");
  check('标签栏显示文件名', tabName === 'sample-utf8.txt', `name="${tabName}"`);
  const tabActive = await evalJs(
    "document.querySelector('.tab-bar .tab')?.classList.contains('active') ?? false",
  );
  check('活动标签高亮', tabActive === true, `active=${tabActive}`);

  await evalJs("document.querySelector('.tab-bar .tab .close')?.click(); true");
  await delay(300);
  const remainingTabs = await evalJs("document.querySelectorAll('.tab-bar .tab').length");
  check('关闭标签后标签栏清空', remainingTabs === 0, `tabs=${remainingTabs}`);
  const emptyBack = await evalJs(
    "(() => { const el = document.querySelector('.empty'); return el ? (el.querySelector('.open-btn')?.textContent ?? '') : ''; })()",
  );
  check('关闭后回到空状态', emptyBack.includes('打开'), `空状态="${emptyBack}"`);

  // —— 虚拟滚动（2 万行大文件）——
  const bigPath = join(sampleDir, 'sample-big.txt');
  const bigLines = [];
  for (let index = 0; index < 20000; index += 1) {
    bigLines.push(`第 ${index} 行：虚拟滚动测试文本内容，用于验证仅渲染可视行。`);
  }
  writeFileSync(bigPath, bigLines.join('\n'), 'utf8');

  await evalJs(`(async () => { await window.__srt.openPath(${JSON.stringify(bigPath)}); return true; })()`);
  await delay(600);
  const rowCount = await evalJs("document.querySelectorAll('.reader .row').length");
  check('虚拟滚动仅渲染可视行', rowCount > 0 && rowCount < 200, `渲染 ${rowCount} 行`);
  const firstText = await evalJs("document.querySelector('.reader .row')?.textContent ?? ''");
  check('首屏文本正确', firstText.includes('第 0 行'), firstText.slice(0, 24));

  await evalJs(
    "(() => { const el = document.querySelector('.reader'); el.scrollTop = el.scrollHeight / 2; return true; })()",
  );
  await delay(900);
  const midText = await evalJs("document.querySelector('.reader .row')?.textContent ?? ''");
  check('滚动后窗口更新', !midText.includes('第 0 行'), midText.slice(0, 24));
  const statusText = await evalJs(
    "(document.querySelector('.status-bar')?.textContent ?? '').replace(/\\s+/g, ' ')",
  );
  check('状态栏百分比更新', /阅读 (4\d|5\d|6\d)%/.test(statusText), statusText.trim().slice(0, 64));

  const shotArg = args.indexOf('--screenshot');
  if (shotArg >= 0) {
    const out = resolve(args[shotArg + 1] ?? 'docs/screenshots/smoke.png');
    const shot = await sendCdp('Page.captureScreenshot', { format: 'png' });
    mkdirSync(dirname(out), { recursive: true });
    writeFileSync(out, Buffer.from(shot.data, 'base64'));
    console.log(`截图已保存：${out}`);
  }

  const missing = await invokeCaught('get_rows', { tabId: 999, startRow: 0, count: 1 });
  check(
    '未知标签返回错误载荷',
    missing.ok === false && missing.error?.code === 'TAB_NOT_FOUND',
    `code=${missing.error?.code}`,
  );
}

runScenarios()
  .catch((err) => check('冒烟流程异常', false, err.message))
  .finally(async () => {
    try {
      ws?.close();
    } catch {
      // 忽略关闭异常
    }
    try {
      spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    } catch {
      // 进程可能已退出
    }
    await delay(800);
    const failed = results.filter((r) => !r.ok).length;
    console.log(`\n结果：${results.length - failed}/${results.length} 通过`);
    process.exit(failed ? 1 : 0);
  });
