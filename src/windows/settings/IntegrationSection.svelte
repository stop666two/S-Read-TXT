<!--
  IntegrationSection — 系统集成（.txt/.log 打开方式候选、右键菜单）。
  默认不改动系统；「当前用户」范围无需管理员；「所有用户」范围在点击时请求 UAC。
  契约：复选框 data-integration-txt/log/menu；状态 data-integration-status-user/machine；
  按钮 data-integration-apply-user / remove-user / apply-machine / remove-machine。
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import ConfirmDialog from '../../lib/components/ConfirmDialog.svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import {
    describeIpcError,
    ipc,
    toIpcError,
    type IntegOptions,
    type IntegScope,
    type IntegStatus,
  } from '../../lib/ipc';
  import { toasts } from '../../lib/state/toasts.svelte';

  /** 全部关闭（注销用目标状态）。 */
  const OFF: IntegOptions = { txt: false, log: false, contextMenu: false };

  let status = $state<IntegStatus | null>(null);
  let desired = $state<IntegOptions>({ txt: true, log: true, contextMenu: true });
  let busy = $state(false);
  let confirmOpen = $state(false);
  let pendingMachine = $state<{ remove: boolean } | null>(null);

  onMount(() => {
    void refresh();
  });

  /** 刷新双作用域状态；勾选态默认对齐当前用户状态。 */
  async function refresh(): Promise<void> {
    try {
      status = await ipc.integrationStatus();
      if (status) desired = { ...status.user };
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 执行注册/注销；成功刷新状态并提示。 */
  async function apply(scope: IntegScope, options: IntegOptions, elevate: boolean): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      status = await ipc.integrationApply(scope, options, elevate);
      if (scope === 'user' && status) desired = { ...status.user };
      toasts.show(t('settings.integration.applied'));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    } finally {
      busy = false;
    }
  }

  /** 「所有用户」操作前确认（会触发 UAC）。 */
  /** 打开系统「默认应用」设置页（系统不允许程序静默设为默认，需用户手动确认）。 */
  async function openDefaults(): Promise<void> {
    try {
      await ipc.openDefaultAppsSettings();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  function askMachine(remove: boolean): void {
    pendingMachine = { remove };
    confirmOpen = true;
  }

  function confirmMachine(): void {
    confirmOpen = false;
    const request = pendingMachine;
    pendingMachine = null;
    if (request) void apply('machine', request.remove ? OFF : desired, true);
  }

  /** 状态徽标文本。 */
  function label(on: boolean): string {
    return on ? t('settings.integration.on') : t('settings.integration.off');
  }
</script>

<p class="section-title">{t('settings.integration.title')}</p>
<div class="rows">
  <div class="row">
    <span class="hint integration-desc">{t('settings.integration.desc')}</span>
  </div>
  <div class="row">
    <span class="label" id="integration-lbl-txt">{t('settings.integration.txt')}</span>
    <span class="control">
      <input
        type="checkbox"
        data-integration-txt
        aria-labelledby="integration-lbl-txt"
        bind:checked={desired.txt}
      />
    </span>
  </div>
  <div class="row">
    <span class="label" id="integration-lbl-log">{t('settings.integration.log')}</span>
    <span class="control">
      <input
        type="checkbox"
        data-integration-log
        aria-labelledby="integration-lbl-log"
        bind:checked={desired.log}
      />
    </span>
  </div>
  <div class="row">
    <span class="label" id="integration-lbl-menu">{t('settings.integration.menu')}</span>
    <span class="control">
      <input
        type="checkbox"
        data-integration-menu
        aria-labelledby="integration-lbl-menu"
        bind:checked={desired.contextMenu}
      />
    </span>
  </div>
  <div class="row">
    <span class="label">{t('settings.integration.statusUser')}</span>
    <span class="control integration-status" data-integration-status-user>
      <span data-integration-chip="user-txt" data-on={status?.user.txt ? 'true' : 'false'}>
        .txt · {label(status?.user.txt ?? false)}
      </span>
      <span data-integration-chip="user-log" data-on={status?.user.log ? 'true' : 'false'}>
        .log · {label(status?.user.log ?? false)}
      </span>
      <span data-integration-chip="user-menu" data-on={status?.user.contextMenu ? 'true' : 'false'}>
        {t('settings.integration.menu')} · {label(status?.user.contextMenu ?? false)}
      </span>
    </span>
  </div>
  <div class="row">
    <span class="label">{t('settings.integration.statusMachine')}</span>
    <span class="control integration-status" data-integration-status-machine>
      <span data-integration-chip="machine-txt" data-on={status?.machine.txt ? 'true' : 'false'}>
        .txt · {label(status?.machine.txt ?? false)}
      </span>
      <span data-integration-chip="machine-log" data-on={status?.machine.log ? 'true' : 'false'}>
        .log · {label(status?.machine.log ?? false)}
      </span>
      <span data-integration-chip="machine-menu" data-on={status?.machine.contextMenu ? 'true' : 'false'}>
        {t('settings.integration.menu')} · {label(status?.machine.contextMenu ?? false)}
      </span>
    </span>
  </div>
  <div class="row">
    <span class="label">{t('settings.integration.applyUser')}</span>
    <span class="control integration-actions">
      <button
        class="btn"
        type="button"
        data-integration-apply-user
        disabled={busy}
        onclick={() => void apply('user', desired, false)}
      >
        {t('settings.integration.applyUser')}
      </button>
      <button
        class="btn"
        type="button"
        data-integration-remove-user
        disabled={busy}
        onclick={() => void apply('user', OFF, false)}
      >
        {t('settings.integration.removeUser')}
      </button>
    </span>
  </div>
  <div class="row">
    <span class="label">{t('settings.integration.applyMachine')}</span>
    <span class="control integration-actions">
      <button
        class="btn"
        type="button"
        data-integration-apply-machine
        disabled={busy}
        onclick={() => askMachine(false)}
      >
        {t('settings.integration.applyMachine')}
      </button>
      <button
        class="btn"
        type="button"
        data-integration-remove-machine
        disabled={busy}
        onclick={() => askMachine(true)}
      >
        {t('settings.integration.removeMachine')}
      </button>
    </span>
  </div>
  <div class="row">
    <span class="label">{t('settings.integration.defaultApps')}</span>
    <span class="control integration-actions">
      <button
        class="btn"
        type="button"
        data-integration-open-defaults
        onclick={() => void openDefaults()}
      >
        {t('settings.integration.openDefaults')}
      </button>
    </span>
  </div>
  <div class="row">
    <span class="hint integration-desc">{t('settings.integration.defaultHint')}</span>
  </div>
</div>

{#if confirmOpen}
  <ConfirmDialog
    open={true}
    title={t('settings.integration.elevateTitle')}
    message={t('settings.integration.elevateMessage')}
    confirmLabel={t('settings.integration.elevateConfirm')}
    onCancel={() => {
      confirmOpen = false;
      pendingMachine = null;
    }}
    onConfirm={confirmMachine}
  />
{/if}

<style>
  .integration-desc {
    color: var(--muted);
    font-size: 12.5px;
    line-height: 1.6;
  }

  .integration-status {
    gap: 12px;
    flex-wrap: wrap;
  }

  .integration-status > span {
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 2px 10px;
    font-size: 12px;
    color: var(--muted);
    white-space: nowrap;
  }

  .integration-status > span[data-on='true'] {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 55%, var(--line));
  }

  .integration-actions {
    gap: 10px;
    flex-wrap: wrap;
  }
</style>
