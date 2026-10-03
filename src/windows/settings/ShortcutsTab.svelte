<!--
  ShortcutsTab — 快捷键设置页签：查看/录制/冲突检测/单条与全部恢复默认。
  语义：后端为唯一真源（生效绑定表 = 默认 + 覆盖）；每次修改立即保存并通知主窗口刷新。
-->
<script lang="ts">
  import { emitTo } from '@tauri-apps/api/event';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { onMount } from 'svelte';

  import { describeIpcError, ipc, toIpcError } from '../../lib/ipc';
  import { t } from '../../lib/i18n/index.svelte';
  import { comboFromEvent, RECORD_ERROR_KEYS, validateRecorded } from '../../lib/shortcuts/keys';
  import { SHORTCUT_ACTIONS, SHORTCUT_LABEL_KEYS, type ShortcutAction } from '../../lib/shortcuts/types';
  import { toasts } from '../../lib/state/toasts.svelte';

  /** 生效绑定（后端快照为唯一真源） */
  let effective = $state<Record<string, string>>({});
  /** 默认绑定（「恢复默认」与「已修改」判定用） */
  let defaults = $state<Record<string, string>>({});
  /** 录制中的动作 id（null = 未录制） */
  let recording = $state<ShortcutAction | null>(null);
  /** 保存进行中（防止并发保存互相覆盖） */
  let saving = $state(false);
  /** 导入/导出进行中（防止并发对话框） */
  let ioBusy = $state(false);

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
      await emitTo('main', 'srt://settings-changed', { kind: 'shortcuts' });
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
        toasts.error(t(RECORD_ERROR_KEYS[error]));
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

  /** 导出快捷键（P0-9）：写「生效绑定」全表为 JSON 文件 */
  async function exportShortcuts(): Promise<void> {
    if (ioBusy) return;
    const path = await save({
      title: t('shortcutIo.exportTitle'),
      defaultPath: 's-read-txt-shortcuts.json',
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (path === null) return;
    ioBusy = true;
    try {
      const bytes = await ipc.exportShortcuts(path);
      toasts.show(t('shortcutIo.exported', { bytes }));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    } finally {
      ioBusy = false;
    }
  }

  /** 导入快捷键（P0-9）：整体替换覆盖项；后端强校验（未知动作/空值/超长/超大拒绝） */
  async function importShortcuts(): Promise<void> {
    if (ioBusy) return;
    const selected = await open({
      title: t('shortcutIo.importTitle'),
      multiple: false,
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (typeof selected !== 'string') return;
    ioBusy = true;
    try {
      const snapshot = await ipc.importShortcuts(selected);
      effective = snapshot.shortcuts.bindings;
      toasts.show(t('shortcutIo.imported'));
      await emitTo('main', 'srt://settings-changed', { kind: 'import' });
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    } finally {
      ioBusy = false;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="wrap">
  <p class="hint">{t('shortcutRecorder.hint')}</p>
  <div class="io-row">
    <button
      class="io-btn"
      type="button"
      data-setting="exportShortcuts"
      disabled={ioBusy}
      onclick={() => void exportShortcuts()}
    >
      {t('shortcutIo.export')}
    </button>
    <button
      class="io-btn"
      type="button"
      data-setting="importShortcuts"
      disabled={ioBusy}
      onclick={() => void importShortcuts()}
    >
      {t('shortcutIo.import')}
    </button>
  </div>
  <ul class="rows">
    {#each SHORTCUT_ACTIONS as action (action)}
      <li class="row">
        <span class="label">{t(SHORTCUT_LABEL_KEYS[action])}</span>
        <button
          class="combo"
          class:recording={recording === action}
          class:modified={isModified(action)}
          type="button"
          onclick={() => startRecording(action)}
        >
          {recording === action ? t('shortcutRecorder.recording') : (effective[action] ?? t('shortcutRecorder.unset'))}
        </button>
        {#if isModified(action)}
          <button class="reset" type="button" onclick={() => resetOne(action)}>{t('shortcutRecorder.resetOne')}</button>
        {/if}
      </li>
    {/each}
  </ul>
  <div class="footer">
    <button class="reset-all" type="button" disabled={saving} onclick={resetAll}>{t('shortcutRecorder.resetAll')}</button>
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

  .io-row {
    display: flex;
    gap: 8px;
  }

  .io-btn {
    padding: 5px 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    color: var(--ink);
    font-size: 12px;
    cursor: pointer;
    transition: border-color 90ms ease, color 90ms ease;
  }

  .io-btn:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }

  .io-btn:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 12px;
    overflow: hidden;
    background: var(--surface);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
  }

  .row:hover {
    background: color-mix(in srgb, var(--hover) 45%, var(--surface));
  }

  .row + .row {
    border-top: 1px solid var(--line);
  }

  .label {
    flex: 1;
    font-size: 13px;
  }

  .combo {
    min-width: 190px;
    padding: 5px 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--base);
    color: var(--ink);
    font-size: 12px;
    cursor: pointer;
    transition: border-color 90ms ease, color 90ms ease, background-color 90ms ease;
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
    padding: 6px 14px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    color: var(--ink);
    font-size: 12px;
    cursor: pointer;
    transition: border-color 90ms ease, color 90ms ease;
  }

  .reset-all:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
