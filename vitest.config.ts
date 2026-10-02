// Vitest 配置（vitest.config.ts）
// 作用：前端单元测试（纯逻辑：虚拟滚动数学、快捷键引擎、位置映射等）
// 说明：组件级测试若未来需要，可在此追加 jsdom 环境与 svelte 插件
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    // 默认 node 环境：当前测试对象均为纯逻辑模块，不依赖 DOM
    environment: 'node',
    // 测试文件匹配：与被测模块同目录放置，*.test.ts
    include: ['src/**/*.test.ts'],
    // 不允许“零测试通过”：确保测试确实被执行
    passWithNoTests: false,
  },
});
