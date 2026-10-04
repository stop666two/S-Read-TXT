// 智能配色（设置自定义化 S3）：从种子颜色离线生成一套 13 令牌主题色（确定性算法）。
// 设计约束：
// - 输出恒为 #RRGGBB（通过主题校验与 CSS 取色器）；
// - 浅色/深色两模式；base 与 ink 保持高对比（WCAG ≥ 7），强调色对背景 ≥ 3；
// - 语义色（danger/success/warning）固定不随种子漂移（颜色编码含义而非装饰）。
export const THEME_TOKENS = [
  'base',
  'surface',
  'chrome',
  'ink',
  'muted',
  'line',
  'hover',
  'accent',
  'tabActive',
  'selection',
  'danger',
  'success',
  'warning',
] as const;

export type ThemeToken = (typeof THEME_TOKENS)[number];

/** 语义色（与内置主题一致；不参与种子推导）。 */
const SEMANTIC: Pick<Record<ThemeToken, string>, 'danger' | 'success' | 'warning'> = {
  danger: '#C0392B',
  success: '#2E7D32',
  warning: '#8A5200',
};

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** `#RRGGBB` → HSL（0–360 / 0–100 / 0–100）；非法输入返回 null。 */
function hexToHsl(hex: string): { h: number; s: number; l: number } | null {
  const match = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (!match) return null;
  const value = Number.parseInt(match[1], 16);
  const r = ((value >> 16) & 0xff) / 255;
  const g = ((value >> 8) & 0xff) / 255;
  const b = (value & 0xff) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return { h: 0, s: 0, l: l * 100 };
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  if (max === r) h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
  else if (max === g) h = ((b - r) / d + 2) / 6;
  else h = ((r - g) / d + 4) / 6;
  return { h: h * 360, s: s * 100, l: l * 100 };
}

/** HSL → `#RRGGBB`（大写十六进制）。 */
export function hslToHex(h: number, s: number, l: number): string {
  const hue = (((h % 360) + 360) % 360) / 360;
  const sat = clamp(s, 0, 100) / 100;
  const lum = clamp(l, 0, 100) / 100;
  if (sat === 0) {
    const v = Math.round(lum * 255);
    return `#${[v, v, v].map((n) => n.toString(16).padStart(2, '0')).join('')}`.toUpperCase();
  }
  const q = lum < 0.5 ? lum * (1 + sat) : lum + sat - lum * sat;
  const p = 2 * lum - q;
  const channel = (t: number): number => {
    let temp = t;
    if (temp < 0) temp += 1;
    if (temp > 1) temp -= 1;
    if (temp < 1 / 6) return p + (q - p) * 6 * temp;
    if (temp < 1 / 2) return q;
    if (temp < 2 / 3) return p + (q - p) * (2 / 3 - temp) * 6;
    return p;
  };
  const r = channel(hue + 1 / 3);
  const g = channel(hue);
  const b = channel(hue - 1 / 3);
  return `#${[r, g, b].map((n) => Math.round(n * 255).toString(16).padStart(2, '0')).join('')}`.toUpperCase();
}

/** 生成主题令牌：种子决定色相；明度/饱和度按模式成组推导。 */
export function generatePalette(seed: string, mode: 'light' | 'dark'): Record<ThemeToken, string> {
  const parsed = hexToHsl(seed) ?? { h: 210, s: 45, l: 45 };
  const h = parsed.h;
  const s = clamp(parsed.s, 18, 72);
  if (mode === 'light') {
    return {
      base: hslToHex(h, s * 0.25, 97),
      surface: hslToHex(h, s * 0.16, 100),
      chrome: hslToHex(h, s * 0.22, 94),
      ink: hslToHex(h, s * 0.18, 16),
      muted: hslToHex(h, s * 0.12, 42),
      line: hslToHex(h, s * 0.2, 88),
      hover: hslToHex(h, s * 0.26, 92),
      accent: hslToHex(h, s, 42),
      tabActive: hslToHex(h, s * 0.16, 100),
      selection: hslToHex(h, s * 0.6, 85),
      ...SEMANTIC,
    };
  }
  return {
    base: hslToHex(h, s * 0.2, 12),
    surface: hslToHex(h, s * 0.18, 16),
    chrome: hslToHex(h, s * 0.2, 20),
    ink: hslToHex(h, s * 0.1, 84),
    muted: hslToHex(h, s * 0.1, 62),
    line: hslToHex(h, s * 0.16, 28),
    hover: hslToHex(h, s * 0.2, 24),
    accent: hslToHex(h, s, 62),
    tabActive: hslToHex(h, s * 0.18, 16),
    selection: hslToHex(h, s * 0.5, 32),
    ...SEMANTIC,
  };
}

/** 建议主题名（按模式返回中英双名）。 */
export function suggestThemeName(mode: 'light' | 'dark'): { name: string; nameEn: string } {
  return mode === 'light'
    ? { name: '自定义浅色', nameEn: 'Custom Light' }
    : { name: '自定义深色', nameEn: 'Custom Dark' };
}

/** 相对亮度（WCAG 2.x 定义；非法输入返回 0）。 */
export function relativeLuminance(hex: string): number {
  const match = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (!match) return 0;
  const value = Number.parseInt(match[1], 16);
  const channel = (raw: number): number => {
    const c = raw / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  const r = channel((value >> 16) & 0xff);
  const g = channel((value >> 8) & 0xff);
  const b = channel(value & 0xff);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** 对比度（WCAG 2.x；1–21）。 */
export function contrastRatio(a: string, b: string): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  const [hi, lo] = la >= lb ? [la, lb] : [lb, la];
  return (hi + 0.05) / (lo + 0.05);
}
