// 编码自检脚本（scripts/check-encoding.mjs）
// 用途：检查项目内文本文件是否满足编码规范——UTF-8（RFC 3629）无 BOM、LF 换行
// 用法：node scripts/check-encoding.mjs
// 退出码：0 = 全部通过；1 = 存在违规（违规明细输出到 stdout）
// 说明：跳过依赖/构建产物/临时目录与二进制文件；大文件（>1MB）不检查（不应入库）
import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/** 项目根目录（本脚本位于 scripts/ 下） */
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

/** 跳过扫描的目录名（依赖、版本库、构建产物、运行时数据、临时工作区） */
const SKIP_DIRS = new Set(['node_modules', '.git', 'target', 'gen', 'dist', 'data', 'tmp']);

/** 需要检查的文本扩展名（含无扩展名的钩子文件特例） */
const TEXT_EXTS = new Set([
  '.ts', '.js', '.mjs', '.cjs', '.json', '.svelte', '.css', '.html',
  '.md', '.toml', '.txt', '.yml', '.yaml', '.rs', '.sh',
]);

/** 单文件大小上限：超过则不检查（同时提示，因为不应入库） */
const MAX_BYTES = 1024 * 1024;

/** UTF-8 严格解码器（fatal：遇到非法字节序列抛出异常） */
const utf8 = new TextDecoder('utf-8', { fatal: true });

/** 违规记录：{ file, reason } */
const violations = [];
/** 检查计数 */
let checked = 0;

/**
 * 递归扫描目录并检查文本文件编码。
 * @param {string} dir 目录绝对路径
 */
function walk(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) continue;
      walk(full);
      continue;
    }
    if (!entry.isFile()) continue;

    const ext = path.extname(entry.name).toLowerCase();
    const isHook = entry.name === 'pre-commit';
    if (!TEXT_EXTS.has(ext) && !isHook) continue;

    // 先按文件大小过滤（避免为超限文件做整读），再读取校验
    const size = statSync(full).size;
    if (size > MAX_BYTES) continue;
    const buf = readFileSync(full);
    checked += 1;
    const rel = path.relative(root, full);

    // 1) BOM 检查：源码/配置一律无 BOM
    if (buf.length >= 3 && buf[0] === 0xef && buf[1] === 0xbb && buf[2] === 0xbf) {
      violations.push({ file: rel, reason: '存在 UTF-8 BOM（应无 BOM）' });
    }
    // 2) 换行检查：禁止 CR（应统一 LF）
    if (buf.includes(0x0d)) {
      violations.push({ file: rel, reason: '存在 CR 字节（应统一 LF 换行）' });
    }
    // 3) UTF-8 合法性检查
    try {
      utf8.decode(buf);
    } catch {
      violations.push({ file: rel, reason: '不是合法的 UTF-8 字节序列' });
    }
  }
}

walk(root);

// 输出中文报告（Node 直接以 UTF-8 输出，避免 PowerShell 5.1 的 GBK 干扰）
if (violations.length === 0) {
  console.log(`编码自检通过：共检查 ${checked} 个文本文件（UTF-8 无 BOM / LF）。`);
  process.exit(0);
}

console.log(`编码自检失败：共检查 ${checked} 个文件，发现 ${violations.length} 处违规：`);
for (const v of violations) {
  console.log(`  - ${v.file}：${v.reason}`);
}
process.exit(1);
