#!/usr/bin/env node
// 全量自检（verify-all）：一条命令串行跑完整个质量门禁，并产出报告 docs/verify/latest.md。
//
// 顺序（关键，勿随意调整）：
//   ① 编码自检 → ② svelte-check → ③ vitest → ④ cargo fmt 检查 → ⑤ cargo test（全目标）
//   → ⑥ Tauri 构建 debug（必须：cargo 测试会把 exe 覆盖为 dev 语义）
//   → ⑦ 全部 E2E 冒烟套件（必须串行：并发运行会互相干扰）
// 判定：任一步非零退出即整体失败（exit 1）。
//
// 用法：
//   node scripts/verify-all.mjs                 # 全量（含 100MB 长行套件）
//   node scripts/verify-all.mjs --skip-longline # 跳过 100MB 套件（快速回归）
//   node scripts/verify-all.mjs --only cargo,svelte-check  # 仅跑名称包含逗号子串的步骤
//   node scripts/verify-all.mjs --skip-build    # 跳过构建（复用现有 exe；仍会跑冒烟）

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const tauriDir = join(root, 'src-tauri');
const MSYS = 'D:\\msys64\\ucrt64\\bin';
const DEV_TARGET = join(tauriDir, 'target', 'debug');

/** 构建类命令环境：PATH 前置 MSYS2（防 Tesseract 旧 DLL 遮蔽）与 target/debug（WebView2Loader） */
const buildEnv = {
  ...process.env,
  PATH: `${MSYS};${DEV_TARGET};${process.env.PATH ?? ''}`,
};

const skipLongline = process.argv.includes('--skip-longline');
const skipBuild = process.argv.includes('--skip-build');
const onlyArg = process.argv.indexOf('--only');
const only = onlyArg >= 0 && process.argv[onlyArg + 1] ? process.argv[onlyArg + 1].split(',') : null;

/** 检查步骤定义（cmd 全为受控字符串；shell 执行以便直接用 npm/npx） */
const steps = [
  { name: '编码自检', cmd: 'node scripts/check-encoding.mjs', cwd: root, env: process.env, timeout: 60_000 },
  { name: 'svelte-check', cmd: 'npm run check', cwd: root, env: process.env, timeout: 300_000 },
  { name: 'vitest 单测', cmd: 'npx vitest run', cwd: root, env: process.env, timeout: 300_000 },
  { name: 'cargo fmt 检查', cmd: 'cargo fmt --check', cwd: tauriDir, env: buildEnv, timeout: 120_000 },
  { name: 'cargo test（全目标）', cmd: 'cargo test', cwd: tauriDir, env: buildEnv, timeout: 600_000 },
  ...(skipBuild
    ? []
    : [
        {
          name: 'Tauri 构建（debug）',
          cmd: 'npm run tauri build -- --debug --no-bundle',
          cwd: root,
          env: buildEnv,
          timeout: 900_000,
        },
      ]),
  { name: 'E2E 编辑（smoke-edit）', cmd: 'node scripts/smoke-edit.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 查找（smoke-find）', cmd: 'node scripts/smoke-find.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 输入法（smoke-ime）', cmd: 'node scripts/smoke-ime.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 多语言（smoke-i18n）', cmd: 'node scripts/smoke-i18n.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 标题栏（smoke-titlebar）', cmd: 'node scripts/smoke-titlebar.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 全按钮（smoke-buttons）', cmd: 'node scripts/smoke-buttons.mjs', cwd: root, env: process.env, timeout: 900_000 },
  { name: 'E2E 设置窗口（smoke-settings）', cmd: 'node scripts/smoke-settings.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 快捷键（smoke-shortcuts）', cmd: 'node scripts/smoke-shortcuts.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 多标签（smoke-tabs）', cmd: 'node scripts/smoke-tabs.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 历史记录（smoke-history）', cmd: 'node scripts/smoke-history.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 会话恢复（smoke-session）', cmd: 'node scripts/smoke-session.mjs', cwd: root, env: process.env, timeout: 900_000 },
  { name: 'E2E 数据目录引导（smoke-datadir）', cmd: 'node scripts/smoke-datadir.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 对抗（smoke-abuse）', cmd: 'node scripts/smoke-abuse.mjs', cwd: root, env: process.env, timeout: 900_000 },
  ...(skipLongline
    ? []
    : [
        {
          name: 'E2E 100MB 长行（smoke-longline）',
          cmd: 'node scripts/smoke-longline.mjs',
          cwd: root,
          env: process.env,
          timeout: 600_000,
        },
      ]),
];

