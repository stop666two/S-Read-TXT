import { describe, expect, it } from 'vitest';

import {
  THEME_TOKENS,
  contrastRatio,
  generatePalette,
  hslToHex,
  suggestThemeName,
} from './palette';

const HEX = /^#[0-9A-F]{6}$/;

describe('智能配色（palette）', () => {
  it('生成全部 13 个令牌且均为合法 #RRGGBB', () => {
    for (const mode of ['light', 'dark'] as const) {
      const palette = generatePalette('#3B6EA5', mode);
      expect(Object.keys(palette).sort()).toEqual([...THEME_TOKENS].sort());
      for (const token of THEME_TOKENS) expect(palette[token]).toMatch(HEX);
    }
  });

  it('确定性：同种子同模式输出一致', () => {
    expect(generatePalette('#3B6EA5', 'light')).toEqual(generatePalette('#3B6EA5', 'light'));
  });

  it('正文可读性：ink 与 base 对比度 ≥ 7（两模式）', () => {
    for (const mode of ['light', 'dark'] as const) {
      const palette = generatePalette('#3B6EA5', mode);
      expect(contrastRatio(palette.ink, palette.base)).toBeGreaterThanOrEqual(7);
    }
  });

  it('强调色与背景对比度 ≥ 3（两模式）', () => {
    for (const mode of ['light', 'dark'] as const) {
      const palette = generatePalette('#8A5200', mode);
      expect(contrastRatio(palette.accent, palette.base)).toBeGreaterThanOrEqual(3);
    }
  });

  it('语义色固定（危险/成功/警告不随种子漂移）', () => {
    const a = generatePalette('#112233', 'light');
    const b = generatePalette('#FFAA00', 'dark');
    expect(a.danger).toBe(b.danger);
    expect(a.success).toBe(b.success);
    expect(a.warning).toBe(b.warning);
  });

  it('非法种子回退中性色相（不抛错）', () => {
    const palette = generatePalette('not-a-color', 'light');
    expect(palette.base).toMatch(HEX);
  });

  it('hslToHex 边界（黑/白）', () => {
    expect(hslToHex(0, 0, 0)).toBe('#000000');
    expect(hslToHex(0, 0, 100)).toBe('#FFFFFF');
  });

  it('建议名称按模式返回中英双名', () => {
    expect(suggestThemeName('light').nameEn).toBe('Custom Light');
    expect(suggestThemeName('dark').name).toBe('自定义深色');
  });
});
