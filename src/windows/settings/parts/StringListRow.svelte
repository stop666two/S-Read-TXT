<!--
  StringListRow — 字符串列表设置行（正则库）。
  每项一行：文本框 + 上移/下移/删除；底部「添加」。
  变更即时提交整表；maxItems / maxChars 来自后端注册表（前端仅做输入约束，
  后端仍做最终强校验，越界值会被注册表校验拒绝）。
-->
<script lang="ts">
  import { untrack } from 'svelte';

  import { t } from '../../../lib/i18n/index.svelte';

  interface Props {
    /** 设置项名称（行标题） */
    label: string;
    /** 补充说明（行副标题） */
    desc: string;
    /** 当前列表值 */
    value: string[];
    /** 设置标识（写入 data-setting；单项 id = `<setting>.<索引>`） */
    setting: string;
    /** 列表上限（来自注册表 kind.maxItems） */
    maxItems: number;
    /** 单项字符上限（来自注册表 kind.maxChars） */
    maxChars: number;
    /** 可选白名单（来自注册表 kind.allowed；存在时用下拉选择代替自由输入） */
    allowed?: string[];
    /** 白名单选项的标签键前缀（如 `setting.enum.status.items`；缺省直接显示 id） */
    optionPrefix?: string;
    /** 提交回调（整表） */
    onCommit: (value: string[]) => void;
    /** 行级「恢复默认」回调（提供时显示按钮） */
    onReset?: () => void;
    /** 恢复按钮的无障碍标签 */
    resetLabel?: string;
  }
  let { label, desc, value, setting, maxItems, maxChars, allowed, optionPrefix, onCommit, onReset, resetLabel }: Props =
    $props();

  /** 白名单模式下取选项标签（缺失时回退原始 id）。 */
  function optionLabel(id: string): string {
    return optionPrefix ? t(`${optionPrefix}.${id}` as never) : id;
  }

  /** 当前值不在白名单时的兜底选项（理论上归一后不会出现）。 */
  function unknownOption(item: string): boolean {
    return allowed !== undefined && !allowed.includes(item);
  }

  /** 本地列表（以 props 为初值；提交后经父级快照回流保持同步，重置也可同步）。 */
  // svelte-ignore state_referenced_locally
  // （仅取初值：挂载后由下方 effect 按 props 变化同步）
  let items = $state<string[]>(value);

  // props 回流同步（例如行级/分组/全部重置）：仅在内容确实不同时替换，避免自触发循环
  $effect(() => {
    const next = value;
    untrack(() => {
      if (JSON.stringify(next) !== JSON.stringify(items)) items = next;
    });
  });

  /** 整体提交并同步本地状态。 */
  function commit(next: string[]): void {
    items = next;
    onCommit(next);
  }

  /** 编辑单项（保存时按 maxChars 截断，避免超长被后端拒绝）。 */
  function updateItem(index: number, text: string): void {
    if ((items[index] ?? '') === text) return;
    const next = items.slice();
    next[index] = text.slice(0, maxChars);
    commit(next);
  }

  /** 删除单项。 */
  function removeItem(index: number): void {
    const next = items.slice();
    next.splice(index, 1);
    commit(next);
  }

  /** 上移/下移（delta = -1 / +1；越界无操作）。 */
  function moveItem(index: number, delta: number): void {
    const target = index + delta;
    if (target < 0 || target >= items.length) return;
    const next = items.slice();
    const [moved] = next.splice(index, 1);
    next.splice(target, 0, moved as string);
    commit(next);
  }

  /** 末尾追加空项（达到上限时禁用按钮）。 */
  function addItem(): void {
    if (items.length >= maxItems) return;
    commit([...items, '']);
  }
</script>

<div class="row block">
  <span class="label">{label}<small>{desc}</small></span>
  <div class="list">
    {#each items as item, index (index)}
      <div class="item">
        {#if allowed}
          <select
            class="text"
            data-setting={`${setting}.${index}`}
            aria-label={`${label} ${index + 1}`}
            onchange={(event) => updateItem(index, event.currentTarget.value)}
          >
            {#if unknownOption(item)}
              <option value={item} selected>{item}</option>
            {/if}
            {#each allowed as option (option)}
              <option value={option} selected={item === option}>{optionLabel(option)}</option>
            {/each}
          </select>
        {:else}
          <input
            class="text"
            type="text"
            data-setting={`${setting}.${index}`}
            maxlength={maxChars}
            value={item}
            aria-label={`${label} ${index + 1}`}
            spellcheck="false"
            onchange={(event) => updateItem(index, event.currentTarget.value)}
          />
        {/if}
        <button
          class="mini"
          type="button"
          data-stringlist-up={index}
          title={t('settings.stringList.up')}
          aria-label={t('settings.stringList.up')}
          disabled={index === 0}
          onclick={() => moveItem(index, -1)}
        >
          ↑
        </button>
        <button
          class="mini"
          type="button"
          data-stringlist-down={index}
          title={t('settings.stringList.down')}
          aria-label={t('settings.stringList.down')}
          disabled={index === items.length - 1}
          onclick={() => moveItem(index, 1)}
        >
          ↓
        </button>
        <button
          class="mini"
          type="button"
          data-stringlist-remove={index}
          title={t('settings.stringList.remove')}
          aria-label={t('settings.stringList.remove')}
          onclick={() => removeItem(index)}
        >
          ×
        </button>
      </div>
    {/each}
    <div class="item add-row">
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
      <button
        class="add"
        type="button"
        data-stringlist-add
        disabled={items.length >= maxItems}
        onclick={addItem}
      >
        {t('settings.stringList.add')}
      </button>
      {#if items.length === 0}
        <span class="empty">{t('settings.stringList.empty')}</span>
      {/if}
    </div>
  </div>
</div>

<style>
  .block {
    display: block;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 6px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .text {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--base);
    color: var(--ink);
    font: inherit;
    outline: none;
  }

  .text:focus {
    border-color: var(--accent);
  }

  select.text {
    cursor: default;
  }

  .mini {
    min-width: 24px;
    height: 26px;
    padding: 0 5px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: transparent;
    color: var(--ink);
    font: inherit;
    cursor: default;
  }

  .mini:hover:not(:disabled) {
    background: var(--hover);
  }

  .mini:disabled {
    color: var(--muted);
  }

  .add-row {
    margin-top: 2px;
  }

  .add {
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: transparent;
    color: var(--ink);
    font: inherit;
    cursor: default;
  }

  .add:hover:not(:disabled) {
    background: var(--hover);
  }

  .add:disabled {
    color: var(--muted);
  }

  .empty {
    color: var(--muted);
    font-size: 12px;
  }
</style>
