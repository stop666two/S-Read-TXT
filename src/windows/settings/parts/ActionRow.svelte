<!--
  ActionRow — 通用动作行（设置页共用）。
  右侧按钮触发 `onAction`（如清空历史）；由使用方负责二次确认。
-->
<script lang="ts">
  interface Props {
    /** 设置项名称（行标题） */
    label: string;
    /** 补充说明（行副标题） */
    desc: string;
    /** 按钮文案 */
    buttonLabel: string;
    /** 设置标识（写入 data-setting） */
    setting: string;
    /** 点击回调 */
    onAction: () => void;
  }

  let { label, desc, buttonLabel, setting, onAction }: Props = $props();
</script>

<div class="row">
  <span class="label">{label}<small>{desc}</small></span>
  <span class="control">
    <button class="danger" type="button" data-setting={setting} onclick={onAction}>{buttonLabel}</button>
  </span>
</div>
