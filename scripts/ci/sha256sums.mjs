#!/usr/bin/env node
// SHA-256 校验和生成（GNU coreutils `sha256sum` 兼容输出，供 Release 校验下载产物）。
//
// 用法：node scripts/ci/sha256sums.mjs <目录> > SHA256SUMS.txt
// 输出：每行 `<64 位小写哈希>  <相对文件名>`；按文件名排序保证可复现。

import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import process from "node:process";

/** 递归收集目录内的文件（跳过目录本身；跳过校验和输出文件自身）。 */
function collectFiles(root, dir = root, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      collectFiles(root, full, out);
    } else if (entry.isFile()) {
      // 输出文件本身（SHA256SUMS.txt）不参与计算，避免自引用
      if (entry.name.toLowerCase() === "sha256sums.txt") {
        continue;
      }
      out.push(full);
    }
  }
  return out;
}

/** 主流程。 */
function main() {
  const dir = process.argv[2];
  if (!dir) {
    console.error("用法：node scripts/ci/sha256sums.mjs <目录>");
    process.exit(2);
  }
  const stat = statSync(dir);
  const files = stat.isDirectory() ? collectFiles(dir) : [dir];
  files.sort((a, b) => relative(dir, a).localeCompare(relative(dir, b)));
  for (const file of files) {
    const hash = createHash("sha256").update(readFileSync(file)).digest("hex");
    console.log(`${hash}  ${relative(dir, file)}`);
  }
}

main();
