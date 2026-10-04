// 开发工具链探测（供 scripts/dev.mjs 使用；所有路径解析均在本项目内完成）。
// 背景：Windows GNU 构建依赖 MSYS2 的 ucrt64\bin 位于 PATH 前部（原因见 README）。
import { existsSync } from 'node:fs';
import { delimiter, join } from 'node:path';

/** 探测 MSYS2 ucrt64\bin：SRT_MSYS_BIN 优先，否则扫描 PATH 中形如 **\ucrt64\bin 的条目。 */
export function detectMsysBin() {
  const env = process.env.SRT_MSYS_BIN?.trim();
  if (env && existsSync(join(env, 'gcc.exe'))) return env;
  for (const dir of (process.env.PATH ?? '').split(delimiter)) {
    if (!dir) continue;
    if (/[\\/]ucrt64[\\/]bin[\\/]?$/i.test(dir) && existsSync(join(dir, 'gcc.exe'))) return dir;
  }
  return null;
}

/** 构建/测试用环境：PATH 前置 MSYS2 与 src-tauri\target\debug（WebView2Loader.dll 所在目录）。 */
export function buildEnv({ root }) {
  const parts = [detectMsysBin(), join(root, 'src-tauri', 'target', 'debug')].filter(Boolean);
  return { ...process.env, PATH: [...parts, process.env.PATH ?? ''].join(delimiter) };
}
