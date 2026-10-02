// S-Read-TXT 卸载清理 E2E（阶段 9 补充）
// 目的：验证卸载器会清除便携数据目录（data/）与注册表残留，且不再留下安装目录。
// 流程：预清理（若已安装先卸载）→ 静默安装（/S /currentuser）
//   → 人工造 data/ 数据（等价于运行过应用）
//   → 静默卸载（/S；钩子 /SD IDYES 默认删除数据）
//   → 断言：安装目录（含 data）消失、卸载注册表项（HKCU/HKLM）均已删除。
// 前置：存在 NSIS 安装包（先 `npm run tauri build`）；不存在时跳过（退出码 0）。
//
// 注意：NSIS 多用户模式的安装器/卸载器带 `highestAvailable` 执行级别清单，
// Node 的 spawn/spawnSync 走 CreateProcess 会返回 ERROR_ELEVATION_REQUIRED（libuv 映射为 EACCES），
// 因此统一经 ShellExecute（PowerShell Start-Process）运行并等待退出。
//
// 用法：node scripts/smoke-uninstall.mjs

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = process.cwd();
const bundleDir = join(root, 'src-tauri', 'target', 'release', 'bundle', 'nsis');

/** 候选安装目录（静默安装未指定范围时，按权限落入其一；CurrentUser 模式为 %LOCALAPPDATA%\Programs） */
const INSTALL_CANDIDATES = [
  process.env.LOCALAPPDATA ? join(process.env.LOCALAPPDATA, 'Programs', 'S-Read-TXT') : null,
  process.env.LOCALAPPDATA ? join(process.env.LOCALAPPDATA, 'S-Read-TXT') : null,
  process.env.ProgramFiles ? join(process.env.ProgramFiles, 'S-Read-TXT') : null,
  process.env['ProgramFiles(x86)'] ? join(process.env['ProgramFiles(x86)'], 'S-Read-TXT') : null,
].filter((value) => value !== null);

/** 候选卸载注册表项（per-user / per-machine） */
const UNINSTALL_KEYS = [
  'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\S-Read-TXT',
  'HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\S-Read-TXT',
];

let passed = 0;
let failed = 0;
const failures = [];

/** 断言与结果输出（中文） */
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

/** 睡眠 */
function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** 轮询等待条件成立 */
async function waitFor(fn, timeoutMs, intervalMs = 500) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (fn()) return true;
    await sleep(intervalMs);
  }
  return false;
}

/** 找到构建产物中的安装包（取修改时间最新者） */
function findSetup() {
  if (!existsSync(bundleDir)) return null;
  const setups = readdirSync(bundleDir)
    .filter((name) => name.endsWith('-setup.exe'))
    .map((name) => join(bundleDir, name));
  return setups.length > 0 ? setups[0] : null;
}

/** 经 ShellExecute（PowerShell Start-Process）运行并等待退出。
 *  返回 `{ code, timedOut }`；`code` 为 null 表示未能获取退出码（启动失败/超时）。 */
function runExe(exe, args, timeoutMs) {
  const quote = (value) => `'${String(value).replace(/'/g, "''")}'`;
  const argList = args.map(quote).join(',');
  const script = [
    `$p = Start-Process -FilePath ${quote(exe)} -ArgumentList ${argList} -PassThru`,
    `if (-not $p.WaitForExit(${timeoutMs})) { Write-Output 'TIMEOUT'; exit 3 }`,
    `Write-Output ("EXIT:" + $p.ExitCode)`,
  ].join('; ');
  const result = spawnSync('powershell', ['-NoProfile', '-Command', script], {
    encoding: 'utf8',
    timeout: timeoutMs + 30000,
  });
  const out = ((result.stdout ?? '').trim().split(/\r?\n/).pop() ?? '').trim();
  if (out === 'TIMEOUT') return { code: null, timedOut: true };
  const match = out.match(/^EXIT:(-?\d+)$/);
  return { code: match ? Number(match[1]) : null, timedOut: false };
}

/** 注册表项是否存在 */
function registryKeyExists(key) {
  const result = spawnSync('reg', ['query', key], { encoding: 'utf8', timeout: 20000 });
  return result.status === 0;
}

/** 从注册表读取安装位置（优先；CurrentUser 模式由 MultiUser 写入），失败返回 null */
function queryInstallLocation() {
  for (const key of UNINSTALL_KEYS) {
    const result = spawnSync('reg', ['query', key, '/v', 'InstallLocation'], {
      encoding: 'utf8',
      timeout: 15000,
    });
    if (result.status !== 0) continue;
    const match = (result.stdout ?? '').match(/InstallLocation\s+REG_SZ\s+(.+)/);
    if (match) return match[1].trim().replace(/^"|"$/g, '');
  }
  return null;
}

