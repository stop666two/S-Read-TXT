#!/usr/bin/env node
// 快捷键动作对齐检查（P0-9）：Rust 默认表与前端动作列表必须一致。
// 用途：新增/删除快捷键动作时，双端漏改会在此处失败（verify-all 一环节）。
import { readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const rust = readFileSync(join(root, 'src-tauri', 'src', 'settings', 'defaults.rs'), 'utf8');
const ts = readFileSync(join(root, 'src', 'lib', 'shortcuts', 'types.ts'), 'utf8');

const rustBlock = rust.match(/DEFAULT_BINDINGS: &\[\(&str, &str\)\] = &\[([\s\S]*?)\];/);
if (!rustBlock) {
  console.error('未在 defaults.rs 找到 DEFAULT_BINDINGS');
  process.exit(1);
}
const rustActions = [...rustBlock[1].matchAll(/\("([A-Za-z0-9]+)",\s*"([^"]+)"\)/g)].map((m) => m[1]);

const tsBlock = ts.match(/SHORTCUT_ACTIONS[^=]*=\s*\[([\s\S]*?)\];/);
if (!tsBlock) {
  console.error('未在 types.ts 找到 SHORTCUT_ACTIONS');
  process.exit(1);
}
const tsActions = [...tsBlock[1].matchAll(/'([A-Za-z0-9]+)'/g)].map((m) => m[1]);

const missingInTs = rustActions.filter((action) => !tsActions.includes(action));
const missingInRust = tsActions.filter((action) => !rustActions.includes(action));
if (rustActions.length !== tsActions.length || missingInTs.length > 0 || missingInRust.length > 0) {
  console.error('快捷键动作不一致：');
  console.error(`  Rust（${rustActions.length}）：${rustActions.join(', ')}`);
  console.error(`  前端（${tsActions.length}）：${tsActions.join(', ')}`);
  if (missingInTs.length > 0) console.error(`  前端缺少：${missingInTs.join(', ')}`);
  if (missingInRust.length > 0) console.error(`  Rust 缺少：${missingInRust.join(', ')}`);
  process.exit(1);
}
console.log(`快捷键动作对齐：${rustActions.length} 项一致`);
