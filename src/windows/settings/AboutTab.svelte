<!--
  AboutTab — 关于：应用图标、名称、版本号，以及更新检查（可配置更新源、下载与校验）。
  无网络模式开启时，检查前先弹出确认，明确「临时联网」意图。
-->
<script lang="ts">
  import { onMount } from 'svelte';

  import appIcon from '../../assets/app-icon.png';
  import { t } from '../../lib/i18n/index.svelte';
  import { ipc, type UpdateInfo } from '../../lib/ipc';
  import { describeIpcError, type IpcErrorPayload } from '../../lib/ipc-error';
  import ConfirmDialog from '../../lib/components/ConfirmDialog.svelte';
  import { settings } from './store.svelte';

  type UpdateStatus = 'idle' | 'checking' | 'latest' | 'available' | 'downloading' | 'downloaded' | 'error';

  /** 版本号（`get_app_info` 返回；载入失败保持空） */
  let version = $state('');

  /** 更新流程状态 */
  let status = $state<UpdateStatus>('idle');
  /** 检查结果（available/downloaded 时展示） */
  let info = $state<UpdateInfo | null>(null);
  /** 错误提示文案（error 时展示） */
  let errorText = $state('');
  /** 已下载安装包的路径 */
  let downloadedPath = $state('');
  /** 下载后的文件名（状态行展示） */
  let downloadedName = $state('');
  /** 无网络模式确认弹窗 */
  let offlineConfirm = $state(false);

  const sourceUrl = $derived(settings.snapshot?.app.update.sourceUrl ?? '');
  const offlineMode = $derived(settings.snapshot?.app.system.offlineMode ?? true);
  const busy = $derived(status === 'checking' || status === 'downloading');

  onMount(() => {
    void ipc.getAppInfo().then(
      (app) => {
        version = app.version;
      },
      () => {
        // 版本读取失败不阻塞：保持空显示
      },
    );
  });

  /** 点击「检查更新」：无网络模式先确认，再执行检查。 */
  function onCheckClick(): void {
    if (offlineMode) {
      offlineConfirm = true;
      return;
    }
    void runCheck();
  }

  /** 执行更新检查（清空上次结果）。 */
  async function runCheck(): Promise<void> {
    status = 'checking';
    info = null;
    downloadedPath = '';
    errorText = '';
    try {
      const result = await ipc.checkUpdate(sourceUrl, version || '0.0.0');
      info = result;
      status = result.newer ? 'available' : 'latest';
    } catch (error) {
      errorText = describeIpcError(error as IpcErrorPayload);
      status = 'error';
    }
  }

  /** 下载安装包（若发布提供 sha256 则强校验）。 */
  async function onDownloadClick(): Promise<void> {
    const asset = info?.asset;
    if (!asset) return;
    status = 'downloading';
    errorText = '';
    try {
      const result = await ipc.downloadUpdate(asset.url, asset.sha256, asset.name);
      downloadedPath = result.path;
      downloadedName = asset.name;
      status = 'downloaded';
    } catch (error) {
      errorText = describeIpcError(error as IpcErrorPayload);
      status = 'error';
    }
  }

  /** 在文件管理器中定位已下载的安装包。 */
  async function onRevealClick(): Promise<void> {
    try {
      await ipc.revealUpdateFile(downloadedPath);
    } catch (error) {
      errorText = describeIpcError(error as IpcErrorPayload);
      status = 'error';
    }
  }

  /** 用系统浏览器打开发布页（无发布页时回退到更新源地址）。 */
  async function onOpenPageClick(): Promise<void> {
    const target = info?.releaseUrl || sourceUrl;
    try {
      await ipc.openUpdatePage(target);
    } catch (error) {
      errorText = describeIpcError(error as IpcErrorPayload);
      status = 'error';
    }
  }
</script>

<div class="about">
  <img class="icon" src={appIcon} alt="" draggable="false" />
  <h1>S-Read-TXT</h1>
  <p class="version">{t('about.version')} {version || '—'}</p>
  <p class="repo">{t('about.repo')}: {t('about.repoPlaceholder')}</p>

  <div class="update">
    <button class="btn" type="button" data-update-check onclick={onCheckClick} disabled={busy}>
      {t('about.update.check')}
    </button>

    <p class="status" data-update-status={status}>
      {#if status === 'checking'}
        {t('about.update.checking')}
      {:else if status === 'latest' && info}
        {t('about.update.latest', { version: info.latest })}
      {:else if status === 'available' && info}
        {t('about.update.available', { latest: info.latest, current: info.current })}
      {:else if status === 'downloading'}
        {t('about.update.downloading')}
      {:else if status === 'downloaded'}
        {t('about.update.downloaded', { name: downloadedName })}
      {:else if status === 'error'}
        <span class="error" data-update-error>{errorText}</span>
      {/if}
    </p>

    <div class="actions">
      {#if status === 'available' && info?.asset}
        <button class="btn primary" type="button" data-update-download onclick={onDownloadClick}>
          {t('about.update.download')}
        </button>
      {/if}
      {#if status === 'downloaded'}
        <button class="btn primary" type="button" data-update-reveal onclick={onRevealClick}>
          {t('about.update.reveal')}
        </button>
      {/if}
      {#if (status === 'available' && !info?.asset) || status === 'latest'}
        <span class="hint">{status === 'available' ? t('about.update.noAsset') : ''}</span>
      {/if}
      {#if info?.releaseUrl || (status === 'error' && sourceUrl)}
        <button class="btn" type="button" data-update-page onclick={onOpenPageClick}>
          {t('about.update.openPage')}
        </button>
      {/if}
    </div>
  </div>
</div>

<ConfirmDialog
  open={offlineConfirm}
  title={t('about.update.offlineTitle')}
  message={t('about.update.offlineMessage')}
  confirmLabel={t('about.update.offlineConfirm')}
  onCancel={() => {
    offlineConfirm = false;
  }}
  onConfirm={() => {
    offlineConfirm = false;
    void runCheck();
  }}
/>

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

  .update {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    margin-top: 18px;
    min-height: 74px;
  }

  .status {
    margin: 0;
    min-height: 18px;
    color: var(--muted);
    font-size: 12.5px;
    text-align: center;
  }

  .status .error {
    color: var(--danger, #c94f4f);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 28px;
  }

  .hint {
    color: var(--muted);
    font-size: 12px;
  }

  .btn {
    appearance: none;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--surface);
    color: var(--ink);
    padding: 5px 14px;
    font-size: 12.5px;
    cursor: pointer;
  }

  .btn:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }

  .btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .btn.primary {
    border-color: var(--accent);
    color: var(--accent);
  }

  .btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
</style>
