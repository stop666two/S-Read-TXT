<!--
  ShortcutsTab — 快捷键设置页签：查看/录制/冲突检测/单条与全部恢复默认。
  语义：后端为唯一真源（生效绑定表 = 默认 + 覆盖）；每次修改立即保存并通知主窗口刷新。
-->
<script lang="ts">
  import { emitTo } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';

  import { describeIpcError, ipc, toIpcError } from '../../lib/ipc';
  import { comboFromEvent, RECORD_ERROR_MESSAGES, validateRecorded } from '../../lib/shortcuts/keys';
  import { SHORTCUT_ACTIONS, SHORTCUT_LABELS, type ShortcutAction } from '../../lib/shortcuts/types';
  import { toasts } from '../../lib/state/toasts.svelte';

  /** 生效绑定（后端快照为唯一真源） */
  let effective = $state<Record<string, string>>({});
  /** 默认绑定（「恢复默认」与「已修改」判定用） */
  let defaults = $state<Record<string, string>>({});
  /** 录制中的动作 id（null = 未录制） */
  let recording = $state<ShortcutAction | null>(null);
  /** 保存进行中（防止并发保存互相覆盖） */
  let saving = $state(false);

  onMount(() => {
    void load();
  });

  /** 载入生效绑定与默认表 */
  async function load(): Promise<void> {
    try {
      const [snapshot, defaultMap] = await Promise.all([
        ipc.getSettings(),
        ipc.getDefaultShortcuts(),
      ]);
      effective = snapshot.shortcuts.bindings;
      defaults = defaultMap;
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 保存生效绑定（读快照 → 覆盖 shortcuts → 落盘 → 通知主窗口刷新） */
  async function persist(next: Record<string, string>): Promise<void> {
    if (saving) return;
    saving = true;
    try {
      const snapshot = await ipc.getSettings();
      const updated = await ipc.saveSettings({
        app: snapshot.app,
        reader: snapshot.reader,
        shortcuts: next,
      });
      effective = updated.shortcuts.bindings;
      toasts.show('快捷键已保存');
      await emitTo('main', 'srt://shortcuts-changed');
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    } finally {
      saving = false;
    }
  }

  /** 录制键盘输入：录制中捕获所有按键（Esc 取消；仅修饰键继续等待主键） */
  function onKeydown(event: KeyboardEvent): void {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === 'Escape') {
      recording = null;
      return;
    }
    const combo = comboFromEvent(event);
    if (!combo) return;
    const action = recording;
    const error = validateRecorded(combo, action, effective);
    if (error) {
      toasts.error(RECORD_ERROR_MESSAGES[error]);
      return;
    }
    recording = null;
    void persist({ ...effective, [action]: combo });
  }

  /** 开始录制指定动作 */
  function startRecording(action: ShortcutAction): void {
    recording = action;
  }

  /** 单条恢复默认 */
  function resetOne(action: ShortcutAction): void {
    const fallback = defaults[action] ?? effective[action];
    if (fallback !== undefined) void persist({ ...effective, [action]: fallback });
  }

  /** 全部恢复默认（提交默认表 → 后端覆盖表清空） */
  function resetAll(): void {
    void persist({ ...defaults });
  }

  /** 该动作是否被修改过（与默认不同） */
  const isModified = (action: ShortcutAction): boolean =>
    effective[action] !== undefined && effective[action] !== defaults[action];
</script>

<svelte:window onkeydown={onKeydown} />

<div class="wrap">
  <p class="hint">
    点击组合键后按下新按键（Esc 取消）；修改立即保存。Ctrl+1~9 为固定标签跳转键，不可占用。
  </p>
  <ul class="rows">
    {#each SHORTCUT_ACTIONS as action (action)}
      <li class="row">
        <span class="label">{SHORTCUT_LABELS[action]}</span>
        <button
          class="combo"
          class:recording={recording === action}
          class:modified={isModified(action)}
          type="button"
          onclick={() => startRecording(action)}
        >
          {recording === action ? '按下新组合…（Esc 取消）' : (effective[action] ?? '未设置')}
        </button>
        {#if isModified(action)}
          <button class="reset" type="button" onclick={() => resetOne(action)}>恢复默认</button>
        {/if}
      </li>
    {/each}
  </ul>
  <div class="footer">
    <button class="reset-all" type="button" disabled={saving} onclick={resetAll}>全部恢复默认</button>
  </div>
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .hint {
    margin: 0;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.6;
  }

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 8px;
    overflow: hidden;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 12px;
    background: var(--surface);
  }

  .row + .row {
    border-top: 1px solid var(--line);
  }

  .label {
    flex: 1;
    font-size: 13px;
  }

  .combo {
    min-width: 180px;
    padding: 4px 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--base);
    color: var(--ink);
    font-size: 12px;
    cursor: pointer;
  }

  .combo:hover {
    border-color: var(--accent);
  }

  .combo.modified {
    color: var(--accent);
    border-color: var(--accent);
  }

  .combo.recording {
    color: var(--accent);
    border-style: dashed;
    border-color: var(--accent);
  }

  .reset {
    border: none;
    background: transparent;
    color: var(--muted);
    font-size: 12px;
    cursor: pointer;
    padding: 2px 4px;
  }

  .reset:hover {
    color: var(--accent);
  }

  .footer {
    display: flex;
    justify-content: flex-end;
  }

  .reset-all {
    padding: 5px 12px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--surface);
    color: var(--ink);
    font-size: 12px;
    cursor: pointer;
  }

  .reset-all:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
