<script lang="ts">
  // 拖拽遮罩：文件拖入窗口时整窗提示（松开即打开）。
  import { t } from '../i18n/index.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    /** 是否处于拖拽悬停状态 */
    visible: boolean;
  }
  let { visible }: Props = $props();
</script>

{#if visible}
  <div class="overlay">
    <Icon name="open" size={36} />
    <p>{t('drop.release')}</p>
  </div>
{/if}

<style>
  .overlay {
    position: absolute;
    inset: 0;
    z-index: 150;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    background: color-mix(in srgb, var(--base) 88%, var(--accent) 12%);
    border: 2px dashed var(--accent);
    box-sizing: border-box;
    color: var(--accent);
    user-select: none;
  }

  .overlay p {
    margin: 0;
    font-size: 14px;
  }
</style>