/** 截取输出尾部（报告与失败诊断用） */
function tail(text, maxChars = 700) {
  const trimmed = (text ?? '').trim();
  return trimmed.length <= maxChars ? trimmed : `…${trimmed.slice(-maxChars)}`;
}

function gitInfo() {
  const head = spawnSync('git', ['rev-parse', '--short', 'HEAD'], { cwd: root, encoding: 'utf8' });
  const status = spawnSync('git', ['status', '--porcelain'], { cwd: root, encoding: 'utf8' });
  const dirtyCount = (status.stdout ?? '').split('\n').filter((line) => line.trim()).length;
  return {
    commit: (head.stdout ?? '').trim() || 'unknown',
    dirtyCount,
  };
}

function main() {
  const selected = steps.filter((step) => !only || only.some((key) => step.name.includes(key)));
  if (selected.length === 0) {
    console.error(`--only 未匹配到任何步骤：${only?.join(',')}`);
    process.exit(2);
  }

  console.log(`全量自检开始：${selected.length} 个步骤（串行）\n`);
  const results = [];
  const startedAt = Date.now();

  for (const step of selected) {
    const stepStart = Date.now();
    process.stdout.write(`▶ ${step.name} … `);
    const run = spawnSync(step.cmd, {
      cwd: step.cwd,
      env: step.env,
      shell: true,
      encoding: 'utf8',
      timeout: step.timeout,
      maxBuffer: 64 * 1024 * 1024,
    });
    const durationSec = ((Date.now() - stepStart) / 1000).toFixed(1);
    const output = `${run.stdout ?? ''}${run.stderr ?? ''}`;
    const passed = run.status === 0;
    results.push({ name: step.name, passed, durationSec, output: tail(output, passed ? 700 : 6000) });
    console.log(
      passed ? `PASS（${durationSec}s）` : `FAIL（${durationSec}s，退出码 ${run.status ?? 'timeout'}）`,
    );
    if (!passed) {
      console.log(`  —— 输出尾部 ——\n${tail(output, 3000).replace(/^/gm, '  ')}`);
    }
  }

  const totalSec = ((Date.now() - startedAt) / 1000).toFixed(1);
  const failed = results.filter((item) => !item.passed);
  const { commit, dirtyCount } = gitInfo();

  // 报告落盘（覆盖式最新报告）
  const reportDir = join(root, 'docs', 'verify');
  mkdirSync(reportDir, { recursive: true });
  const now = new Date().toISOString();
  const lines = [
    '# 全量自检报告（最新一次）',
    '',
    `- 时间（UTC）：${now}`,
    `- 提交：${commit}（未提交变更 ${dirtyCount} 项）`,
    `- 结果：**${results.length - failed.length}/${results.length} 通过**，总耗时 ${totalSec}s`,
    '',
    '| 步骤 | 结果 | 耗时(s) |',
    '| --- | --- | --- |',
    ...results.map((item) => `| ${item.name} | ${item.passed ? '✅' : '❌'} | ${item.durationSec} |`),
    '',
  ];
  if (failed.length > 0) {
    lines.push('## 失败详情', '');
    for (const item of failed) {
      lines.push(`### ${item.name}`, '', '```', item.output, '```', '');
    }
  }
  writeFileSync(join(reportDir, 'latest.md'), `${lines.join('\n')}\n`, 'utf8');

  console.log(`\n全量自检：${results.length - failed.length}/${results.length} 通过，总耗时 ${totalSec}s`);
  console.log(`报告：docs/verify/latest.md`);
  if (failed.length > 0) {
    console.error(`失败步骤：${failed.map((item) => item.name).join('；')}`);
    process.exit(1);
  }
}

if (!existsSync(join(root, 'package.json'))) {
  console.error('未在项目根目录运行（缺少 package.json）');
  process.exit(2);
}
main();
