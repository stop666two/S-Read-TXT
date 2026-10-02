#!/usr/bin/env node
// 构建环境信息收集（供 Release 描述的可复现信息块）。
//
// 用法：node scripts/ci/env-info.mjs --out env-info.md
//
// 说明：不执行任何网络请求；工具版本通过子进程查询（缺失时显示「不可用」），
// Tauri CLI 版本直接读 package.json（避免 npx 在 CI 中的跨平台启动差异）。

import { execFileSync } from "node:child_process";
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

/** 查询命令版本（失败返回「不可用」；Windows 下 npm 为 .cmd 包装，
 *  Node 出于安全考虑不允许直接 spawn .cmd/.bat，需回退经 shell 启动）。 */
function versionOf(command, args) {
  try {
    return execFileSync(command, args, { encoding: "utf8" }).trim();
  } catch {
    // 继续尝试 shell 回退
  }
  if (process.platform === "win32") {
    try {
      // 合并为单条命令字符串后经 shell 启动（避免 Node 对 .cmd 参数的弃用告警；
      // 参数均为脚本内静态常量，不涉及外部输入）
      return execFileSync([command, ...args].join(" "), { encoding: "utf8", shell: true }).trim();
    } catch {
      // 无此命令
    }
  }
  return "（不可用）";
}

/** 读取 package.json 中的依赖版本。 */
function depVersion(name) {
  try {
    const pkg = JSON.parse(readFileSync("package.json", "utf8"));
    return (
      pkg.devDependencies?.[name] ?? pkg.dependencies?.[name] ?? "（未声明）"
    );
  } catch {
    return "（读取失败）";
  }
}

/** 主流程。 */
function main() {
  const args = parseArgs(process.argv.slice(2));
  const lines = [];
  lines.push("# 构建环境");
  lines.push("");
  lines.push("| 项目 | 值 |");
  lines.push("|---|---|");
  lines.push(`| Runner 镜像 | ${process.env.ImageOS ? `${process.env.ImageOS} ${process.env.ImageVersion ?? ""}`.trim() : "（本地）"} |`);
  lines.push(`| 系统 / 架构 | ${process.env.RUNNER_OS ?? "（本地）"} / ${process.env.RUNNER_ARCH ?? "（本地）"} |`);
  lines.push(`| rustc | ${versionOf("rustc", ["--version"])} |`);
  lines.push(`| cargo | ${versionOf("cargo", ["--version"])} |`);
  lines.push(`| node | ${versionOf("node", ["--version"])} |`);
  lines.push(`| npm | ${versionOf("npm", ["--version"])} |`);
  lines.push(`| Tauri CLI（package.json） | ${depVersion("@tauri-apps/cli")} |`);
  lines.push("| 目标架构 | x86_64-pc-windows-msvc / i686-pc-windows-msvc / aarch64-pc-windows-msvc |");
  lines.push("| WebView2 分发 | skip（依赖系统已安装的 Evergreen 运行时，安装包离线可用） |");
  lines.push("");

  const markdown = lines.join("\n") + "\n";
  if (args.out) {
    writeFileSync(args.out, markdown, "utf8");
    console.log(`构建环境信息已写入：${args.out}`);
  }
  console.log(markdown);
}

main();
