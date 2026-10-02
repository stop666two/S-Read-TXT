#!/usr/bin/env node
// 压力测试（阶段 9）：10×100MB 标签、反复滚动、频繁开关标签、频繁切编码、大文件编辑；
// 同步采样应用进程树内存（工作集合计）与各操作耗时，输出红线判定与 JSON 结果。
//
// 前置：已构建 release 产物（npm run tauri build）。
// 用法：node scripts/stress.mjs [--exe <路径>] [--files 10] [--size-mb 100] [--churn 30]
// 输出：docs/verify/stress-latest.json；截图 docs/screenshots/phase9-stress.png

import { spawn, spawnSync } from 'node:child_process';
import { closeSync, existsSync, mkdirSync, openSync, rmSync, writeSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget, openPathDone, waitForValue } from './lib/smoke-cdp.mjs';
import { makeBigFile, makeSmallFile, removeWithRetry, sampleMemoryTree } from './lib/system.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(
  argValue('--exe', join(root, 'src-tauri', 'target', 'release', 's-read-txt.exe')),
);
const fileCount = Number(argValue('--files', '10'));
const sizeMb = Number(argValue('--size-mb', '100'));
const churnCount = Number(argValue('--churn', '30'));
const watchdogMs = Number(process.env.SRT_STRESS_WATCHDOG_MS ?? '900000');

const workDir = join(process.env.TEMP ?? '.', 'srt-stress');
const dataDir = join(workDir, `data-${Date.now()}`);
const port = 19600 + Math.floor(Math.random() * 300);

