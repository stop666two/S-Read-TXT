<!--
  App.svelte — 根组件（阶段 0 冒烟版本）
  职责：验证「WebView → IPC → Rust」链路与主题令牌渲染是否正常。
  说明：阶段 2 将整体替换为完整三段式布局（菜单栏/工具栏/标签栏/阅读区/状态栏）。
-->
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  /** 后端 get_app_info 命令的返回体（与 Rust 侧 AppInfo 字段一一对应，camelCase） */
  interface AppInfo {
    /** 应用版本号，如 0.0.1-beta */
    version: string;
    /** 当前数据目录绝对路径（便携模式：程序目录/data） */
    dataDir: string;
  }

  /** 后端信息：异步获取后驱动界面更新（Runes 响应式） */
  let info = $state<AppInfo | null>(null);
  /** IPC 错误信息：失败时显式展示，不吞异常（项目规则：优雅错误处理） */
  let ipcError = $state<string | null>(null);

  // 挂载后调用一次冒烟命令；错误转为可读文案展示
  $effect(() => {
    invoke<AppInfo>('get_app_info')
      .then((value) => {
        info = value;
      })
      .catch((error: unknown) => {
        ipcError = error instanceof Error ? error.message : String(error);
      });
  });
</script>

<main class="h-screen flex items-center justify-center bg-base text-ink">
  <div class="text-center select-none">
    <h1 class="text-2xl font-medium tracking-wide">S-Read-TXT</h1>
    {#if ipcError}
      <p class="mt-3 text-sm text-muted">IPC 链路异常：{ipcError}</p>
    {:else if info}
      <p class="mt-3 text-sm text-muted">v{info.version} · 数据目录：{info.dataDir}</p>
    {:else}
      <p class="mt-3 text-sm text-muted">正在初始化…</p>
    {/if}
  </div>
</main>
