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
  }

  let { label, desc, checked, setting, onCommit }: Props = $props();
</script>

<div class="row">
  <span class="label" id={`lbl-${setting}`}>{label}<small>{desc}</small></span>
  <span class="control">
    <input
      type="checkbox"
      data-setting={setting}
      aria-labelledby={`lbl-${setting}`}
      {checked}
      onchange={(event) => onCommit((event.currentTarget as HTMLInputElement).checked)}
    />
  </span>
</div>
