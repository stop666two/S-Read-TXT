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
//   node scripts/verify-all.mjs --exclude smoke-uninstall  # 排除环境受限步骤（附原因，计入「排除项」，不影响退出码）

import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const tauriDir = join(root, 'src-tauri');
const DEV_TARGET = join(tauriDir, 'target', 'debug');

/**
 * MSYS2 工具链目录探测（禁止硬编码目录）：
 * 1. 环境变量 `SRT_MSYS_BIN` 显式指定（最高优先；见 README 环境变量表）；
 * 2. 否则从当前 PATH 中取以 `ucrt64\bin` 结尾的条目（本机 MSYS2 安装位置）；
 * 3. 都未命中则为空——CI（MSVC）无需 MSYS。
 */
function detectMsysBin() {
  if (process.env.SRT_MSYS_BIN) return process.env.SRT_MSYS_BIN;
  const entries = (process.env.PATH ?? '').split(';').filter(Boolean);
  return entries.find((entry) => /[\\/]ucrt64[\\/]bin[\\/]?$/i.test(entry)) ?? '';
}

/** 构建类命令环境：PATH 前置 MSYS2（防旧 DLL 遮蔽）与 target/debug（WebView2Loader） */
const buildEnv = {
  ...process.env,
  PATH: [detectMsysBin(), DEV_TARGET, process.env.PATH ?? ''].filter(Boolean).join(';'),
};

const skipLongline = process.argv.includes('--skip-longline');
const skipBuild = process.argv.includes('--skip-build');
const onlyArg = process.argv.indexOf('--only');
const only = onlyArg >= 0 && process.argv[onlyArg + 1] ? process.argv[onlyArg + 1].split(',') : null;
const excludeArg = process.argv.indexOf('--exclude');
const exclude =
  excludeArg >= 0 && process.argv[excludeArg + 1] ? process.argv[excludeArg + 1].split(',') : [];

/** 已知环境排除项及原因（`--exclude` 按键命中时随报告与终端附注）。 */
const KNOWN_EXCLUSIONS = {
  'smoke-uninstall': '需提权运行安装包（UAC）；同意提权时可不带 --exclude 运行',
};

