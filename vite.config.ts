// Vite 构建配置（vite.config.ts）
// 作用：Tauri 前端开发/构建；端口与 HMR 与 Tauri CLI 约定对齐
// 注意：本文件中的 process.env 读取由 @types/node 提供类型
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import UnoCSS from 'unocss/vite';
import extractorSvelte from '@unocss/extractor-svelte';

// TAURI_DEV_HOST：仅移动端真机调试时由 Tauri CLI 注入；桌面端为空
const devHost = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [
    // UnoCSS 必须放在 svelte() 之前：先完成类名提取再交给 Svelte 编译
    // extractorSvelte：正确识别 .svelte 模板中的类名（含条件表达式）
    UnoCSS({ extractors: [extractorSvelte()] }),
    svelte(),
  ],
  // 保持终端输出（Tauri CLI 需要看到 Vite 日志，清屏会干扰）
  clearScreen: false,
  server: {
    // 固定端口：tauri.conf.json 的 devUrl 依赖它；端口被占直接失败而非换端口
    port: 1420,
    strictPort: true,
    // 默认仅本机；TAURI_DEV_HOST 有值时用于局域网真机调试
    host: devHost || false,
    hmr: devHost ? { protocol: 'ws', host: devHost, port: 1421 } : undefined,
    watch: {
      // 忽略 Rust 工程：避免 cargo 产物触发前端热更
      ignored: ['**/src-tauri/**'],
    },
  },
  // 暴露给前端的注入变量前缀（TAURI_ENV_* 由 Tauri CLI 注入平台/调试信息）
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    // 浏览器编译目标：Windows WebView2 基线（Chromium 105）；其他平台保守目标
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    // 仅调试构建禁用压缩；正式构建使用 Vite 8 默认压缩器（Oxc，无需额外依赖）
    minify: !process.env.TAURI_ENV_DEBUG,
    // 仅调试构建产出 sourcemap（正式包更小）
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
  },
});
