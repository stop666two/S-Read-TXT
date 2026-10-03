<!--
  SliderRow — 通用滑块行（设置页共用）。
  行为：拖动滑块时经 `onLive` 实时预览（节流落盘，可选）；松开或数字框确认时经
  `onCommit` 立即落盘。内部维护拖动态值，外部值变化（重载/重置）自动同步。
-->
<script lang="ts">
  import { untrack } from 'svelte';

  interface Props {
    /** 设置项名称（行标题） */
    label: string;
    /** 补充说明（行副标题） */
    desc: string;
    /** 当前值（来自配置快照） */
    value: number;
    /** 允许最小值 */
    min: number;
    /** 允许最大值 */
    max: number;
    /** 步进 */
    step: number;
    /** 单位（显示在数字框右侧） */
    unit: string;
    /** 设置标识（写入 data-setting，供自动化测试与样式定位） */
    setting: string;
    /** 拖动中的实时回调（可省略；省略时拖动仅更新本地显示） */
    onLive?: (value: number) => void;
    /** 确认落盘回调（松开滑块 / 数字框 change） */
    onCommit: (value: number) => void;
  }

  let { label, desc, value, min, max, step, unit, setting, onLive, onCommit }: Props = $props();

  /** 拖动态/输入态当前值（以外部值为源同步；初始值经 untrack 捕获，后续由 $effect 同步） */
  let current = $state(untrack(() => value));
  $effect(() => {
    current = value;
  });

  /** 钳制到允许区间 */
  function clamp(raw: number): number {
    return Math.min(max, Math.max(min, raw));
  }
</script>

<div class="row">
  <span class="label" id={`lbl-${setting}`}>{label}<small>{desc}</small></span>
  <span class="control">
    <input
      type="range"
      data-setting={setting}
      aria-labelledby={`lbl-${setting}`}
      {min}
      {max}
      {step}
      value={current}
      oninput={(event) => {
        current = Number((event.currentTarget as HTMLInputElement).value);
        onLive?.(current);
      }}
      onchange={(event) =>
        // 读事件值而非内部状态：change 可能独立于 input 到达（程序化事件/极端时序）
        onCommit(clamp(Number((event.currentTarget as HTMLInputElement).value)))}
    />
    <input
      class="num"
      type="number"
      data-setting-num={setting}
      aria-labelledby={`lbl-${setting}`}
      {min}
      {max}
      {step}
      value={current}
      onchange={(event) => {
        const parsed = Number((event.currentTarget as HTMLInputElement).value);
        if (!Number.isFinite(parsed)) return;
        current = clamp(parsed);
        onCommit(current);
      }}
    />
    <span class="unit">{unit}</span>
  </span>
</div>