/** 检查步骤定义（cmd 全为受控字符串；shell 执行以便直接用 npm/npx） */
const steps = [
  { name: '编码自检', cmd: 'node scripts/check-encoding.mjs', cwd: root, env: process.env, timeout: 60_000 },
  { name: '快捷键动作对齐', cmd: 'node scripts/check-shortcut-parity.mjs', cwd: root, env: process.env, timeout: 30_000 },
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
  { name: 'E2E 基础（smoke）', cmd: 'node scripts/smoke.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 查找（smoke-find）', cmd: 'node scripts/smoke-find.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 批量序号（smoke-batch）', cmd: 'node scripts/smoke-batch.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 行操作（smoke-lineops）', cmd: 'node scripts/smoke-lineops.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 过滤视图（smoke-filter）', cmd: 'node scripts/smoke-filter.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 多光标（smoke-multi）', cmd: 'node scripts/smoke-multi.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 剪贴板历史（smoke-clipboard）', cmd: 'node scripts/smoke-clipboard.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 辅助编辑（smoke-tools）', cmd: 'node scripts/smoke-tools.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 输入法（smoke-ime）', cmd: 'node scripts/smoke-ime.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 多语言（smoke-i18n）', cmd: 'node scripts/smoke-i18n.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 工作区搜索（smoke-workspace）', cmd: 'node scripts/smoke-workspace.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 标题栏（smoke-titlebar）', cmd: 'node scripts/smoke-titlebar.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 全按钮（smoke-buttons）', cmd: 'node scripts/smoke-buttons.mjs', cwd: root, env: process.env, timeout: 900_000 },
  { name: 'E2E 设置窗口（smoke-settings）', cmd: 'node scripts/smoke-settings.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 状态栏（smoke-status）', cmd: 'node scripts/smoke-status.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 快照（smoke-snapshots）', cmd: 'node scripts/smoke-snapshots.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 新建/导出/打印（smoke-p32）', cmd: 'node scripts/smoke-p32.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 命令行/单实例（smoke-cli）', cmd: 'node scripts/smoke-cli.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 大纲（smoke-outline）', cmd: 'node scripts/smoke-outline.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 阅读模式（smoke-reading）', cmd: 'node scripts/smoke-reading.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 标注（smoke-annotations）', cmd: 'node scripts/smoke-annotations.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 显示选项（smoke-display）', cmd: 'node scripts/smoke-display.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 设置 I/O（smoke-settings-io）', cmd: 'node scripts/smoke-settings-io.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 设置 v2（smoke-settings-v2）', cmd: 'node scripts/smoke-settings-v2.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 磁盘占用（smoke-disk）', cmd: 'node scripts/smoke-disk.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 数据目录迁移（smoke-migrate）', cmd: 'node scripts/smoke-migrate.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 快捷键（smoke-shortcuts）', cmd: 'node scripts/smoke-shortcuts.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 多标签（smoke-tabs）', cmd: 'node scripts/smoke-tabs.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 多窗口（smoke-windows）', cmd: 'node scripts/smoke-windows.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 分屏（smoke-split）', cmd: 'node scripts/smoke-split.mjs', cwd: root, env: process.env, timeout: 900_000 },
  { name: 'E2E 工具功能（smoke-utility）', cmd: 'node scripts/smoke-utility.mjs', cwd: root, env: process.env, timeout: 900_000 },
  { name: 'E2E 比较与合并（smoke-compare）', cmd: 'node scripts/smoke-compare.mjs', cwd: root, env: process.env, timeout: 900_000 },
  { name: 'E2E 历史记录（smoke-history）', cmd: 'node scripts/smoke-history.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 会话恢复（smoke-session）', cmd: 'node scripts/smoke-session.mjs', cwd: root, env: process.env, timeout: 900_000 },
  { name: 'E2E 数据目录引导（smoke-datadir）', cmd: 'node scripts/smoke-datadir.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 卸载清理（smoke-uninstall）', cmd: 'node scripts/smoke-uninstall.mjs', cwd: root, env: process.env, timeout: 900_000 },
  { name: 'E2E 对抗（smoke-abuse）', cmd: 'node scripts/smoke-abuse.mjs', cwd: root, env: process.env, timeout: 900_000 },
  { name: 'E2E 滚动完整性（smoke-scroll）', cmd: 'node scripts/smoke-scroll.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 主题系统（smoke-theme）', cmd: 'node scripts/smoke-theme.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 背景图（smoke-bg）', cmd: 'node scripts/smoke-bg.mjs', cwd: root, env: process.env, timeout: 600_000 },
  { name: 'E2E 双阈值（smoke-limits）', cmd: 'node scripts/smoke-limits.mjs', cwd: root, env: process.env, timeout: 600_000 },
  {
    name: '离线核查（offline-check）',
    cmd: 'node scripts/offline-check.mjs --exe src-tauri/target/debug/s-read-txt.exe',
    cwd: root,
    env: process.env,
    timeout: 300_000,
  },
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
  const selectedAll = steps.filter((step) => !only || only.some((key) => step.name.includes(key)));
  if (selectedAll.length === 0) {
    console.error(`--only 未匹配到任何步骤：${only?.join(',')}`);
    process.exit(2);
  }

  const selected = [];
  const excludedSteps = [];
  for (const step of selectedAll) {
    const matchedKey = exclude.find((key) => step.name.includes(key));
    if (matchedKey) {
      const reason = KNOWN_EXCLUSIONS[matchedKey] ?? '用户指定排除';
      excludedSteps.push({ name: step.name, reason });
    } else {
      selected.push(step);
    }
  }
  for (const item of excludedSteps) {
    console.log(`⏭ 排除：${item.name}（${item.reason}）`);
  }

  console.log(
    `全量自检开始：${selected.length} 个步骤（串行${excludedSteps.length > 0 ? `，另有 ${excludedSteps.length} 项排除` : ''}）\n`,
  );
  const results = [];
  const startedAt = Date.now();

  for (const step of selected) {
    const stepStart = Date.now();
    process.stdout.write(`▶ ${step.name} … `);
    const runStep = () =>
      spawnSync(step.cmd, {
        cwd: step.cwd,
        env: step.env,
        shell: true,
        encoding: 'utf8',
        timeout: step.timeout,
        maxBuffer: 64 * 1024 * 1024,
      });
    let run = runStep();
    // E2E 启动偶发（高负载下应用未在就绪超时内出现 CDP 目标）：
    // 仅对 E2E 步骤自动重试一次，报告透明标注「重跑通过」；门禁类步骤（构建/单测）不重试。
    let retried = false;
    if (run.status !== 0 && step.name.startsWith('E2E')) {
      retried = true;
      console.log('重试一次 …');
      process.stdout.write(`▶ ${step.name}（重试） … `);
      run = runStep();
    }
    const durationSec = ((Date.now() - stepStart) / 1000).toFixed(1);
    const output = `${run.stdout ?? ''}${run.stderr ?? ''}`;
    const passed = run.status === 0;
    results.push({
      name: step.name,
      passed,
      retried,
      durationSec,
      output: tail(output, passed ? 700 : 6000),
    });
    console.log(
      passed
        ? `PASS（${durationSec}s${retried ? '，重跑通过' : ''}）`
        : `FAIL（${durationSec}s，退出码 ${run.status ?? 'timeout'}）`,
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
    `- 结果：**${results.length - failed.length}/${results.length} 通过**${
      excludedSteps.length > 0 ? `（另有 ${excludedSteps.length} 项排除）` : ''
    }，总耗时 ${totalSec}s`,
    '',
    '| 步骤 | 结果 | 耗时(s) |',
    '| --- | --- | --- |',
    ...results.map(
      (item) =>
        `| ${item.name} | ${item.passed ? `✅${item.retried ? '（重跑通过）' : ''}` : '❌'} | ${item.durationSec} |`,
    ),
    ...excludedSteps.map((item) => `| ${item.name} | ⏭ 排除 | — |`),
    '',
  ];
  if (excludedSteps.length > 0) {
    lines.push('## 排除项', '');
    for (const item of excludedSteps) {
      lines.push(`- ${item.name}：${item.reason}`);
    }
    lines.push('');
  }
  if (failed.length > 0) {
    lines.push('## 失败详情', '');
    for (const item of failed) {
      lines.push(`### ${item.name}`, '', '```', item.output, '```', '');
    }
  }
    writeFileSync(join(reportDir, 'latest.md'), `${lines.join('\n').replace(/\r\n?/g, '\n')}\n`, 'utf8');

  console.log(
    `\n全量自检：${results.length - failed.length}/${results.length} 通过${
      excludedSteps.length > 0 ? `（另有 ${excludedSteps.length} 项排除）` : ''
    }，总耗时 ${totalSec}s`,
  );
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
