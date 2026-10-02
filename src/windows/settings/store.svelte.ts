// 设置窗口共享保存仓库（src/windows/settings/store.svelte.ts）
// 职责：载入聚合快照；任意页签的修改经「读快照 → 合并补丁 → 保存 → 广播」统一落盘。
// 广播事件 `srt://settings-changed` 供主窗口热刷新（无需重启）。

import { emitTo } from '@tauri-apps/api/event';

import {
  describeIpcError,
  ipc,
  toIpcError,
  type AppSettings,
  type ReaderSettings,
  type SettingsSnapshot,
} from '../../lib/ipc';
import { toasts } from '../../lib/state/toasts.svelte';

class SettingsStore {
  /** 聚合快照（null = 尚未载入） */
  snapshot = $state<SettingsSnapshot | null>(null);

  /** 载入配置（窗口启动时调用一次） */
  async load(): Promise<void> {
    try {
      this.snapshot = await ipc.getSettings();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 保存主配置补丁（修改即存） */
  async saveApp(patch: Partial<AppSettings>): Promise<void> {
    if (!this.snapshot) return;
    await this.persist({ app: { ...this.snapshot.app, ...patch }, kind: 'app' });
  }

  /** 保存阅读排版补丁（修改即存） */
  async saveReader(patch: Partial<ReaderSettings>): Promise<void> {
    if (!this.snapshot) return;
    await this.persist({ reader: { ...this.snapshot.reader, ...patch }, kind: 'reader' });
  }

  /** 内部：合并保存并广播（shortcuts 始终带上当前生效表，避免覆盖） */
  private async persist(change: {
    app?: AppSettings;
    reader?: ReaderSettings;
    kind: 'app' | 'reader';
  }): Promise<void> {
    if (!this.snapshot) return;
    try {
      const updated = await ipc.saveSettings({
        app: change.app ?? this.snapshot.app,
        reader: change.reader ?? this.snapshot.reader,
        shortcuts: this.snapshot.shortcuts.bindings as Record<string, string>,
      });
      this.snapshot = updated;
      await emitTo('main', 'srt://settings-changed', { kind: change.kind });
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }
}

/** 设置窗口单例（与主窗口互不影响：各自进程内模块实例） */
export const settings = new SettingsStore();
