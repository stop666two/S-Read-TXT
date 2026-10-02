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

/** 轮询查找页面型 CDP 目标（返回 WebSocket URL）。 */
export async function findTarget(port) {
  for (let attempt = 0; attempt < 180; attempt += 1) {
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json`);
      const targets = await response.json();
      const page = targets.find((target) => target.type === 'page');
      if (page?.webSocketDebuggerUrl) return page.webSocketDebuggerUrl;
    } catch {
      // 应用尚未就绪，继续轮询
    }
    await delay(250);
  }
  throw new Error('未发现 CDP 页面目标（应用未启动或调试端口未开）');
}

/** 建立 CDP 客户端（send/close）。 */
export function createClient(wsUrl) {
  return new Promise((resolveClient, rejectClient) => {
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
