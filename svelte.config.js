// Svelte 编译器配置（svelte.config.js）
// 作用：为 .svelte 文件提供预处理器与编译选项；由 @sveltejs/vite-plugin-svelte 读取
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default {
  // 预处理器：让 <script lang="ts"> 与 PostCSS 等能力在 Svelte 文件中可用
  preprocess: vitePreprocess(),
  compilerOptions: {
    // 强制 Runes 模式（Svelte 5 响应式系统）：
    // - 全部状态使用 $state/$derived/$effect/$props，禁止遗留的隐式响应式写法
    // - 项目为全新代码库，统一模式可避免两套语义混用
    runes: true,
  },
};
