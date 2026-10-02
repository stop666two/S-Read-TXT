#!/usr/bin/env node
// 测试摘要生成器（供 Release 描述与排障使用）。
//
// 用法：
//   node scripts/ci/collect-test-summary.mjs \
//     --cargo cargo-test.log       （cargo test 文本输出）
//     --vitest vitest-report.json  （vitest --reporter=json 输出；缺失时尝试文本日志）
//     --vitest-log vitest.log      （可选：vitest 文本输出，用于 JSON 缺失时回退）
//     --check check.log            （svelte-check 输出）
//     --out test-summary.md        （输出文件；省略则仅打印）
//
// 设计说明：解析保持宽容——日志缺失/格式变化时输出「（未提供）」而不报错，
// 保证随测试项持续补充时摘要仍可生成；真正的通过/失败由 CI 作业本身决定。

import { readFileSync, writeFileSync } from "node:fs";
import process from "node:process";

/** 解析 `--key value` 形式参数。 */
function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 1) {
    const key = argv[i];
    if (key.startsWith("--")) {
      args[key.slice(2)] = argv[i + 1];
      i += 1;
    }
  }
  return args;
}

/** 安全读取文本文件（不存在返回 null）。 */
function readText(path) {
  if (!path) return null;
  try {
    return readFileSync(path, "utf8");
  } catch {
    return null;
  }
}

/** 解析 cargo test 输出：汇总所有「test result」行的用例数。 */
function parseCargo(log) {
  if (!log) return null;
  const pattern = /test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored/g;
  let suites = 0;
  let passed = 0;
  let failed = 0;
  let ignored = 0;
  for (const match of log.matchAll(pattern)) {
    suites += 1;
    passed += Number(match[2]);
    failed += Number(match[3]);
    ignored += Number(match[4]);
  }
  if (suites === 0) return null;
  return { suites, passed, failed, ignored };
}

/** 解析 vitest JSON 报告（Jest 兼容格式）。 */
function parseVitestJson(jsonText) {
  if (!jsonText) return null;
  try {
    const report = JSON.parse(jsonText);
    const files = Array.isArray(report.testResults) ? report.testResults.length : 0;
    return {
      files,
      total: Number(report.numTotalTests ?? 0),
      passed: Number(report.numPassedTests ?? 0),
      failed: Number(report.numFailedTests ?? 0),
    };
  } catch {
    return null;
  }
}

/** 解析 svelte-check 输出（形如 `found 0 errors and 0 warnings`）。 */
function parseCheck(log) {
  if (!log) return null;
  const match = log.match(/found (\d+) errors? and (\d+) warnings?/);
  if (!match) return null;
  return { errors: Number(match[1]), warnings: Number(match[2]) };
}

/** 通过/失败标记。 */
function badge(failedCount) {
  return failedCount > 0 ? "❌ 失败" : "✅ 通过";
}

/** 主流程。 */
function main() {
  const args = parseArgs(process.argv.slice(2));
  const cargo = parseCargo(readText(args.cargo));
  const vitest =
    parseVitestJson(readText(args.vitest)) ??
    (readText(args["vitest-log"])?.match(/Tests\s+(\d+) passed/) ? null : null);
  const check = parseCheck(readText(args.check));

  const lines = [];
  lines.push("# 测试摘要");
  lines.push("");
  const stamp = new Date().toISOString().replace("T", " ").slice(0, 19);
  const sha = process.env.GITHUB_SHA ? process.env.GITHUB_SHA.slice(0, 12) : "（本地）";
  const runner = process.env.ImageOS
    ? `${process.env.ImageOS} ${process.env.ImageVersion ?? ""}`
    : process.env.RUNNER_OS ?? "（本地）";
  lines.push(`> 生成时间（UTC）：${stamp} ／ 提交：${sha} ／ 运行环境：${runner}`);
  lines.push("");
  lines.push("| 套件 | 结果 | 用例统计 | 备注 |");
  lines.push("|---|---|---|---|");

  if (cargo) {
    lines.push(
      `| Rust（cargo test） | ${badge(cargo.failed)} | ${cargo.passed} 通过 / ${cargo.failed} 失败 / ${cargo.ignored} 忽略 | ${cargo.suites} 个测试目标 |`,
    );
  } else {
    lines.push("| Rust（cargo test） | ➖ | （未提供日志） | — |");
  }

  if (vitest) {
    lines.push(
      `| 前端单元（vitest） | ${badge(vitest.failed)} | ${vitest.passed} 通过 / ${vitest.failed} 失败 / 共 ${vitest.total} | ${vitest.files} 个测试文件 |`,
    );
  } else {
    lines.push("| 前端单元（vitest） | ➖ | （未提供报告） | — |");
  }

  if (check) {
    const checkFailed = check.errors > 0 ? 1 : 0;
    lines.push(
      `| 静态检查（svelte-check） | ${badge(checkFailed)} | — | ${check.errors} 错误 / ${check.warnings} 警告 |`,
    );
  } else {
    lines.push("| 静态检查（svelte-check） | ➖ | （未提供日志） | — |");
  }

  lines.push("");
  lines.push(
    "说明：完整测试报告随 Release 附件提供；测试项随开发持续补充，本摘要自动随套件增长。",
  );

  const markdown = lines.join("\n") + "\n";
  if (args.out) {
    writeFileSync(args.out, markdown, "utf8");
    console.log(`测试摘要已写入：${args.out}`);
  }
  console.log(markdown);
}

main();
