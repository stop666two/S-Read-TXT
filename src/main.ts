// S-Read-TXT 前端入口（src/main.ts）
// 职责：挂载 Svelte 根组件；不做任何业务初始化（业务在 App 及其子模块内）
import { mount } from 'svelte';
// UnoCSS 原子样式：构建期按需生成，只包含实际使用的类
import 'virtual:uno.css';
// 基础样式与主题令牌（CSS 变量：浅色/深色/护眼）
import './styles/base.css';
import App from './App.svelte';

// 挂载点由 index.html 提供；缺失视为入口页被破坏，直接抛错（启动自检，fail fast）
const target = document.getElementById('app');
if (!target) {
  throw new Error('挂载点 #app 缺失：index.html 被意外修改');
}

// Svelte 5 函数式挂载；返回实例供将来可能的销毁/热更场景使用
const app = mount(App, { target });

export default app;
