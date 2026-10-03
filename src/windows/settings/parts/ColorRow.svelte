<!--
  ColorRow — 颜色设置行（P1-6：查找高亮色等）。
  值为 CSS 颜色字符串（#RGB / #RRGGBB / #RRGGBBAA / rgb() / rgba()）；
  空串 = 跟随主题内置色（占位留空即为此语义）。
  文本输入失焦或回车提交；取色器用于快捷选色（取整为 #RRGGBB）。
-->
<script lang="ts">
  interface Props {
    /** 设置项名称（行标题） */
    label: string;
    /** 补充说明（行副标题） */
    desc: string;
    /** 当前颜色值（空串 = 跟随主题） */
    value: string;
    /** 设置标识（写入 data-setting） */
    setting: string;
    /** 提交回调（去首尾空白后整体提交） */
    onCommit: (value: string) => void;
    /** 行级「恢复默认」回调（提供时显示按钮） */
    onReset?: () => void;
    /** 恢复按钮的无障碍标签 */
    resetLabel?: string;
  }
  let { label, desc, value, setting, onCommit, onReset, resetLabel }: Props = $props();

  /** 取色器回退色：值不是 6 位十六进制时用内置高亮黄（不改变文本值）。 */
  const PICKER_FALLBACK = '#ffc107';

  /** 取色器显示值（合法 #RRGGBB 用之，否则回退）。 */
  let pickerValue = $derived(/^#[0-9a-fA-F]{6}$/.test(value.trim()) ? value.trim() : PICKER_FALLBACK);

  /** 提交文本输入（去首尾空白；留空表示跟随主题内置色）。 */
  function commitText(element: HTMLInputElement): void {
    onCommit(element.value.trim());
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
      class="color-text"
      type="text"
      data-setting={setting}
      aria-labelledby={`lbl-${setting}`}
      spellcheck="false"
      value={value}
      onchange={(event) => commitText(event.currentTarget)}
      onkeydown={(event) => {
        if (event.key === 'Enter') commitText(event.currentTarget);
      }}
    />
    <input
      class="color-pick"
      type="color"
      aria-label={label}
      value={pickerValue}
      onchange={(event) => onCommit((event.currentTarget as HTMLInputElement).value)}
    />
  </span>
</div>

<style>
  .color-text {
    width: 150px;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--base);
    color: var(--ink);
    font: inherit;
    outline: none;
  }

  .color-text:focus {
    border-color: var(--accent);
  }

  .color-pick {
    width: 30px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--base);
    cursor: pointer;
  }
</style>
