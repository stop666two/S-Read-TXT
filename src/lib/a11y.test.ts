import { describe, expect, it } from 'vitest';
import { computeA11yState, decideReduceMotion } from './a11y';
import type { AppSettings } from './ipc';

/** 构造仅包含本测试所需字段的设置对象（其余以断言场景为准）。 */
function makeApp(
  overrides: {
    reduceMotion?: 'system' | 'on' | 'off';
    fontScale?: number;
    focusVisible?: boolean;
    screenReader?: boolean;
    highContrastOverlay?: boolean;
    performanceMode?: boolean;
  } = {},
): AppSettings {
  return {
    a11y: {
      reduceMotion: overrides.reduceMotion ?? 'system',
      fontScale: overrides.fontScale ?? 100,
      focusVisible: overrides.focusVisible ?? true,
      screenReader: overrides.screenReader ?? true,
      highContrastOverlay: overrides.highContrastOverlay ?? false,
    },
    system: { performanceMode: overrides.performanceMode ?? false },
  } as unknown as AppSettings;
}

describe('decideReduceMotion', () => {
  it('强制开/关优先于系统偏好', () => {
    expect(decideReduceMotion('on', false)).toBe(true);
    expect(decideReduceMotion('off', true)).toBe(false);
  });

  it('跟随系统时使用系统偏好', () => {
    expect(decideReduceMotion('system', true)).toBe(true);
    expect(decideReduceMotion('system', false)).toBe(false);
  });
});

describe('computeA11yState', () => {
  it('跟随系统且系统偏好减少动画时挂 reduce-motion', () => {
    const state = computeA11yState(makeApp(), true);
    expect(state.reduceMotion).toBe(true);
    expect(state.classes).toContain('reduce-motion');
  });

  it('强制关且系统偏好减少时也不挂 reduce-motion', () => {
    const state = computeA11yState(makeApp({ reduceMotion: 'off' }), true);
    expect(state.reduceMotion).toBe(false);
    expect(state.classes).not.toContain('reduce-motion');
  });

  it('性能模式额外挂类并强制减少动画', () => {
    const state = computeA11yState(makeApp({ performanceMode: true }), false);
    expect(state.reduceMotion).toBe(true);
    expect(state.classes).toContain('reduce-motion');
    expect(state.classes).toContain('performance-mode');
  });

  it('高对比/焦点/屏幕阅读器类按设置挂载', () => {
    const on = computeA11yState(
      makeApp({ highContrastOverlay: true, focusVisible: true, screenReader: true }),
      false,
    );
    expect(on.classes).toContain('hc-overlay');
    expect(on.classes).toContain('focus-strong');
    expect(on.classes).toContain('sr-enhanced');
    expect(on.screenReader).toBe(true);
    const off = computeA11yState(
      makeApp({ focusVisible: false, screenReader: false }),
      false,
    );
    expect(off.classes).not.toContain('focus-strong');
    expect(off.classes).not.toContain('sr-enhanced');
    expect(off.screenReader).toBe(false);
  });

  it('字体缩放 100% 不设置 zoom，其余换算为倍数', () => {
    expect(computeA11yState(makeApp({ fontScale: 100 }), false).zoom).toBeNull();
    expect(computeA11yState(makeApp({ fontScale: 125 }), false).zoom).toBe(1.25);
    expect(computeA11yState(makeApp({ fontScale: 80 }), false).zoom).toBe(0.8);
  });
});
