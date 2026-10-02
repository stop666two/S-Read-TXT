// 环境类型声明（仅类型层，不产出运行时代码）
// - svelte：Svelte 5 组件与内置类型的全局声明
// - vite/client：import.meta.env 等 Vite 注入变量的类型
/// <reference types="svelte" />
/// <reference types="vite/client" />

/**
 * UnoCSS 虚拟模块声明。
 * 该模块由 unocss/vite 插件在构建期生成（原子样式汇总），
 * 只做副作用导入（import 'virtual:uno.css'），无导出值。
 */
declare module 'virtual:uno.css';
