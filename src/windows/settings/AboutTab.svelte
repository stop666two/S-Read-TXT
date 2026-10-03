<!--
  AboutTab — 关于：仅显示应用图标、应用名、版本号与仓库地址（按需求不提供检测更新按钮）。
  视觉与「方向一」一致：居中卡片式排布、克制留白。
-->
<script lang="ts">
  import { onMount } from 'svelte';

  import appIcon from '../../assets/app-icon.png';
  import { t } from '../../lib/i18n/index.svelte';
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
  <img class="icon" src={appIcon} alt="" draggable="false" />
  <h1>S-Read-TXT</h1>
  <p class="version">{t('about.version')} {version || '—'}</p>
  <p class="repo">{t('about.repo')}: {t('about.repoPlaceholder')}</p>
</div>

<style>
  .about {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
    color: var(--ink);
  }

  .icon {
    width: 56px;
    height: 56px;
    margin-bottom: 4px;
    opacity: 0.95;
  }

  h1 {
    margin: 0;
    font-size: 21px;
    font-weight: 600;
    letter-spacing: 0.2px;
  }

  .version {
    margin: 0;
    padding: 3px 12px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: var(--surface);
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }

  .repo {
    margin: 14px 0 0;
    color: var(--muted);
    font-size: 12.5px;
  }
</style>
