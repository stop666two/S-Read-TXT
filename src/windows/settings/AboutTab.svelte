<!--
  AboutTab — 关于：仅显示应用名、版本号与仓库地址（按需求不提供检测更新按钮）。
-->
<script lang="ts">
  import { onMount } from 'svelte';

  import { ipc } from '../../lib/ipc';

  /** 版本号（`get_app_info` 返回；载入失败保持空） */
  let version = $state('');

  onMount(() => {
    void ipc.getAppInfo().then(
      (info) => {
        version = info.version;
      },
      () => {
        // 版本读取失败不阻塞：保持空显示
      },
    );
  });
</script>

<div class="about">
  <h1>S-Read-TXT</h1>
  <p class="version">版本 {version || '—'}</p>
  <p class="repo">仓库地址：待补充（占位）</p>
</div>

<style>
  .about {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 6px;
    color: var(--ink);
  }

  h1 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
  }

  .version {
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }

  .repo {
    margin: 12px 0 0;
    color: var(--muted);
    font-size: 12.5px;
  }
</style>
