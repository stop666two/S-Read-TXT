// 前端共享类型（跨组件复用，避免字面量类型漂移）。

/** 主题选择：三套显式主题 + 跟随系统。 */
export type ThemeChoice = 'light' | 'dark' | 'eye' | 'system';

/** 解析后的实际主题（跟随系统解析后只可能是这三者之一）。 */
export type ResolvedTheme = 'light' | 'dark' | 'eye';
