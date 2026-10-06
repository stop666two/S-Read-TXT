// S-Read-TXT 前端入口（src/main.ts）
// 职责：挂载 Svelte 根组件；不做任何业务初始化（业务在 App 及其子模块内）
import { mount } from 'svelte';
// UnoCSS 原子样式：构建期按需生成，只包含实际使用的类
import 'virtual:uno.css';
// 基础样式与主题令牌（CSS 变量：浅色/深色/护眼）
import './styles/base.css';
import App from './App.svelte';
import { ipc } from './lib/ipc';
import { dataDirStore } from './lib/state/data-dir.svelte';
import { tabs } from './lib/state/tabs.svelte';
import { applyThemeTokens } from './lib/theme';

// 挂载点由 index.html 提供；缺失视为入口页被破坏，直接抛错（启动自检，fail fast）
const target = document.getElementById('app');
if (!target) {
  throw new Error('挂载点 #app 缺失：index.html 被意外修改');
}

// 启动占位（index.html 内置）在挂载前清除：避免占位与 Svelte 首帧同屏重叠。
target.replaceChildren();

declare global {
  interface Window {
    /** 自动化测试钩子（scripts/smoke.mjs 经 CDP 驱动真实应用；仅暴露最小动作面） */
    __srt?: {
      /** 打开文件（与拖拽/对话框相同的 store 路径） */
      openPath: (path: string) => Promise<void>;
      /** 数据目录不可写引导：等价于用户在弹窗中选择目录后的应用动作（E2E 用） */
      setDataDir: (dir: string) => Promise<void>;
      /** 切换主题（真实应用路径：保存设置并应用令牌；E2E 截图/断言用；App 挂载后可用） */
      setTheme?: (id: string) => void;
    };
  }
}

// 测试钩子尽早暴露（挂载前）：就绪判定不再受主题 IPC 往返影响
window.__srt = {
  openPath: (path) => tabs.openPath(path),
  setDataDir: async (dir) => {
    await dataDirStore.apply(dir);
  },
};

// FOUC 防护 + 就绪并行：主题预载最多等待 150ms——正常路径先应用主题再挂载（无闪烁）；
// 极端首启 IPC 慢时立即以默认浅色挂载，主题到达后再应用，不阻塞首帧。
const themeReady = ipc.getTheme(null).then(
  (resolved) => {
    applyThemeTokens(resolved);
    return true;
  },
  (error) => {
    console.error('[srt] 主题预载失败（回退默认浅色）：', error);
    return false;
  },
);
await Promise.race([
  themeReady,
  new Promise<void>((resolve) => {
    setTimeout(resolve, 150);
  }),
]);
// Svelte 5 函数式挂载；返回实例供将来可能的销毁/热更场景使用。
// 若走了超时分支，themeReady 完成后仍会应用主题（对已挂载 UI 生效）。
const app = mount(App, { target });
// 页面内就绪时点（首帧渲染后，供启动测量脚本使用真实时钟，排除 CDP 连接开销）
requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    (window as Window & { __srtReadyAt?: number }).__srtReadyAt = performance.now();
  });
});

export default app;
