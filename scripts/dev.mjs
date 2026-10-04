#!/usr/bin/env node
// 项目内开发辅助入口：所有操作均在当前项目目录内执行，命令本身不引用外部绝对路径
// （工具链路径由脚本内部探测，见 scripts/lib/toolchain.mjs）。
//
// 用法：
//   node scripts/dev.mjs check          # svelte-check + vitest
//   node scripts/dev.mjs test           # cargo test（自动注入工具链 PATH）
//   node scripts/dev.mjs fmt            # cargo fmt --check
//   node scripts/dev.mjs build          # Tauri debug 构建（--no-bundle，供 E2E 使用）
//   node scripts/dev.mjs build-release  # Tauri release 构建（NSIS + 便携包前置产物）
//   node scripts/dev.mjs smoke edit     # 运行 scripts/smoke-edit.mjs（名称省略 smoke- 前缀）
import { spawnSync } from 'node:child_process';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { buildEnv, detectMsysBin } from './lib/toolchain.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const tauriDir = join(root, 'src-tauri');
const command = process.argv[2];
const arg = process.argv[3];

/** 运行子命令（shell 形式，便于直接使用 npm/npx；返回退出码）。 */
function run(cmd, cwd, env = process.env) {
  const result = spawnSync(cmd, { shell: true, cwd, env, stdio: 'inherit' });
  return result.status ?? 1;
}

let code = 0;
switch (command) {
  case 'check':
    code = run('npm run check', root) || run('npx vitest run', root);
    break;
  case 'test':
    if (!detectMsysBin()) {
      console.error('提示：未探测到 MSYS2 ucrt64\\bin（可通过环境变量 SRT_MSYS_BIN 指定）。');
    }
    code = run('cargo test', tauriDir, buildEnv({ root }));
    break;
  case 'fmt':
    code = run('cargo fmt --check', tauriDir, buildEnv({ root }));
    break;
  case 'build':
    code = run('npm run tauri build -- --debug --no-bundle', root, buildEnv({ root }));
    break;
  case 'build-release':
    code = run('npm run tauri build', root, buildEnv({ root }));
    break;
  case 'smoke':
    if (!arg) {
      console.error('用法：node scripts/dev.mjs smoke <名称>（如 edit / find / status）');
      code = 2;
      break;
    }
    code = run(`node scripts/smoke-${arg}.mjs`, root);
    break;
  default:
    console.error('用法：node scripts/dev.mjs check|test|fmt|build|build-release|smoke <名称>');
    code = 2;
}
process.exit(code);
