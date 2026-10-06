import type { AppSettings, ReduceMotion } from './ipc';

// 可访问性运行时：把设置解析为根节点类名与缩放，供各窗口统一应用。
// 纯函数（computeA11yState/decideReduceMotion）独立于 DOM，便于单测。

/** 根节点上生效的可访问性状态 */
export interface A11yState {
  /** 是否处于减少动画（设置开启，或跟随系统且系统偏好减少，或性能模式） */
  reduceMotion: boolean;
  /** 需要挂到根节点上的类名（不含 reduceMotion 对应的类） */
  classes: string[];
  /** 字体缩放（1 = 不缩放；null = 不设置） */
  zoom: number | null;
  /** 屏幕阅读器增强是否启用 */
  screenReader: boolean;
}

/** 依据设置与系统偏好决定是否减少动画。 */
export function decideReduceMotion(setting: ReduceMotion, systemPrefers: boolean): boolean {
  if (setting === 'on') return true;
  if (setting === 'off') return false;
  return systemPrefers;
}

/** 解析设置生成根节点状态（纯函数；DOM 应用见 applyA11ySettings）。 */
export function computeA11yState(app: AppSettings, systemPrefers: boolean): A11yState {
  const performance = app.system.performanceMode;
  const reduceMotion = decideReduceMotion(app.a11y.reduceMotion, systemPrefers) || performance;
  const classes: string[] = [];
  if (reduceMotion) classes.push('reduce-motion');
  if (performance) classes.push('performance-mode');
  if (app.a11y.highContrastOverlay) classes.push('hc-overlay');
  if (app.a11y.focusVisible) classes.push('focus-strong');
  if (app.a11y.screenReader) classes.push('sr-enhanced');
  const zoom = app.a11y.fontScale / 100;
  return {
    reduceMotion,
    classes,
    zoom: zoom === 1 ? null : zoom,
    screenReader: app.a11y.screenReader,
  };
}

/** 系统是否偏好减少动画（非浏览器环境返回 false）。 */
export function systemPrefersReducedMotion(): boolean {
  return (
    typeof window !== 'undefined' &&
    typeof window.matchMedia === 'function' &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
  );
}

/** 模块级生效状态：供组件在动画决策处查询（默认跟随系统偏好）。 */
let reduceMotionActive =
  typeof window !== 'undefined' &&
  typeof window.matchMedia === 'function' &&
  window.matchMedia('(prefers-reduced-motion: reduce)').matches;
let screenReaderActive = true;

/** 当前是否应减少动画（包含性能模式）。 */
export function isReduceMotion(): boolean {
  return reduceMotionActive;
}

/** 屏幕阅读器增强是否启用（提示与状态区实时播报）。 */
export function isScreenReaderEnhanced(): boolean {
  return screenReaderActive;
}

const A11Y_CLASSES = ['reduce-motion', 'performance-mode', 'hc-overlay', 'focus-strong', 'sr-enhanced'];

/** 把设置应用到当前窗口根节点（类名 + 字体缩放）。 */
export function applyA11ySettings(app: AppSettings): void {
  const state = computeA11yState(app, systemPrefersReducedMotion());
  reduceMotionActive = state.reduceMotion;
  screenReaderActive = state.screenReader;
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  for (const name of A11Y_CLASSES) {
    root.classList.toggle(name, state.classes.includes(name));
  }
  root.style.zoom = state.zoom === null ? '' : String(state.zoom);
}
