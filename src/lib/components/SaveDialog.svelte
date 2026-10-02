<!--
  SaveDialog — 保存询问弹窗（应用内自定义弹窗）。
  职责：每次保存询问目标编码（默认保持当前）与是否写入 `.bak` 备份。
  设计依据：需求 §6.2 / D19「保存每次询问编码（默认保持原编码）」。
-->
<script lang="ts">
  interface Props {
    /** 是否显示 */
    open: boolean;
    /** 当前编码（默认选项值 null = 保持当前编码） */
    currentEncoding: string;
    /** 可选编码列表 */
    encodings: string[];
    /** 备份复选框默认值 */
    defaultBackup: boolean;
    /** 确认（encoding = null 表示保持当前编码） */
    onConfirm: (encoding: string | null, backup: boolean) => void;
    /** 取消/关闭 */
    onCancel: () => void;
  }
  let { open, currentEncoding, encodings, defaultBackup, onConfirm, onCancel }: Props = $props();

  /** 目标编码选择（'' = 保持当前编码） */
  let choice = $state('');
  /** 是否写 .bak */
  let backup = $state(true);

  // 每次打开时重置为默认值（保持当前编码 + 默认备份策略）
  $effect(() => {
    if (open) {
      choice = '';
      backup = defaultBackup;
    }
  });

  function confirm(): void {
    onConfirm(choice === '' ? null : choice, backup);
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault();
      onCancel();
    }
  }
</script>

{#if open}
  <div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onCancel()}>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-label="保存文件"
      tabindex="-1"
      onkeydown={handleKeydown}
    >
      <h2>保存文件</h2>
      <label class="field">
        <span>目标编码</span>
        <select bind:value={choice}>
          <option value="">保持当前编码（{currentEncoding}）</option>
          {#each encodings as encoding (encoding)}
            <option value={encoding}>{encoding}</option>
          {/each}
        </select>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={backup} />
        <span>写入 .bak 备份（覆盖磁盘上前一次内容）</span>
      </label>
      <div class="actions">
        <button class="btn" type="button" onclick={onCancel}>取消</button>
        <button class="btn primary" type="button" onclick={confirm}>保存</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    /* 顶部让位给标题栏：弹窗打开时窗口仍可拖拽、标题栏按钮可用 */
    inset: var(--h-titlebar) 0 0 0;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.28);
  }

  .dialog {
    width: 380px;
    padding: 20px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    color: var(--ink);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.18);
  }

  h2 {
    margin: 0 0 14px;
    font-size: 15px;
    font-weight: 600;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
  }

  select {
    height: 30px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--base);
    color: var(--ink);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 14px;
    font-size: 13px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 20px;
  }

  .btn {
    height: 30px;
    padding: 0 16px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--base);
    color: var(--ink);
    font-size: 13px;
    cursor: pointer;
  }

  .btn.primary {
    border-color: var(--accent);
    background: var(--accent);
    color: #fff;
  }
</style>
