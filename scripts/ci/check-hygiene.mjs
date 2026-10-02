#!/usr/bin/env node
// 仓库卫生检查（CI 与本地共用）。
//
// 用法：
//   node scripts/ci/check-hygiene.mjs <变更文件清单.txt>
//   git diff --name-only A..B | node scripts/ci/check-hygiene.mjs -   （- 表示从标准输入读取）
//
// 检查规则（任一命中即失败，退出码 1）：
//   1. AGENTS.md 及其变体：任意目录下文件名以 `agents` 开头且以 `.md` 结尾（大小写不敏感）；
//   2. 便携数据目录 `data/`（历史/会话/设置属于用户数据，不入仓库）；
//   3. 构建产物与依赖：`src-tauri/target/`、`node_modules/`、`dist/`；
//   4. 单文件 ≥ 1 MiB（文件仍在工作区时检查；已删除的文件跳过）。
//
// 输出：GitHub Actions 环境下输出 `::error::` 注解（会显示在 PR 检查页）；
//       本地运行时输出可读文本。违规列表为空时输出通过信息。

import { readFileSync, statSync } from "node:fs";
import process from "node:process";

/** 单文件大小上限（1 MiB，含）。 */
const SIZE_LIMIT = 1024 * 1024;

/** 命名/路径类规则（逐条作用于每个变更文件）。 */
const RULES = [
  {
    reason: "AGENTS.md 及其变体禁止进入版本库（项目最高优先级文件，仅本地保留）",
    matches: (path) => {
      const base = path.split("/").pop().toLowerCase();
      return base.startsWith("agents") && base.endsWith(".md");
    },
  },
  {
    reason: "便携数据目录 data/ 禁止进入版本库（用户历史/会话/设置数据）",
    matches: (path) => path.startsWith("data/"),
  },
  {
    reason: "Rust 构建产物禁止进入版本库",
    matches: (path) => path === "src-tauri/target" || path.startsWith("src-tauri/target/"),
  },
  {
    reason: "前端依赖与构建产物禁止进入版本库",
    matches: (path) => {
      const hit = (p, root) => p === root || p.startsWith(`${root}/`);
      return hit(path, "node_modules") || hit(path, "dist");
    },
  },
];

/** 读取变更文件清单（参数为文件路径；`-` 表示标准输入）。 */
function readChangedFiles(arg) {
  const raw = arg === "-" ? readFileSync(0, "utf8") : readFileSync(arg, "utf8");
  return raw
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

/** 按规则检查单个文件，返回违规原因列表（空 = 通过）。 */
function violationsOf(path) {
  const found = [];
  for (const rule of RULES) {
    if (rule.matches(path)) {
      found.push(rule.reason);
    }
  }
  try {
    const stat = statSync(path);
    if (stat.isFile() && stat.size >= SIZE_LIMIT) {
      found.push(`单文件超过 1 MiB（${stat.size} 字节），请改用外置存储/发布附件承载`);
    }
  } catch {
    // 文件不存在（新增/删除路径或已清理）→ 跳过大小检查
  }
  return found;
}

/** 主流程。 */
function main() {
  const arg = process.argv[2];
  if (!arg) {
    console.error("用法：node scripts/ci/check-hygiene.mjs <变更文件清单.txt | ->");
    process.exit(2);
  }
  const files = readChangedFiles(arg);
  const inActions = process.env.GITHUB_ACTIONS === "true";
  let failed = false;

  for (const file of files) {
    for (const reason of violationsOf(file)) {
      failed = true;
      if (inActions) {
        console.log(`::error file=${file}::${reason}`);
      } else {
        console.error(`[违规] ${file}：${reason}`);
      }
    }
  }

  if (failed) {
    console.error(`仓库卫生检查未通过：${files.length} 个变更文件中存在违规项。`);
    process.exit(1);
  }
  console.log(`仓库卫生检查通过：${files.length} 个变更文件均符合规则。`);
}

main();
