// 冒烟测试共享工具：CDP 最小客户端（Node 内置 fetch + WebSocket，零依赖）。
// 供 smoke-edit.mjs / smoke-abuse.mjs 复用。

/** 读取命令行参数（--name value）。 */
export function argValue(name, fallback) {
  const index = process.argv.indexOf(name);
  return index >= 0 && process.argv[index + 1] ? process.argv[index + 1] : fallback;
}

/** 延时。 */
export function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** 轮询等待（直到返回值真值或超时；超时返回最后取值，供断言展示）。 */
export async function waitForValue(fn, timeoutMs = 4000, interval = 150) {
  const deadline = Date.now() + timeoutMs;
  let value;
  for (;;) {
    value = await fn();
    if (value || Date.now() >= deadline) return value;
    await delay(interval);
  }
}

/** 轮询查找页面型 CDP 目标（返回 WebSocket URL）。
 *  `urlIncludes`：可选 URL 子串过滤（多窗口场景，如设置窗口 'settings.html'）。 */
export async function findTarget(port, urlIncludes = null) {
  for (let attempt = 0; attempt < 180; attempt += 1) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json`);
      const targets = await response.json();
      const page = targets.find(
        (target) =>
          target.type === 'page' &&
          (!urlIncludes || (target.url ?? '').includes(urlIncludes)),
      );
      if (page?.webSocketDebuggerUrl) return page.webSocketDebuggerUrl;
    } catch {
      // 应用尚未就绪，继续轮询
    }
    await delay(250);
  }
  throw new Error('未发现 CDP 页面目标（应用未启动或调试端口未开）');
}

/** 生成「等待 openPath 完成」的页面表达式（返回 Promise<true>）。
 *  必须经 IIFE 包裹：WebView2 对「直接调用函数返回的 Promise」经 CDP `awaitPromise`
 *  会立即报 “Promise was collected”（2026-10-02 起实测必现）；IIFE 外层 Promise 稳定可等待。 */
export function openPathDone(path) {
  return `(async () => { await window.__srt.openPath(${JSON.stringify(path)}); return true; })()`;
}

/** 关闭首启引导（若存在；真实用户路径：勾选「不再显示」并点「开始使用」）。
 *  为什么必须关：引导以模态呈现——会遮挡 `[role="dialog"]` 选择器（测试易命中错弹窗），
 *  并在设计上挂起全部全局快捷键（见 shortcuts/engine 的 modalOpen 判定）。 */
export async function dismissOnboarding(evalJs) {
  const present = await waitForValue(async () => {
    const has = await evalJs(`!!document.querySelector('.overlay[aria-label="使用向导"]')`);
    return has ? true : null;
  }, 2000);
  if (present !== true) return;
  await evalJs(
    `(() => { const overlay = document.querySelector('.overlay[aria-label="使用向导"]');
      const check = overlay?.querySelector('.dont-show input');
      if (check && !check.checked) check.click();
      const button = [...(overlay?.querySelectorAll('button') ?? [])].find((b) => b.textContent.includes('开始使用'));
      button?.click(); return true; })()`,
  );
  await waitForValue(async () => {
    const gone = await evalJs(`!document.querySelector('.overlay[aria-label="使用向导"]')`);
    return gone ? true : null;
  }, 4000);
}

/** 建立 CDP 客户端（send/close）。 */
export function createClient(wsUrl) {  return new Promise((resolveClient, rejectClient) => {
    const socket = new WebSocket(wsUrl);
    let nextId = 1;
    const pending = new Map();
    socket.addEventListener('open', () => {
      resolveClient({
        send(method, params = {}) {
          const id = nextId;
          nextId += 1;
          socket.send(JSON.stringify({ id, method, params }));
          return new Promise((resolveSend, rejectSend) => {
            pending.set(id, { resolve: resolveSend, reject: rejectSend });
          });
        },
        close() {
          socket.close();
        },
      });
    });
    socket.addEventListener('error', () => rejectClient(new Error('CDP 连接失败')));
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(event.data);
      if (message.id && pending.has(message.id)) {
        const { resolve: resolveSend, reject: rejectSend } = pending.get(message.id);
        pending.delete(message.id);
        if (message.error) rejectSend(new Error(message.error.message));
        else resolveSend(message.result);
      }
    });
  });
}