const results = [];
let currentStep = 'S0 准备';
function check(name, passed, detail = '') {
  results.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

const watchdog = setTimeout(() => {
  console.error(`看门狗超时（${watchdogMs}ms），最后步骤：${currentStep}`);
  process.exit(4);
}, watchdogMs);

// 样本生成/内存采样共用工具已抽取到 ./lib/system.mjs（DRY）。

async function main() {
  if (!existsSync(exePath)) {
    console.error(`可执行文件不存在：${exePath}（先运行 npm run tauri build）`);
    process.exit(2);
  }
  currentStep = 'S1 生成样本';
  removeWithRetry(workDir);
  mkdirSync(workDir, { recursive: true });
  const bigFiles = [];
  for (let i = 1; i <= fileCount; i += 1) {
    const path = join(workDir, `big-${String(i).padStart(2, '0')}.txt`);
    makeBigFile(path, sizeMb, `BIG-${i}`);
    bigFiles.push(path);
  }
  const churnFiles = [];
  for (let i = 1; i <= churnCount; i += 1) {
    const path = join(workDir, `churn-${String(i).padStart(2, '0')}.txt`);
    makeSmallFile(path, 120);
    churnFiles.push(path);
  }
  console.log(`样本就绪：${bigFiles.length}×${sizeMb}MiB + ${churnFiles.length} 个小型文件`);

  currentStep = 'S2 启动应用';
  const child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });

  const t0 = performance.now();
  let client;
  try {
    client = await createClient(await findTarget(port));
    const evalJs = async (expression) => {
      const result = await client.send('Runtime.evaluate', {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
      if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
      return result.result?.value;
    };
    const invoke = async (cmd, args = {}) =>
      evalJs(
        `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}).then((v) => (v === undefined ? null : v))`,
      );
    const tabsInfo = () => invoke('list_tabs');
    const statusText = () => evalJs(`(document.querySelector('.status-bar')?.textContent ?? '').trim()`);

    let ready = false;
    for (let attempt = 0; attempt < 400; attempt += 1) {
      ready = (await evalJs('!!window.__srt?.openPath && !!window.__TAURI_INTERNALS__')) === true;
      if (ready) break;
      await delay(50);
    }
    if (!ready) throw new Error('前端未就绪');
    await dismissOnboarding(evalJs);
    console.log(`应用就绪：${(performance.now() - t0).toFixed(0)}ms`);

    // ---- S3：打开 10 个 100MB 标签（记录耗时） ----
    currentStep = 'S3 打开 10 个大标签';
    const openTimes = [];
    for (let i = 0; i < bigFiles.length; i += 1) {
      const started = performance.now();
      await evalJs(openPathDone(bigFiles[i]));
      await waitForValue(async () => {
        const view = await tabsInfo();
        return view.tabs.length === i + 1 ? true : null;
      }, 30000);
      openTimes.push(performance.now() - started);
    }
    const view10 = await tabsInfo();
    check('S3 十个 100MB 标签全部打开', view10.tabs.length === fileCount, `tabs=${view10.tabs.length}`);
    const slowestOpen = Math.max(...openTimes);
    check(
      'S3b 单标签打开耗时（含索引）',
      slowestOpen < 15000,
      `max=${slowestOpen.toFixed(0)}ms avg=${(openTimes.reduce((a, b) => a + b, 0) / openTimes.length).toFixed(0)}ms`,
    );

    // ---- S4：内存（打开后稳态；红线口径 = 专用工作集） ----
    currentStep = 'S4 内存采样（打开后）';
    await delay(2500);
    const memAfterOpen = [];
    let wsOpenSample = null;
    for (let i = 0; i < 3; i += 1) {
      const sample = sampleMemoryTree(child.pid);
      memAfterOpen.push(sample.privateMb);
      wsOpenSample = sample;
      await delay(1000);
    }
    const steadyOpen = Math.max(...memAfterOpen);
    console.log(
      `内存（打开后）专用工作集三次采样：${memAfterOpen.map((m) => m.toFixed(1)).join(' / ')} MiB（参考：工作集合计 ${wsOpenSample.workingSetMb.toFixed(1)} MiB，进程 ${wsOpenSample.count} 个）`,
    );

    // ---- S5：反复滚动（当前标签 = 最后一个大文件） ----
    currentStep = 'S5 反复滚动';
    const scrollStart = performance.now();
    let scrollOk = 0;
    const scrollDeltas = [];
    for (let rep = 0; rep < 15; rep += 1) {
      const value = 100000 + rep * 900000;
      await evalJs(`(document.querySelector('.reader').scrollTop = ${value}, true)`, false);
      // 行高实测矫正会做滚动锚定补偿（万行级文件的估算误差累积可达数万像素）：
      // 逐次轮询等待落定（2% 相对容差；最多 2s），并记录每个落点的漂移量用于诊断。
      const tolerance = Math.max(50000, value * 0.02);
      const top = await waitForValue(async () => {
        const current = await evalJs(`document.querySelector('.reader').scrollTop`);
        return typeof current === 'number' && Math.abs(current - value) <= tolerance
          ? current
          : null;
      }, 2000);
      const settled = top ?? (await evalJs(`document.querySelector('.reader').scrollTop`));
      if (typeof settled === 'number') {
        scrollDeltas.push(Math.round(Math.abs(settled - value)));
        if (top !== null) scrollOk += 1;
      }
    }
    const scrollMs = performance.now() - scrollStart;
    const maxDelta = scrollDeltas.length > 0 ? Math.max(...scrollDeltas) : -1;
    check(
      'S5 反复滚动 15 次（位置保持）',
      scrollOk >= 15,
      `ok=${scrollOk}/15 用时=${scrollMs.toFixed(0)}ms 最大漂移=${maxDelta}px 逐轮漂移=${scrollDeltas.join('/')}`,
    );

    // ---- S6：频繁切换标签（20 轮循环） ----
    currentStep = 'S6 频繁切换标签';
    const view = await tabsInfo();
    const tabIds = view.tabs.map((tab) => tab.tabId);
    const switchStart = performance.now();
    let switchOk = 0;
    for (let round = 0; round < 20; round += 1) {
      const target = tabIds[round % tabIds.length];
      await invoke('set_active_tab', { tabId: target });
      const active = await waitForValue(
        async () => ((await tabsInfo()).activeTabId === target ? target : null),
        4000,
      );
      if (active === target) switchOk += 1;
    }
    const switchMs = performance.now() - switchStart;
    check('S6 频繁切换标签 20 轮', switchOk === 20, `ok=${switchOk}/20 用时=${switchMs.toFixed(0)}ms`);

    // ---- S7：频繁开关标签（开→关 25 轮，小型文件） ----
    currentStep = 'S7 频繁开关标签';
    const churnStart = performance.now();
    let churnOk = 0;
    for (let round = 0; round < 25; round += 1) {
      const file = churnFiles[round % churnFiles.length];
      await evalJs(openPathDone(file));
      const opened = await waitForValue(async () => {
        const v = await tabsInfo();
        return v.tabs.length === fileCount + 1 ? v.tabs[v.tabs.length - 1].tabId : null;
      }, 8000);
      if (opened === null) continue;
      // 经界面关闭（保持前端 store 与后端一致；贴近真实用户路径）
      await evalJs(
        `(() => { const el = document.querySelector('[data-tab-id="${opened}"] .close'); el?.click(); return !!el; })()`,
      );
      const closed = await waitForValue(async () => {
        const v = await tabsInfo();
        return v.tabs.length === fileCount ? true : null;
      }, 8000);
      if (closed === true) churnOk += 1;
    }
    const churnMs = performance.now() - churnStart;
    check('S7 开关标签 25 轮', churnOk === 25, `ok=${churnOk}/25 用时=${churnMs.toFixed(0)}ms`);

    // ---- S8：频繁切换编码（大文件，6 次） ----
    currentStep = 'S8 频繁切换编码';
    const firstTabId = tabIds[0];
    const encodingSeq = ['GB18030', 'UTF-8', 'GB18030', 'UTF-8', 'GB18030', 'UTF-8'];
    const encTimes = [];
    let encOk = 0;
    for (const label of encodingSeq) {
      const started = performance.now();
      await invoke('set_encoding', { tabId: firstTabId, encoding: label });
      const applied = await waitForValue(async () => {
        const v = await tabsInfo();
        return v.tabs.find((tab) => tab.tabId === firstTabId)?.encoding === label ? true : null;
      }, 20000);
      encTimes.push(performance.now() - started);
      if (applied === true) encOk += 1;
    }
    const encMax = Math.max(...encTimes);
    check('S8 切换编码 6 次（含重建索引）', encOk === 6, `max=${encMax.toFixed(0)}ms avg=${(encTimes.reduce((a, b) => a + b, 0) / encTimes.length).toFixed(0)}ms`);
    const rowsProbe = await invoke('get_rows', { tabId: firstTabId, startRow: 0, count: 3 });
    check('S8b 切换后取行正常', Array.isArray(rowsProbe?.rows) && rowsProbe.rows.length === 3);

    // ---- S9：大文件编辑（插入/撤销/退出） ----
    currentStep = 'S9 大文件编辑';
    await invoke('set_active_tab', { tabId: firstTabId });
    await invoke('toggle_edit', { tabId: firstTabId });
    const editApplied = await invoke('apply_edits', {
      tabId: firstTabId,
      ops: [{ kind: 'insert', row: 0, utf16: 0, text: 'X' }],
    });
    const dirty = await waitForValue(async () => {
      const v = await tabsInfo();
      return v.tabs.find((tab) => tab.tabId === firstTabId)?.dirty === true ? true : null;
    }, 8000);
    await invoke('undo_edit', { tabId: firstTabId });
    const clean = await waitForValue(async () => {
      const v = await tabsInfo();
      return v.tabs.find((tab) => tab.tabId === firstTabId)?.dirty === false ? true : null;
    }, 8000);
    await invoke('toggle_edit', { tabId: firstTabId });
    check(
      'S9 大文件编辑（插入→脏→撤销→干净）',
      editApplied !== null && dirty === true && clean === true,
    );

    // ---- S10：内存（压力后） + 截图 ----
    currentStep = 'S10 内存采样（压力后）';
    const activeView = await tabsInfo();
    const bigTabs = activeView.tabs.filter((tab) => String(tab.name ?? '').startsWith('big-'));
    const lastBigId =
      bigTabs[bigTabs.length - 1]?.tabId ?? activeView.tabs[activeView.tabs.length - 1].tabId;
    // 经界面激活大标签（保证状态栏与截图反映真实阅读态）
    await evalJs(
      `(() => { const el = document.querySelector('[data-tab-id="${lastBigId}"]'); el?.click(); return !!el; })()`,
    );
    await waitForValue(async () => {
      const status = await statusText();
      return status.includes('big-') ? true : null;
    }, 6000);
    await delay(2000);
    const memAfterStress = [];
    let wsStressSample = null;
    for (let i = 0; i < 3; i += 1) {
      const sample = sampleMemoryTree(child.pid);
      memAfterStress.push(sample.privateMb);
      wsStressSample = sample;
      await delay(1000);
    }
    const steadyStress = Math.max(...memAfterStress);
    console.log(
      `内存（压力后）专用工作集三次采样：${memAfterStress.map((m) => m.toFixed(1)).join(' / ')} MiB（参考：工作集合计 ${wsStressSample.workingSetMb.toFixed(1)} MiB）`,
    );
    const status = await statusText();
    check(
      'S10 状态栏仍正常（活动标签信息）',
      status.includes('big-') && status.includes('%') && status.includes('UTF-8'),
      status.slice(0, 80),
    );

    currentStep = 'S10b 截图';
    try {
      const shot = await client.send('Page.captureScreenshot', { format: 'png' });
      const shotPath = join(root, 'docs', 'screenshots', 'phase9-stress.png');
      mkdirSync(dirname(shotPath), { recursive: true });
      const shotFd = openSync(shotPath, 'w');
      writeSync(shotFd, Buffer.from(shot.data, 'base64'));
      closeSync(shotFd);
      check('S10b 压力截图已保存', true, shotPath);
    } catch (error) {
      check('S10b 压力截图已保存', false, String(error));
    }

    // ---- 红线判定（内存：稳态峰值 <120MiB 通过；同时报告理想线 100MiB） ----
    const peak = Math.max(steadyOpen, steadyStress);
    const memPass = peak < 120;
    const idealPass = peak < 100;
    const wsPeak = Math.max(wsOpenSample.workingSetMb, wsStressSample.workingSetMb);
    check(
      'S11 内存红线（10 标签 + 压力后；专用工作集口径）',
      memPass,
      `peak=${peak.toFixed(1)}MiB（理想线 ${idealPass ? '≤' : '>'}100MiB；红线 <120MiB；工作集参考峰值 ${wsPeak.toFixed(1)}MiB——含可回收的 mmap 文件缓存页）`,
    );

    const payload = {
      generatedAt: new Date().toISOString(),
      exe: exePath,
      files: fileCount,
      sizeMb,
      openTimesMs: openTimes.map((v) => Math.round(v)),
      memoryAfterOpen: memAfterOpen.map((v) => Number(v.toFixed(1))),
      memoryAfterStress: memAfterStress.map((v) => Number(v.toFixed(1))),
      memoryPeak: Number(peak.toFixed(1)),
      memoryPass: memPass,
      memoryIdeal: idealPass,
      encodingTimesMs: encTimes.map((v) => Math.round(v)),
      scrollMs: Math.round(scrollMs),
      switchMs: Math.round(switchMs),
      churnMs: Math.round(churnMs),
      checks: results,
    };
    const outPath = join(root, 'docs', 'verify', 'stress-latest.json');
    mkdirSync(dirname(outPath), { recursive: true });
    writeSync(openSync(outPath, 'w'), Buffer.from(`${JSON.stringify(payload, null, 2)}\n`, 'utf8'));
  } finally {
    try {
      client?.close();
    } catch {
      // 忽略
    }
    if (child.pid) {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    }
  }

  clearTimeout(watchdog);
  const failed = results.filter((item) => !item.passed);
  console.log(`\n压力测试结果：${results.length - failed.length}/${results.length} 通过`);
  if (failed.length > 0) {
    console.log(`失败项：${failed.map((item) => item.name).join('；')}`);
  }
  removeWithRetry(workDir);
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((error) => {
  clearTimeout(watchdog);
  console.error(`压力测试失败（${currentStep}）：${error?.message ?? error}`);
  process.exit(3);
});
