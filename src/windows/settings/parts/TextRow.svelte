<!--
  TextRow — 单行文本设置行（设置页共用）。
  输入经修剪后在提交（change / 失焦）时落盘；外部值变化时回落到最新配置。
-->
<script lang="ts">
  interface Props {
    /** 设置项名称（行标题） */
    label: string;
    /** 补充说明（行副标题） */
    desc: string;
    /** 当前配置值 */
    value: string;
    /** 设置标识（写入 data-setting） */
    setting: string;
    /** 最大字符数（与注册表校验一致） */
    maxLen: number;
    /** 输入占位提示 */
    placeholder?: string;
    /** 提交回调（修剪后的新值） */
    onCommit: (value: string) => void;
    /** 行级「恢复默认」回调（提供时显示按钮） */
    onReset?: () => void;
    /** 恢复按钮的无障碍标签 */
    resetLabel?: string;
  }

  let {
    label,
    desc,
    value,
    setting,
    maxLen,
    placeholder = '',
    onCommit,
    onReset,
    resetLabel,
  }: Props = $props();

  /** 编辑草稿（null = 跟随配置值，避免外部更新被旧草稿覆盖） */
  let draft = $state<string | null>(null);
  const shown = $derived(draft ?? value);

  /** 提交：修剪后与当前值不同才回调（清空草稿回到跟随态） */
  function commit(): void {
    const next = (draft ?? value).trim();
    draft = null;
    if (next !== value) onCommit(next);
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
      class="text-input"
      type="text"
      data-setting={setting}
      maxlength={maxLen}
      {placeholder}
      value={shown}
      aria-labelledby={`lbl-${setting}`}
      oninput={(event) => (draft = event.currentTarget.value)}
      onchange={commit}
      onblur={commit}
    />
  </span>
</div>
