// UnoCSS 配置（uno.config.ts）
// 作用：定义原子 CSS 引擎的预设与主题令牌；与 vite.config.ts 中的 UnoCSS() 插件配合
// 说明：颜色令牌一律映射到 CSS 变量（见 src/styles/base.css），
//       使主题切换（浅色/深色/护眼）无需重新生成原子类
import { defineConfig, presetWind3 } from 'unocss';

export default defineConfig({
  presets: [
    // presetWind3：Tailwind/Windi 兼容的原子类预设（稳定、文档齐全）
    // 不使用 presetUno（已弃用的旧预设）
    presetWind3(),
  ],
  theme: {
    // 语义颜色令牌：类名 → CSS 变量
    // 用法示例：bg-base / text-ink / border-line / bg-accent / text-muted
    colors: {
      // 页面背景（最深的一层）
      base: 'var(--c-bg)',
      // 表面/面板背景（工具栏、标签、弹窗等）
      surface: 'var(--c-surface)',
      // 正文文字颜色（最高对比度）
      ink: 'var(--c-text)',
      // 次要文字（时间、辅助说明）
      muted: 'var(--c-muted)',
      // 边框与分隔线
      line: 'var(--c-border)',
      // 强调色（选中、焦点、链接；蓝系，语义“信息/交互”）
      accent: 'var(--c-accent)',
    },
  },
});