/** 当前落位的安装目录（注册表优先，其次候选路径；未安装返回 null） */
function findInstalledDir() {
  const fromRegistry = queryInstallLocation();
  if (fromRegistry && existsSync(join(fromRegistry, 's-read-txt.exe'))) return fromRegistry;
  return INSTALL_CANDIDATES.find((dir) => existsSync(join(dir, 's-read-txt.exe'))) ?? null;
}

/** 是否存在正在运行的 S-Read-TXT 实例（存在时卸载器会弹「请先关闭应用」并等待）。 */
function runningInstanceExists() {
  const result = spawnSync(
    'tasklist',
    ['/FI', 'IMAGENAME eq s-read-txt.exe', '/FO', 'CSV', '/NH'],
    { encoding: 'utf8', timeout: 20000 },
  );
  return /s-read-txt\.exe/i.test(result.stdout ?? '');
}

async function main() {
  const setup = findSetup();
  if (!setup) {
    console.log('跳过：未找到 NSIS 安装包（先运行 npm run tauri build 生成 bundle）');
    return;
  }
  console.log(`安装包：${setup}`);

  // ---- U0：预清理（若已安装，先卸载到干净状态，保证流程确定性） ----
  const preexisting = findInstalledDir();
  if (preexisting) {
    const uninstaller = join(preexisting, 'uninstall.exe');
    if (existsSync(uninstaller)) {
      await runExe(uninstaller, ['/S'], 120000);
      await waitFor(() => !existsSync(preexisting), 120000);
    }
  }

  // ---- U1/U2：静默安装（/currentuser 为确定性的当前用户安装；失败再试默认） ----
  let installDir = null;
  const installAttempts = [
    ['/S', '/currentuser'],
    ['/S', '/currentuser'],
    ['/S'],
  ];
  for (const args of installAttempts) {
    const result = await runExe(setup, args, 300000);
    installDir = (await waitFor(() => findInstalledDir(), 45000)) ? findInstalledDir() : null;
    if (installDir !== null) {
      check('U1 静默安装成功', result.code === 0, `args=${args.join(' ')} code=${result.code} dir=${installDir}`);
      break;
    }
    await sleep(2000);
  }
  check('U2 安装目录存在', installDir !== null, installDir ?? '未找到（候选：' + INSTALL_CANDIDATES.join(' | ') + '）');
  if (!installDir) {
    console.log(`\n卸载清理冒烟：${passed}/${passed + failed} 通过（提前终止）`);
    console.log(`失败项：${failures.join('；')}`);
    process.exitCode = 1;
    return;
  }

  // ---- U3：造数据（等价于运行过应用：历史/日志） ----
  mkdirSync(join(installDir, 'data', 'logs'), { recursive: true });
  writeFileSync(join(installDir, 'data', 'history.jsonl'), '{"probe":true}\n', 'utf8');
  writeFileSync(join(installDir, 'data', 'logs', 'app.log'), 'probe\n', 'utf8');
  check('U3 数据标记写入', existsSync(join(installDir, 'data', 'history.jsonl')));

  // ---- U4：静默卸载（钩子默认删除数据；应用必须已退出，否则卸载器会等待） ----
  if (runningInstanceExists()) {
    console.log('跳过：检测到正在运行的 S-Read-TXT 实例（请先退出应用后再运行本套件）');
    return;
  }
  const uninstaller = join(installDir, 'uninstall.exe');
  const unResult = await runExe(uninstaller, ['/S'], 120000);
  check('U4 静默卸载已启动', unResult.code === 0, `code=${unResult.code}${unResult.timedOut ? '（超时）' : ''}`);
  const gone = await waitFor(() => !existsSync(installDir), 120000);
  check('U5 安装目录（含 data）彻底删除', gone, installDir);

  // ---- U6：注册表残留清理 ----
  const remainingKeys = UNINSTALL_KEYS.filter((key) => registryKeyExists(key));
  check('U6 卸载注册表项已清除', remainingKeys.length === 0, remainingKeys.join('；') || '无残留');

  const total = passed + failed;
  console.log(`\n卸载清理冒烟：${passed}/${total} 通过`);
  if (failed > 0) {
    console.log(`失败项：${failures.join('；')}`);
    process.exitCode = 1;
  }
}

await main();
