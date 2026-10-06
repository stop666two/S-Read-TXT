// 压力/分析脚本共享的系统工具：样本生成、稳健删除、进程树内存采样（专用工作集口径）。
// 说明：本文件被 scripts/stress.mjs 与 scripts/memory-report.mjs 复用（DRY）。

import { spawnSync } from 'node:child_process';
import { closeSync, existsSync, openSync, rmSync, writeSync } from 'node:fs';

/** 生成精确 sizeMiB（MiB）的大文件：行结构（88×y + \n）+ 首行标签。 */
export function makeBigFile(path, sizeMiB, tag) {
  const fd = openSync(path, 'w');
  const line = Buffer.from('y'.repeat(88) + '\n');
  const block = Buffer.alloc(1 << 20);
  for (let off = 0; off < block.length; off += line.length) {
    line.copy(block, off, 0, Math.min(line.length, block.length - off));
  }
  const totalBytes = sizeMiB * 1024 * 1024;
  const tagBytes = Buffer.from(`# ${tag}\n`);
  writeSync(fd, tagBytes);
  let remaining = totalBytes - tagBytes.length;
  while (remaining >= block.length) {
    writeSync(fd, block);
    remaining -= block.length;
  }
  if (remaining > 0) writeSync(fd, block.subarray(0, remaining));
  closeSync(fd);
}

/** 生成小型行文件（标签开关/滚动样本）。 */
export function makeSmallFile(path, lines) {
  const fd = openSync(path, 'w');
  for (let i = 0; i < lines; i += 1) writeSync(fd, Buffer.from(`churn-${i} ${'z'.repeat(40)}\n`));
  closeSync(fd);
}

/** 带重试删除（文件可能被 Windows 句柄短暂占用）。 */
export function removeWithRetry(path, attempts = 24) {
  for (let i = 0; i < attempts; i += 1) {
    try {
      rmSync(path, { recursive: true, force: true });
    } catch {
      // 忽略并重试
    }
    if (!existsSync(path)) return;
  }
}

/** 带重试删除（异步版）：每轮之间等待 delayMs，适合进程刚被杀、句柄尚未释放的套件清理。 */
export async function removeWithRetryAsync(path, attempts = 20, delayMs = 300) {
  for (let i = 0; i < attempts; i += 1) {
    try {
      rmSync(path, { recursive: true, force: true });
    } catch {
      // 忽略并重试
    }
    if (!existsSync(path)) return true;
    await new Promise((resolve) => setTimeout(resolve, delayMs));
  }
  return false;
}

/**
 * 进程树详情：每个进程的名称、角色、专用工作集、工作集。
 * 口径：WorkingSetPrivate 与任务管理器「内存」一致（mmap 文件缓存页不计入）；
 *       工作集（WorkingSetSize）含可回收的文件缓存页，仅作参考。
 */
export function sampleProcessTable(rootPid) {
  const out = spawnSync(
    'powershell',
    [
      '-NoProfile',
      '-Command',
      `$procs = Get-CimInstance Win32_Process | ForEach-Object { "P,$($_.ProcessId),$($_.ParentProcessId),$($_.Name),$($_.WorkingSetSize),$($_.CommandLine)" }; $mem = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | ForEach-Object { "W,$($_.IDProcess),$($_.WorkingSetPrivate)" }; $procs; $mem`,
    ],
    { encoding: 'utf8', maxBuffer: 128 * 1024 * 1024 },
  );
  const children = new Map();
  const info = new Map();
  const wsPrivate = new Map();
  for (const line of (out.stdout ?? '').split(/\r?\n/)) {
    if (line.startsWith('P,')) {
      // CommandLine 可能含逗号：仅前 5 段固定，其余合并
      const parts = line.slice(2).split(',');
      if (parts.length < 5) continue;
      const pid = Number(parts[0]);
      const ppid = Number(parts[1]);
      const name = parts[2];
      const wsSize = Number(parts[3]);
      const cmdline = parts.slice(4).join(',');
      if (!Number.isFinite(pid)) continue;
      info.set(pid, { name, wsSize, cmdline });
      if (!children.has(ppid)) children.set(ppid, []);
      children.get(ppid).push(pid);
    } else if (line.startsWith('W,')) {
      const parts = line.slice(2).split(',');
      const pid = Number(parts[0]);
      const size = Number(parts[1]);
      if (Number.isFinite(pid) && Number.isFinite(size)) wsPrivate.set(pid, size);
    }
  }
  const rows = [];
  const queue = [rootPid];
  const seen = new Set();
  while (queue.length > 0) {
    const pid = queue.shift();
    if (seen.has(pid)) continue;
    seen.add(pid);
    const meta = info.get(pid);
    if (meta) {
      rows.push({
        pid,
        name: meta.name,
        role: classifyRole(meta.name, meta.cmdline),
        privateMb: (wsPrivate.get(pid) ?? meta.wsSize) / (1024 * 1024),
        workingSetMb: meta.wsSize / (1024 * 1024),
      });
    }
    for (const kid of children.get(pid) ?? []) queue.push(kid);
  }
  return rows;
}

/** 依命令行归类 WebView2 进程角色（便于构成分析）。 */
function classifyRole(name, cmdline) {
  const text = cmdline ?? '';
  if (/--type=renderer/.test(text)) return 'webview-renderer';
  if (/--type=gpu-process/.test(text)) return 'webview-gpu';
  if (/--type=utility/.test(text)) return 'webview-utility';
  if (/--type=crashpad-handler/.test(text)) return 'webview-crashpad';
  if (/--type=/.test(text)) return 'webview-browser';
  if (/s-read-txt/i.test(name)) return 'app';
  return 'other';
}

/** 进程树内存合计（stress 用简洁接口）。 */
export function sampleMemoryTree(rootPid) {
  const rows = sampleProcessTable(rootPid);
  return {
    privateMb: rows.reduce((sum, row) => sum + row.privateMb, 0),
    workingSetMb: rows.reduce((sum, row) => sum + row.workingSetMb, 0),
    count: rows.length,
  };
}
