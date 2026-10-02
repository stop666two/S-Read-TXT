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
  }

  let { label, desc, value, options, setting, onCommit }: Props = $props();
</script>

<div class="row">
  <span class="label" id={`lbl-${setting}`}>{label}<small>{desc}</small></span>
  <span class="control">
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
