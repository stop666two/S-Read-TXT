<script lang="ts">
  // 通知堆叠区（右下角、状态栏之上；极简：细色条 + 表面底 + 无阴影）。
  import Icon from './Icon.svelte';
  import { t } from '../i18n/index.svelte';
  import { toasts } from '../state/toasts.svelte';
</script>

<div class="toast-region" aria-live="polite">
  {#each toasts.items as item (item.id)}
    <div class="toast" class:error={item.kind === 'error'} class:warn={item.kind === 'warn'}>
      <span class="bar" aria-hidden="true"></span>
      <span class="text">{item.message}</span>
      <button class="close" title={t('common.closeHint')} aria-label={t('common.closeHint')} onclick={() => toasts.dismiss(item.id)}>
        <Icon name="close" size={11} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toast-region {
    position: absolute;
    right: 12px;
    bottom: calc(var(--h-status) + 10px);
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-end;
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 240px;
    max-width: 400px;
    padding: 8px 8px 8px 0;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 6px;
    pointer-events: auto;
  }

  .bar {
    align-self: stretch;
    width: 3px;
    border-radius: 0 2px 2px 0;
    background: var(--accent);
  }

  .warn .bar {
    background: #c98a2b;
  }

  .error .bar {
    background: #c0503f;
  }

  .text {
    flex: 1;
    color: var(--ink);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--muted);
  }

  .close:hover {
    background: var(--hover);
    color: var(--ink);
  }
</style>
