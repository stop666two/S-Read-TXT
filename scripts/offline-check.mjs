#!/usr/bin/env node
// 离线核查：
//  A. 静态：依赖树扫描——Cargo.lock / package.json 中不应出现 HTTP 客户端库（本项目零联网）。
//  B. 运行时：启动应用（无调试端口）→ 采样进程树 TCP 连接 → 不应存在对外（非本机）连接。
// 用法：node scripts/offline-check.mjs [--exe <路径>]

import { spawn, spawnSync } from 'node:child_process';
import { existsSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, delay } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(
  argValue('--exe', join(root, 'src-tauri', 'target', 'release', 's-read-txt.exe')),
);
const results = [];
function check(name, passed, detail = '') {
  results.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

// ---- A. 依赖树扫描 ----
// 权威口径：Windows 主机目标的实际构建图（cargo tree 默认目标）。
// Cargo.lock 为全平台解析，会包含 tauri 在其他平台/特性下的依赖（如 reqwest），不作失败条件。
const cargoTree = spawnSync('cargo', ['tree', '--prefix', 'none'], {
  cwd: join(root, 'src-tauri'),
  encoding: 'utf8',
  maxBuffer: 64 * 1024 * 1024,
});
const httpCrates = ['reqwest', 'hyper', 'ureq', 'isahc', 'attohttpc', 'curl', 'surf', 'awc'];
const treeNames = new Set(
  (cargoTree.stdout ?? '')
    .split(/\r?\n/)
    .map((line) => line.trim().split(' ')[0])
    .filter(Boolean),
);
const foundCrates = httpCrates.filter((name) => treeNames.has(name));
check(
  'A1 Windows 目标依赖树无 HTTP 客户端（cargo tree 实测）',
  foundCrates.length === 0,
  foundCrates.length === 0 ? `扫描 ${treeNames.size} 个 crate，未发现` : `发现：${foundCrates.join(', ')}`,
);

const cargoLock = join(root, 'src-tauri', 'Cargo.lock');
const lockText = existsSync(cargoLock) ? (await import('node:fs')).readFileSync(cargoLock, 'utf8') : '';
const lockHas = httpCrates.filter((name) => new RegExp(`^name = "${name}"$`, 'm').test(lockText));
check(
  'A1b 锁文件全平台条目（信息性）',
  true,
  lockHas.length > 0
    ? `锁文件含 ${lockHas.join(', ')}（跨平台解析；未编译进 Windows 产物）`
    : '锁文件无相关条目',
);

const pkg = JSON.parse(
  (await import('node:fs')).readFileSync(join(root, 'package.json'), 'utf8'),
);
const allDeps = { ...pkg.dependencies, ...pkg.devDependencies };
const httpJs = ['axios', 'node-fetch', 'got', 'undici', 'superagent', 'request'];
const foundJs = httpJs.filter((name) => name in allDeps);
check('A2 前端依赖无 HTTP 客户端', foundJs.length === 0, foundJs.length === 0 ? '未发现' : `发现：${foundJs.join(', ')}`);

// ---- B. 运行时连接采样 ----
if (!existsSync(exePath)) {
  console.error(`可执行文件不存在：${exePath}`);
  process.exit(2);
}
const dataDir = join(process.env.TEMP ?? '.', 'srt-offline', `data-${Date.now()}`);
const child = spawn(exePath, [], {
  env: { ...process.env, SRT_DATA_DIR: dataDir },
  stdio: 'ignore',
});

/** 读取进程树的连接（Get-NetTCPConnection 按 PID 过滤；仅返回非 Listen 状态）。 */
function sampleConnections(rootPid) {
  const script = `
    $procs = Get-CimInstance Win32_Process | ForEach-Object { "$($_.ProcessId),$($_.ParentProcessId)" }
    $children = @{}
    foreach ($line in $procs) { $parts = $line.Split(','); if ($parts.Length -eq 2) { $kids = $children[$parts[1]]; if ($null -eq $kids) { $kids = @() }; $kids += $parts[0]; $children[$parts[1]] = $kids } }
    $queue = New-Object System.Collections.Queue
    $queue.Enqueue(${rootPid})
    $seen = @{}
    while ($queue.Count -gt 0) {
      $pid = $queue.Dequeue()
      if ($seen.ContainsKey($pid)) { continue }
      $seen[$pid] = $true
      $kids = $children["$pid"]
      if ($null -ne $kids) { foreach ($kid in $kids) { $queue.Enqueue($kid) } }
    }
    $conns = Get-NetTCPConnection -ErrorAction SilentlyContinue | Where-Object { $seen.ContainsKey([string]$_.OwningProcess) -and $_.State -ne 'Listen' -and $_.State -ne 'Bound' }
    $conns | ForEach-Object { "$($_.State)|$($_.RemoteAddress)|$($_.RemotePort)" }
  `;
  const out = spawnSync('powershell', ['-NoProfile', '-Command', script], {
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  return (out.stdout ?? '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.includes('|'));
}

try {
  await delay(4000); // 等待窗口与 WebView 启动完成
  const samples = [];
  for (let i = 0; i < 6; i += 1) {
    samples.push(...sampleConnections(child.pid));
    await delay(1200);
  }
  const external = samples.filter((line) => {
    const [, address] = line.split('|');
    return (
      address !== '127.0.0.1' &&
      address !== '::1' &&
      address !== '0.0.0.0' &&
      address !== '::' &&
      !address.startsWith('127.')
    );
  });
  check(
    'B1 运行时无对外网络连接（进程树 6 次采样）',
    external.length === 0,
    external.length === 0 ? `采样连接数 ${samples.length}（均为本机/无）` : `发现对外：${external.join(' ; ')}`,
  );
} finally {
  if (child.pid) {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  }
  await delay(400);
  try {
    rmSync(dataDir, { recursive: true, force: true });
  } catch {
    // 忽略
  }
}

const failed = results.filter((item) => !item.passed);
console.log(`\n离线核查结果：${results.length - failed.length}/${results.length} 通过`);
process.exit(failed.length === 0 ? 0 : 1);
