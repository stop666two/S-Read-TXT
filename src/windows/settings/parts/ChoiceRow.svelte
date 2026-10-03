<!--
  ChoiceRow — 通用下拉选择行（设置页共用）。
  受控 select：值来自配置快照，选择即经 `onCommit` 落盘。
-->
<script lang="ts">
  interface Option {
    value: string;
    label: string;
  }

  interface Props {
    /** 设置项名称（行标题） */
    label: string;
    /** 补充说明（行副标题） */
    desc: string;
    /** 当前值 */
    value: string;
    /** 选项列表 */
    options: readonly Option[];
    /** 设置标识（写入 data-setting） */
    setting: string;
    /** 选择回调（立即落盘） */
    onCommit: (value: string) => void;
    /** 行级「恢复默认」回调（提供时显示按钮） */
    onReset?: () => void;
    /** 恢复按钮的无障碍标签 */
    resetLabel?: string;
  }

  let { label, desc, value, options, setting, onCommit, onReset, resetLabel }: Props = $props();
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
    <select
      data-setting={setting}
      aria-labelledby={`lbl-${setting}`}
      {value}
      onchange={(event) => onCommit((event.currentTarget as HTMLSelectElement).value)}
    >
      {#each options as item (item.value)}
        <option value={item.value}>{item.label}</option>
      {/each}
    </select>
  </span>
</div>
