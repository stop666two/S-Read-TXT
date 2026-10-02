// CDP（Chrome DevTools Protocol）工具脚本（scripts/cdp.mjs）
// 用途：对本机以 --remote-debugging-port 启动的 WebView2 / Chromium 执行冒烟与 E2E 验证
// 依赖：Node 内置 fetch 与 WebSocket（Node ≥ 22；本机为 26），零第三方依赖
//
// 用法：
//   node scripts/cdp.mjs list                    列出可调试页面（类型 / 标题 / URL）
//   node scripts/cdp.mjs text [最大字符数]        输出页面标题与正文文本（默认 2000 字符）
//   node scripts/cdp.mjs eval "<表达式>"          执行 JS 表达式并打印 JSON 结果
//   node scripts/cdp.mjs screenshot <输出路径>    截图保存 PNG（自动创建父目录）
//
// 环境变量：
//   SRT_CDP_PORT  调试端口（默认 9222）

import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';

/** 调试端口与 HTTP 端点基址 */
const port = process.env.SRT_CDP_PORT ?? '9222';
const base = `http://127.0.0.1:${port}`;

/**
 * 获取第一个 type=page 的调试目标。
 * @returns {Promise<{title:string,url:string,webSocketDebuggerUrl:string}>}
 * @throws 端口无响应或不存在页面目标时抛出（上层统一转为中文错误）
 */
async function firstPageTarget() {
  const res = await fetch(`${base}/json/list`);
  if (!res.ok) throw new Error(`调试端口无响应：HTTP ${res.status}`);
  const targets = await res.json();
  const page = targets.find((t) => t.type === 'page');
  if (!page) throw new Error('未找到可调试页面（type=page）');
  return page;
}

/**
 * 极简 CDP 客户端：连接页面目标，按消息 id 匹配请求与响应。
 * @param {{webSocketDebuggerUrl:string}} target 页面目标
 * @returns {Promise<{send:(method:string,params?:object)=>Promise<any>, close:()=>void}>}
 */
async function connectPage(target) {
  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', () => reject(new Error('WebSocket 连接失败')), { once: true });
  });

  /** 自增消息 id */
  let nextId = 1;
  /** 未决请求表：id → {resolve, reject} */
  const pending = new Map();

  ws.addEventListener('message', (event) => {
    const msg = JSON.parse(event.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(String(msg.error.message)));
      else resolve(msg.result);
    }
  });

  /**
   * 发送 CDP 命令并等待响应。
   * @param {string} method 命令名（如 Runtime.evaluate）
   * @param {object} params 命令参数
   */
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params }));
    });

  return { send, close: () => ws.close() };
}

/**
 * 在页面中执行 JS 表达式并返回值（支持 await Promise；返回值为 JSON 可序列化数据时展开）。
 * @param {object} target 页面目标
 * @param {string} expression JS 表达式
 * @returns {Promise<any>} 表达式结果
 * @throws 页面内异常时抛出，携带异常文本
 */
async function evaluate(target, expression) {
  const { send, close } = await connectPage(target);
  try {
    const result = await send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.text ?? '页面内执行异常');
    }
    return result.result.value;
  } finally {
    close();
  }
}

/** 主流程：解析命令行子命令并执行 */
async function main() {
  const [command, ...args] = process.argv.slice(2);

  // list：不需要页面目标，直接打印全部调试目标
  if (!command || command === 'list') {
    const res = await fetch(`${base}/json/list`);
    const targets = await res.json();
    for (const t of targets) {
      console.log(`${t.type}\t${t.title}\t${t.url}`);
    }
    return;
  }

  const target = await firstPageTarget();

  if (command === 'text') {
    const limit = Number(args[0] ?? 2000);
    const raw = await evaluate(
      target,
      `JSON.stringify({title: document.title, text: document.body.innerText.slice(0, ${limit})})`,
    );
    const parsed = JSON.parse(raw);
    console.log(`标题：${parsed.title}`);
    console.log('--- 正文 ---');
    console.log(parsed.text);
    return;
  }

  if (command === 'eval') {
    const value = await evaluate(target, args[0] ?? 'undefined');
    console.log(JSON.stringify(value, null, 2));
    return;
  }

  if (command === 'screenshot') {
    const out = path.resolve(args[0] ?? 'screenshot.png');
    const { send, close } = await connectPage(target);
    try {
      const shot = await send('Page.captureScreenshot', { format: 'png' });
      mkdirSync(path.dirname(out), { recursive: true });
      writeFileSync(out, Buffer.from(shot.data, 'base64'));
      console.log(`截图已保存：${out}`);
    } finally {
      close();
    }
    return;
  }

  throw new Error(`未知子命令：${command}（可用：list / text / eval / screenshot）`);
}

main().catch((error) => {
  console.error(`CDP 工具执行失败：${error.message}`);
  process.exit(1);
});
