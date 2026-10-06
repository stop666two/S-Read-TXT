<!--
  SliderRow — 通用滑块行（设置页共用）。
  行为：拖动滑块时经 `onLive` 实时预览（节流落盘，可选）；松开或数字框确认时经
  `onCommit` 立即落盘。内部维护拖动态值，外部值变化（重载/重置）自动同步。
-->
<script lang="ts">
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
    /** 行级「恢复默认」回调（提供时显示按钮） */
    onReset?: () => void;
    /** 恢复按钮的无障碍标签 */
    resetLabel?: string;
  }

  let { label, desc, value, min, max, step, unit, setting, onLive, onCommit, onReset, resetLabel }: Props =
    $props();

  /** 拖动中的本地覆盖值（仅交互期间使用；外部值到达即自动让位，重置/重载天然生效） */
  let interacting = $state(false);
  let local = $state(0);
  const current = $derived(interacting ? local : value);

  /** 钳制到允许区间 */
  function clamp(raw: number): number {
    return Math.min(max, Math.max(min, raw));
  }
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
      type="range"
      data-setting={setting}
      aria-labelledby={`lbl-${setting}`}
      {min}
      {max}
      {step}
      value={current}
      oninput={(event) => {
        interacting = true;
        local = Number((event.currentTarget as HTMLInputElement).value);
        onLive?.(local);
      }}
      onchange={(event) => {
        // 读事件值而非内部状态：change 可能独立于 input 到达（程序化事件/极端时序）
        const next = clamp(Number((event.currentTarget as HTMLInputElement).value));
        local = next;
        onCommit(next);
        interacting = false;
      }}
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
        const next = clamp(parsed);
        local = next;
        onCommit(next);
      }}
    />
    <span class="unit">{unit}</span>
  </span>
</div>
