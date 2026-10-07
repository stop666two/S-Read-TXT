#!/usr/bin/env node
// 便携版打包：exe + WebView2Loader.dll + LICENSE + 便携版说明 → zip。
//
// 用法：node scripts/make-portable.mjs [--exe <路径>] [--arch x64] [--out <目录>]
//   --exe   可执行文件路径（默认 src-tauri/target/release/s-read-txt.exe）
//   --arch  架构标签（x64 / x86 / arm64；仅用于 zip 文件名，默认 x64）
//   --out   输出目录（默认 portable-dist）
// 前置：先完成 release 构建（可执行文件与 WebView2Loader.dll 必须同目录存在）。
// 特性：解压即用、数据全部写在解压目录 data/ 内、不写注册表、不写系统环境变量；
//       依赖系统已安装 WebView2 运行时（Win10/11 通常预装）。

import { spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const argValue = (name, fallback) => {
  const index = args.indexOf(name);
  return index >= 0 && args[index + 1] ? args[index + 1] : fallback;
};

const pkg = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'));
const version = pkg.version;
const arch = argValue('--arch', 'x64');
const exe = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'release', 's-read-txt.exe')));
const outDir = resolve(argValue('--out', join(root, 'portable-dist')));
const dll = join(dirname(exe), 'WebView2Loader.dll');
const license = join(root, 'LICENSE');

for (const [label, path] of [
  ['可执行文件', exe],
  ['WebView2Loader.dll', dll],
  ['LICENSE', license],
]) {
  if (!existsSync(path)) {
    console.error(`缺少${label}：${path}`);
    console.error('请先完成 release 构建：npm run tauri build -- --target <target>');
    process.exit(1);
  }
}

const zipName = `S-Read-TXT_${version}_${arch}-portable.zip`;
const zipPath = join(outDir, zipName);
const staging = join(outDir, '.staging');
rmSync(staging, { recursive: true, force: true });
mkdirSync(staging, { recursive: true });

const readmeLines = [
  `S-Read-TXT ${version}（便携版）`,
  '========================================',
  '',
  '1. 解压到任意可写目录（本地磁盘或 U 盘均可），双击 s-read-txt.exe 即可使用；',
  '   启动不需要管理员权限（除非所选文件夹本身受限，届时系统会提示）。',
  '2. 全部数据（设置/历史/会话/阅读进度）都写入本目录下的 data\\ 文件夹；',
  '   整个文件夹可随时复制到其他电脑继续使用，删除文件夹即完成卸载。',
  '3. 默认不写注册表、不写系统环境变量、完全离线（无联网、无遥测）。',
  '   如需「用 S-Read-TXT 打开」等系统集成，可在 设置 → 系统 中手动开启：',
  '   - 当前用户级：写 HKCU，不需要管理员权限；',
  '   - 全局级：会弹出一次 UAC 请求；其余功能不受影响，可随时注销。',
  '4. 若更换了文件夹位置或出现「打开方式」残留，可运行同目录下的',
  '   「清理右键与打开方式.bat」一键清除（不需要管理员权限）。',
  '5. 系统要求：Windows 10 1803 或更高（x64 / x86 / ARM64）。',
  '6. 依赖系统 WebView2 运行时（Windows 10/11 通常已预装；缺失时请安装',
  '   微软官方 Evergreen 运行时）。',
  '',
];
writeFileSync(join(staging, '便携版说明.txt'), readmeLines.join('\r\n'), 'utf8');
copyFileSync(exe, join(staging, 's-read-txt.exe'));
copyFileSync(dll, join(staging, 'WebView2Loader.dll'));
copyFileSync(license, join(staging, 'LICENSE'));
const cleanupScript = join(root, 'packaging', 'cleanup-shell-integration.bat');
if (existsSync(cleanupScript)) {
  copyFileSync(cleanupScript, join(staging, '清理右键与打开方式.bat'));
  copyFileSync(cleanupScript, join(staging, 'cleanup-shell-integration.bat'));
} else {
  console.error(`缺少清理脚本：${cleanupScript}`);
  process.exit(1);
}

mkdirSync(outDir, { recursive: true });
rmSync(zipPath, { force: true });
const ps = spawnSync(
  'powershell',
  ['-NoProfile', '-Command', `Compress-Archive -Path '${staging}\\*' -DestinationPath '${zipPath}' -Force`],
  { stdio: 'inherit' },
);
if (ps.status !== 0 || !existsSync(zipPath)) {
  console.error('Compress-Archive 打包失败');
  process.exit(1);
}
rmSync(staging, { recursive: true, force: true });
const size = statSync(zipPath).size;
console.log(`便携版已生成：${zipPath}（${(size / 1024 / 1024).toFixed(2)} MiB）`);
