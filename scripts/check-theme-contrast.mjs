#!/usr/bin/env node
// 主题令牌 WCAG 对比度校验（rule 40 / 设计文档：全部主题满足 AA）。
//
// 用途：校验 `src-tauri/resources/themes/*.json` 中每套主题的关键前景/背景组合，
//       对比度低于阈值即失败（exit 1），并在 verify-all 或 CI 中可复用。
//
// 用法：
//   node scripts/check-theme-contrast.mjs          # 校验全部内置主题
//   node scripts/check-theme-contrast.mjs <dir>    # 校验指定目录
//
// 判定：正文 ink/base 与 ink/surface ≥ 7.0（AAA 正文）；muted/accent/danger 等
//       前景色对其常用底色 ≥ 4.5（AA 正常文本）；行/边框类不参与对比度判定。

import { readdirSync, readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const targetDir = process.argv[2] ? process.argv[2] : join(root, 'src-tauri', 'resources', 'themes');

/** sRGB 分量线性化（WCAG 2.x 定义） */
function srgbToLinear(channel) {
  return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
}

/** #RRGGBB → 相对亮度 */
function relativeLuminance(hex) {
  const raw = hex.replace('#', '');
  if (!/^[0-9a-fA-F]{6}$/.test(raw)) {
    throw new Error(`非法颜色值：${hex}（仅支持 #RRGGBB）`);
  }
  const r = srgbToLinear(parseInt(raw.slice(0, 2), 16) / 255);
  const g = srgbToLinear(parseInt(raw.slice(2, 4), 16) / 255);
  const b = srgbToLinear(parseInt(raw.slice(4, 6), 16) / 255);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** 两色对比度 */
function contrast(fg, bg) {
  const l1 = relativeLuminance(fg);
  const l2 = relativeLuminance(bg);
  const [hi, lo] = l1 >= l2 ? [l1, l2] : [l2, l1];
  return (hi + 0.05) / (lo + 0.05);
}

/** 校验规则：[前景令牌, 背景令牌, 最低对比度, 说明] */
const RULES = [
  ['ink', 'base', 7.0, '正文/阅读背景（AAA）'],
  ['ink', 'surface', 7.0, '正文/面板背景（AAA）'],
  ['muted', 'base', 4.5, '次要文字/阅读背景'],
  ['muted', 'surface', 4.5, '次要文字/面板背景'],
  ['accent', 'base', 4.5, '强调文字/阅读背景'],
  ['accent', 'surface', 4.5, '强调文字/面板背景'],
  ['danger', 'base', 4.5, '错误文字/阅读背景'],
  ['success', 'base', 4.5, '成功文字/阅读背景'],
  ['warning', 'base', 4.5, '警告文字/阅读背景'],
];

function main() {
  const files = readdirSync(targetDir).filter((name) => name.endsWith('.json')).sort();
  if (files.length === 0) {
    console.error(`未找到主题文件：${targetDir}`);
    process.exit(1);
  }

  let failures = 0;
  for (const file of files) {
    const theme = JSON.parse(readFileSync(join(targetDir, file), 'utf8'));
    const tokens = theme.tokens ?? {};
    const results = [];
    for (const [fgKey, bgKey, min, label] of RULES) {
      const fg = tokens[fgKey];
      const bg = tokens[bgKey];
      if (!fg || !bg) {
        console.error(`${theme.id}：缺少令牌 ${fgKey} 或 ${bgKey}`);
        failures += 1;
        continue;
      }
      const ratio = contrast(fg, bg);
      const pass = ratio >= min;
      if (!pass) failures += 1;
      results.push(`${pass ? 'PASS' : 'FAIL'} ${label} ${fgKey}/${bgKey} ${ratio.toFixed(2)} (≥${min})`);
    }
    console.log(`[${theme.id}] ${theme.name} / ${theme.nameEn}`);
    for (const line of results) console.log(`  ${line}`);
  }

  if (failures > 0) {
    console.error(`\n对比度校验失败：${failures} 项不达标。`);
    process.exit(1);
  }
  console.log(`\n对比度校验通过：${files.length} 套主题全部达标（WCAG AA）。`);
}

main();
