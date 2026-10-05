// 拖影窗口入口（src/windows/drag-ghost/main.ts）
// 职责：挂载拖影内容；窗口本身由 Rust 创建（visible=false、点击穿透），内容就绪即有真实样式。

import { mount } from 'svelte';

import DragGhost from './DragGhost.svelte';

const target = document.getElementById('app');
if (!target) {
  throw new Error('拖影页面缺少 #app 挂载点');
}

export default mount(DragGhost, { target });
