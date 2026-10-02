#!/usr/bin/env node
// Release 描述生成器（板块经维护者确认：基础五件套 + 构建环境 + 变更明细 +
// 测试报告附件说明 + 已知问题与限制）。
//
// 用法：
//   node scripts/ci/release-notes.mjs \
//     --tag v0.0.2-beta \
//     --test-summary release-meta/test-summary.md \
//     --env-info release-meta/env-info.md \
//     --changelog CHANGELOG.md \
//     --known-issues docs/known-issues.md \
//     --git-log git-log.md \
//     --sha256 release-assets-all/SHA256SUMS.txt \
//     --out release-notes.md
//
// 可选文件缺失时以「（未提供）」占位，保证发布不因单个文件缺失而失败。

import { readFileSync, writeFileSync } from "node:fs";
import process from "node:process";

/** 解析 `--key value` 参数。 */
function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i].startsWith("--")) {
      args[argv[i].slice(2)] = argv[i + 1];
      i += 1;
    }
  }
  return args;
}

/** 安全读取（缺失返回 null）。 */
function readText(path) {
  if (!path) return null;
  try {
    return readFileSync(path, "utf8").replace(/\r\n/g, "\n").trim();
  } catch {
    return null;
  }
}

/** 提取 CHANGELOG 的「未发布」段；不存在则返回全文。 */
function changelogSection(changelog) {
  if (!changelog) return null;
  const match = changelog.match(/## \[未发布\]([\s\S]*?)(?=\n## \[|$)/);
  return match ? match[1].trim() : changelog;
}

/** 去掉嵌入文档的首行一级标题（避免与 Release 描述自身的标题层级冲突）。 */
function stripH1(text) {
  if (!text) return null;
  return text.replace(/^#\s+[^\n]*\n?/, "").trim() || null;
}

/** 从校验和清单提取文件名列表。 */
function assetNames(sha256) {
  if (!sha256) return [];
  return sha256
    .split("\n")
    .map((line) => line.trim().split(/\s+/).pop())
    .filter((name) => name && name.length > 0);
}

/** 主流程。 */
function main() {
  const args = parseArgs(process.argv.slice(2));
  const tag = args.tag ?? "（未知版本）";
  const isBeta = tag.includes("-");
  const releaseType = isBeta ? "预发布（beta）" : "正式版";
  const date = new Date().toISOString().slice(0, 10);
  const sha256 = readText(args.sha256);

  const lines = [];
  lines.push(`# S-Read-TXT ${tag}`);
  lines.push("");
  lines.push(`> 发布类型：${releaseType} ／ 发布日期：${date}（UTC）`);
  lines.push("");
  lines.push("## 亮点（本次变更）");
  lines.push("");
  lines.push(changelogSection(readText(args.changelog)) ?? "（未提供变更日志）");
  lines.push("");
  lines.push("## 测试摘要");
  lines.push("");
  lines.push(stripH1(readText(args["test-summary"])) ?? "（未提供测试摘要）");
  lines.push("");
  lines.push("完整测试摘要见附件 `test-summary.md`；仓库内存在 `docs/test-report.md` 时随附完整测试报告。");
  lines.push("");
  lines.push("## 构建环境");
  lines.push("");
  lines.push(stripH1(readText(args["env-info"])) ?? "（未提供构建环境信息）");
  lines.push("");
  lines.push("## 产物与校验和");
  lines.push("");
  const names = assetNames(sha256);
  if (names.length > 0) {
    for (const name of names) {
      lines.push(`- \`${name}\``);
    }
  } else {
    lines.push("（未提供产物清单）");
  }
  lines.push("");
  if (sha256) {
    lines.push("SHA256 校验和（下载后可用 `certutil -hashfile <文件> SHA256` 核对）：");
    lines.push("");
    lines.push("```");
    lines.push(sha256);
    lines.push("```");
  }
  lines.push("");
  lines.push("## 变更明细");
  lines.push("");
  lines.push(readText(args["git-log"]) ?? "（未提供变更明细）");
  lines.push("");
  lines.push("## 已知问题与限制");
  lines.push("");
  lines.push(stripH1(readText(args["known-issues"])) ?? "（未提供）");
  lines.push("");
  lines.push("## 安装说明");
  lines.push("");
  lines.push("1. **系统要求：Windows 10 1803 及以上（x64 / x86 / ARM64）**；不支持 Windows 7 / 8.1（微软已随 Edge/WebView2 109 于 2023-01 终止对旧系统的支持，本程序工具链亦要求 Windows 10+）；");
  lines.push("2. **安装范围二选一**（安装向导第二步）：「为本机所有用户安装」装入 `C:\\Program Files\\S-Read-TXT`，需要管理员权限（选择此项时安装器请求 UAC）；「只为我自己安装」装入 `%LOCALAPPDATA%\\S-Read-TXT`，全程无需任何权限；");
  lines.push("3. 按系统架构下载对应安装包：`x64`（64 位，绝大多数设备）/ `x86`（32 位旧设备）/ `arm64`（Windows on ARM）；");
  lines.push("4. 安装包为 NSIS 安装程序，未进行代码签名，SmartScreen 可能提示「未知发布者」——请核对 SHA256 校验和后选择「仍要运行」；");
  lines.push("5. 程序依赖系统 WebView2 运行时（Windows 10/11 通常已预装；缺失时请先从微软官网安装 Evergreen 运行时）；");
  lines.push("6. **启动权限行为（自动按需）**：便携运行与「仅为我」安装不请求任何权限；「所有用户」安装且当前非管理员时，启动会请求管理员权限（UAC）——同意后继续（数据仍在程序目录），取消则进入「数据目录引导」（可选其他可写目录或只读运行）；完全禁用自动提权可设置环境变量 `SRT_NO_ELEVATION=1`（详见 README「安装与权限行为」）；");
  lines.push("7. 完全离线运行，不联网、不含遥测；所有数据保存在程序目录 `data/` 内。");
  lines.push("");
  lines.push("## 备份提醒");
  lines.push("");
  lines.push("本程序为便携模式，阅读历史、会话与设置均保存在程序目录 `data/` 内。**升级或更换版本前，请先备份 `data/` 目录**；");
  lines.push("Releases 只保留最新一个版本（旧 Release 会被新版本替换），旧版本源码仍可通过对应 tag 永久获取并自行构建。");
  lines.push("");

  const markdown = lines.join("\n");
  if (args.out) {
    writeFileSync(args.out, markdown, "utf8");
    console.log(`Release 描述已写入：${args.out}（${markdown.length} 字符）`);
  } else {
    console.log(markdown);
  }
}

main();
