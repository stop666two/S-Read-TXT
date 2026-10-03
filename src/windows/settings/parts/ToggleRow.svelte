<!--
  ToggleRow — 通用开关行（设置页共用）。
  复选框：勾选状态来自配置快照，切换即经 `onCommit` 落盘。
-->
<script lang="ts">
  interface Props {
    /** 设置项名称（行标题） */
    label: string;
    /** 补充说明（行副标题） */
    desc: string;
    /** 当前勾选状态 */
    checked: boolean;
    /** 设置标识（写入 data-setting） */
    setting: string;
    /** 切换回调（立即落盘） */
    onCommit: (checked: boolean) => void;
    /** 行级「恢复默认」回调（提供时显示按钮） */
    onReset?: () => void;
    /** 恢复按钮的无障碍标签 */
    resetLabel?: string;
  }

  let { label, desc, checked, setting, onCommit, onReset, resetLabel }: Props = $props();
</script>

<div class="row">
  <span class="label" id={`lbl-${setting}`}>{label}<small>{desc}</small></span>
  <span class="control">
    {#if onReset}
      <button
        class="reset-mini"
        type="button"
        data-setting-reset={setting}
        title={resetLabel}
        aria-label={resetLabel}
        onclick={onReset}
      >
        <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true">
          <path
            d="M3.5 6.5a5 5 0 1 1 .6 4.4M3.5 3v3.5H7"
            fill="none"
            stroke="currentColor"
            stroke-width="1.2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
    {/if}
    <input
      type="checkbox"
      data-setting={setting}
      aria-labelledby={`lbl-${setting}`}
      {checked}
      onchange={(event) => onCommit((event.currentTarget as HTMLInputElement).checked)}
    />
  </span>
</div>
