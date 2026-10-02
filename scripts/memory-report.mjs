#!/usr/bin/env node
// 内存构成分析（诊断工具，阶段 9）：打开 N 个 100MiB 标签后，
// 输出应用进程树逐进程（角色/专用工作集/工作集）与 WebView2 页面侧指标（JS 堆/DOM 计数）。
// 用法：node scripts/memory-report.mjs [--files 10] [--size-mb 100] [--tag baseline]
// 环境：SRT_EXTRA_BROWSER_ARGS="--disable-gpu …" 追加 WebView2 启动参数（A/B 对比用）。

import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, openSync, closeSync, writeSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget, openPathDone, waitForValue } from './lib/smoke-cdp.mjs';
import { makeBigFile, removeWithRetry, sampleProcessTable } from './lib/system.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'release', 's-read-txt.exe')));
const fileCount = Number(argValue('--files', '10'));
const sizeMb = Number(argValue('--size-mb', '100'));
const tag = argValue('--tag', 'baseline');
const extraArgs = process.env.SRT_EXTRA_BROWSER_ARGS ?? '';

const workDir = join(process.env.TEMP ?? '.', 'srt-memreport');
const dataDir = join(workDir, `data-${Date.now()}-${tag}`);
const port = 19200 + Math.floor(Math.random() * 300);

mkdirSync(workDir, { recursive: true });
const files = [];
for (let i = 1; i <= fileCount; i += 1) {
  const path = join(workDir, `mem-${String(i).padStart(2, '0')}.txt`);
  makeBigFile(path, sizeMb, `MEM-${i}`);
  files.push(path);
}

const browserArgs = [`--remote-debugging-port=${port}`, ...(extraArgs ? extraArgs.split(/\s+/) : [])].join(' ');
const child = spawn(exePath, [], {
  env: { ...process.env, SRT_DATA_DIR: dataDir, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: browserArgs },
  stdio: 'ignore',
});

let report = null;
try {
  const client = await createClient(await findTarget(port));
  const evalJs = async (expression) => {
    const result = await client.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
    return result.result?.value;
  };
  let ready = false;
  for (let attempt = 0; attempt < 400; attempt += 1) {
    ready = (await evalJs('!!window.__srt?.openPath')) === true;
    if (ready) break;
    await delay(50);
  }
  if (!ready) throw new Error('前端未就绪');
  await dismissOnboarding(evalJs);
  for (let i = 0; i < files.length; i += 1) {
    await evalJs(openPathDone(files[i]));
    await waitForValue(async () => {
      const count = await evalJs(`window.__TAURI_INTERNALS__.invoke('list_tabs').then((v) => v.tabs.length)`);
      return count === i + 1 ? true : null;
    }, 30000);
  }
  await delay(3000);

  await client.send('Performance.enable');
  const perf = await client.send('Performance.getMetrics');
  const metrics = Object.fromEntries(perf.metrics.map((item) => [item.name, item.value]));
  const dom = await client.send('Memory.getDOMCounters');
  const table = sampleProcessTable(child.pid);
  const totals = {
    privateMb: table.reduce((sum, row) => sum + row.privateMb, 0),
    workingSetMb: table.reduce((sum, row) => sum + row.workingSetMb, 0),
  };

  console.log(`\n[${tag}] 进程树构成（专用工作集合计 ${totals.privateMb.toFixed(1)} MiB）`);
  for (const row of [...table].sort((a, b) => b.privateMb - a.privateMb)) {
    console.log(
      `  ${row.role.padEnd(18)} pid=${String(row.pid).padEnd(6)} 专用 ${row.privateMb.toFixed(1).padStart(7)} MiB  工作集 ${row.workingSetMb.toFixed(1).padStart(8)} MiB`,
    );
  }
  console.log(`[${tag}] 页面侧：JSHeapUsed=${(metrics.JSHeapUsedSize / 1048576).toFixed(1)} MiB（总 ${(metrics.JSHeapTotalSize / 1048576).toFixed(1)} MiB）DOM节点=${dom.nodes} 监听器=${dom.jsEventListeners} 文档=${dom.documents}`);
  report = {
    tag,
    extraArgs,
    generatedAt: new Date().toISOString(),
    files: fileCount,
    sizeMb,
    totals: { privateMb: Number(totals.privateMb.toFixed(1)), workingSetMb: Number(totals.workingSetMb.toFixed(1)) },
    processes: table.map((row) => ({ ...row, privateMb: Number(row.privateMb.toFixed(1)), workingSetMb: Number(row.workingSetMb.toFixed(1)) })),
    page: {
      jsHeapUsedMb: Number((metrics.JSHeapUsedSize / 1048576).toFixed(1)),
      jsHeapTotalMb: Number((metrics.JSHeapTotalSize / 1048576).toFixed(1)),
      domNodes: dom.nodes,
      jsEventListeners: dom.jsEventListeners,
      documents: dom.documents,
    },
  };
  client.close();
} finally {
  if (child.pid) {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  }
  await delay(400);
}

if (report) {
  const outPath = join(root, 'docs', 'verify', `memory-${tag}.json`);
  mkdirSync(dirname(outPath), { recursive: true });
  const fd = openSync(outPath, 'w');
  writeSync(fd, Buffer.from(`${JSON.stringify(report, null, 2)}\n`, 'utf8'));
  closeSync(fd);
  console.log(`报告：${outPath}`);
}
removeWithRetry(workDir);
process.exit(report ? 0 : 1);
