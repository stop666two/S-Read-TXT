#!/usr/bin/env node
// 启动度量（红线：完全冷启动到可阅读状态 <1s；本机为 OS 缓存预热口径，报告中注明）。
//
// 指标（2026-10-02 起，启动策略 =「立即显示 + HTML 内置占位」）：
// - 可见（应用日志）：应用日志中「主窗口已显示」相对「启动」的耗时——窗口先以主题背景色
//   显示（HTML 内置占位紧随其后），用户不会看到悬空空白；
// - 内容就绪（CDP）：`window.__srt` 已挂载且应用标题渲染（= 真实可阅读）。
// 判定：可见 max < 1000ms 且 内容就绪 median < 1000ms（默认 5 次；
// 首跑含全新 WebView2 配置目录，单独记录，不参与判定结论）。
// 用法：node scripts/measure-startup.mjs [--runs 5] [--exe <路径>]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, findTarget } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const runs = Number(argValue('--runs', '5'));
const exePath = resolve(
  argValue('--exe', join(root, 'src-tauri', 'target', 'release', 's-read-txt.exe')),
);
const workDir = join(process.env.TEMP ?? '.', 'srt-startup-measure');
const dataDir = join(workDir, 'data');
const logPath = join(dataDir, 'logs', 'app.log');

if (!existsSync(exePath)) {
  console.error(`可执行文件不存在：${exePath}`);
  process.exit(2);
}
mkdirSync(workDir, { recursive: true });

/** 从应用日志解析「启动 → 主窗口已显示」耗时（毫秒；缺失返回 null） */
function visibleFromLog() {
  if (!existsSync(logPath)) return null;
  const lines = readFileSync(logPath, 'utf8').split(/\r?\n/);
  let startTs = null;
  let shownTs = null;
  for (const line of lines) {
    const timeMatch = line.match(/"time":"([^"]+)"/);
    if (!timeMatch) continue;
    if (startTs === null && line.includes('启动（数据目录')) startTs = Date.parse(timeMatch[1]);
    if (line.includes('主窗口已显示')) shownTs = Date.parse(timeMatch[1]);
  }
  if (startTs === null || shownTs === null) return null;
  return shownTs - startTs;
}

const readyTimes = [];
const visibleTimes = [];

for (let run = -1; run < runs; run += 1) {
  const isWarmup = run < 0;
  const port = 19400 + run * 11 + Math.floor(Math.random() * 7);
  // 每次独立日志：避免跨次匹配到上一轮的「主窗口已显示」
  rmSync(logPath, { force: true });
  const startedAt = performance.now();
  const startedAtEpoch = Date.now();
  const child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dataDir,
      // SRT_MEASURE_EXTRA_ARGS：可选附加 WebView2 参数（A/B 对比用；如 ' --in-process-gpu'
      // 可让应用跳过其默认参数组，用于隔离参数对启动的影响）。
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}${process.env.SRT_MEASURE_EXTRA_ARGS ?? ''}`,
    },
    stdio: 'ignore',
  });
  let tReady = null;
  let tPageReady = null;
  try {
    const client = await createClient(await findTarget(port, null, 30000));
    const evalJs = async (expression) => {
      const result = await client.send('Runtime.evaluate', {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
      return result.result?.value;
    };
    for (let attempt = 0; attempt < 800 && tReady === null; attempt += 1) {
      const ok = await evalJs(`!!window.__srt?.openPath && !!document.querySelector('.app-title')`);
      if (ok === true) tReady = performance.now() - startedAt;
      else await delay(10);
    }
    // 页面内时钟就绪（排除 CDP 连接开销）：绝对就绪 ≈ (导航起点-进程起点) + 页面内就绪标记
    const metrics = await evalJs(
      `({ origin: performance.timeOrigin ?? null, readyAt: window.__srtReadyAt ?? null })`,
    );
    if (metrics && typeof metrics.readyAt === 'number' && typeof metrics.origin === 'number') {
      tPageReady = metrics.origin - startedAtEpoch + metrics.readyAt;
    }
    client.close();
  } finally {
    if (child.pid) {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    }
    await delay(400);
  }
  const tVisible = visibleFromLog();
  if (!isWarmup) {
    readyTimes.push(tPageReady ?? tReady);
    visibleTimes.push(tVisible);
  }
  console.log(
    `${isWarmup ? '预热' : `RUN ${run + 1}`}: ready=${tReady === null ? '-' : tReady.toFixed(0)}ms ready(page)=${tPageReady === null ? '-' : tPageReady.toFixed(0)}ms visible(日志)=${tVisible === null ? '-' : `${tVisible}ms`}`,
  );
}

function stats(values) {
  const ok = values.filter((v) => v !== null);
  if (ok.length === 0) return null;
  const sorted = [...ok].sort((a, b) => a - b);
  const median = sorted[Math.floor(sorted.length / 2)];
  return { min: sorted[0], median, max: sorted[sorted.length - 1], count: ok.length, total: values.length };
}

const readyStats = stats(readyTimes);
const visibleStats = stats(visibleTimes);
console.log('');
console.log(
  `内容就绪(页面时钟)：min=${readyStats?.min.toFixed(0)}ms median=${readyStats?.median.toFixed(0)}ms max=${readyStats?.max.toFixed(0)}ms（${readyStats?.count}/${readyStats?.total}）`,
);
console.log(
  `窗口可见：min=${visibleStats?.min ?? '-'}ms median=${visibleStats?.median ?? '-'}ms max=${visibleStats?.max ?? '-'}ms（${visibleStats?.count}/${visibleStats?.total}）`,
);
const pass =
  visibleStats !== null &&
  readyStats !== null &&
  visibleStats.max < 1000 &&
  readyStats.median < 1000;
console.log(
  `判定（可见 max < 1000ms 且 内容就绪[页面内时钟] median < 1000ms；含 1 次预热轮不计入、冷首跑单独展示）：${pass ? 'PASS' : 'FAIL'}`,
);

try {
  rmSync(workDir, { recursive: true, force: true });
} catch {
  // 忽略
}
process.exit(pass ? 0 : 1);
