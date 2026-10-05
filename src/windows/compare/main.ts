// 比较/合并窗口入口（src/windows/compare/main.ts）
// 职责：挂载比较应用、就绪后显示窗口（与设置窗口同一「防空白」策略）。

import { getCurrentWindow } from '@tauri-apps/api/window';
import { mount } from 'svelte';

import 'virtual:uno.css';
import '../../styles/base.css';

import CompareApp from './CompareApp.svelte';

const target = document.getElementById('app');
if (!target) {
  throw new Error('比较页面缺少 #app 挂载点');
}

const app = mount(CompareApp, { target });

// 就绪即显：等待真实首帧（双重 rAF）后再显示，避免用户看到空白窗口；
// 本窗口由 Rust 侧按需创建（visible=false），显示权限已在能力配置中开放。
void (async () => {
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
  const window_ = getCurrentWindow();
  await window_.show();
  await window_.setFocus();
})();

export default app;
