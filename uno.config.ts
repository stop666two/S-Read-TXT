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
    // 语义颜色令牌：类名 → CSS 变量（变量定义见 src/styles/base.css）
    // 用法示例：bg-base / text-ink / border-line / bg-accent / text-muted
    colors: {
      // 阅读区/页面主背景（最深的一层）
      base: 'var(--base)',
      // 面板/下拉/弹窗背景
      surface: 'var(--surface)',
      // 菜单栏/工具栏/标签栏/状态栏背景
      chrome: 'var(--chrome)',
      // 主文字颜色（最高对比度）
      ink: 'var(--ink)',
      // 次要文字（时间、辅助说明）
      muted: 'var(--muted)',
      // 边框与分隔线
      line: 'var(--line)',
      // 悬停底色
      hover: 'var(--hover)',
      // 强调色（选中、焦点、链接；蓝系，语义“信息/交互”）
      accent: 'var(--accent)',
    },
  },
});
