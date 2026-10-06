// 主题应用层：把后端解析出的颜色令牌写成 CSS 变量，并处理切换过渡。
//
// 设计：
// - 令牌键为 camelCase（tabActive）→ CSS 变量 `--tab-active`；
// - 明暗基底写入 `<html data-theme-base>`（驱动 color-scheme 与系统联动控件）；
// - 切换过渡：临时挂 `.theme-anim` 类，按设置时长做颜色过渡；**尊重系统“减少动态效果”**
//   （prefers-reduced-motion: reduce 时不加过渡）；
// - 失败回滚：本模块只负责 UI 应用，令牌合法性由后端强校验；调用方在 IPC 失败时保留现状。

import type { ResolvedTheme } from './ipc';
import { isReduceMotion } from './a11y';

/** 令牌键（camelCase）→ CSS 变量名（kebab-case，`--` 前缀） */
export function tokenToCssVar(token: string): string {
  const kebab = token.replace(/[A-Z]/g, (ch) => `-${ch.toLowerCase()}`);
  return `--${kebab}`;
}

/** 过渡参数（来自阅读设置；动画关闭/时长为 0/系统减少动态效果时不加过渡） */
export interface ThemeTransition {
  enabled: boolean;
  durationMs: number;
}

let animTimer: ReturnType<typeof setTimeout> | null = null;

/** 系统是否要求减少动态效果 */
function prefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

/** 是否应跳过主题过渡：系统偏好或用户设置（含性能模式）要求减少动画。 */
function reduceMotionNow(): boolean {
  return prefersReducedMotion() || isReduceMotion();
}

/**
 * 应用主题令牌到当前文档（主窗口与设置窗口共用）。
 *
 * 参数：
 * - `theme`：后端解析结果（`system` 已解析为具体 light/dark 主题）；
 * - `transition`：过渡配置（缺省不加过渡）。
 * 边界：无 `document`（测试环境）时安全跳过。
 */
export function applyThemeTokens(theme: ResolvedTheme, transition?: ThemeTransition): void {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  const animate =
    transition?.enabled === true &&
    (transition.durationMs ?? 0) > 0 &&
    !reduceMotionNow();
  if (animate) {
    root.style.setProperty('--theme-anim-ms', `${transition?.durationMs ?? 200}ms`);
    root.classList.add('theme-anim');
    if (animTimer !== null) clearTimeout(animTimer);
    animTimer = setTimeout(() => {
      root.classList.remove('theme-anim');
      animTimer = null;
    }, (transition?.durationMs ?? 200) + 60);
  }
  for (const [token, value] of Object.entries(theme.tokens)) {
    root.style.setProperty(tokenToCssVar(token), value);
  }
  root.dataset.themeBase = theme.base;
  root.dataset.themeId = theme.id;
}
