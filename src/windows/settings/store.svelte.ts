// 设置窗口共享保存仓库（src/windows/settings/store.svelte.ts）
// 职责：载入聚合快照；任意页签的修改经「读快照 → 合并补丁 → 保存 → 广播」统一落盘。
// 广播事件 `srt://settings-changed` 供主窗口热刷新（无需重启）。

import { emitTo } from '@tauri-apps/api/event';

import { t } from '../../lib/i18n/index.svelte';
import {
  describeIpcError,
  ipc,
  toIpcError,
  type AppSettings,
  type ReaderSettings,
  type ResetScope,
  type SettingSpec,
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

  /** 拖动实时预览：立即更新本地快照，并按 140ms 节流落盘（主窗口经广播实时生效）。 */
  private liveTimer: ReturnType<typeof setTimeout> | null = null;

  saveReaderLive(patch: Partial<ReaderSettings>): void {
    if (!this.snapshot) return;
    this.snapshot = { ...this.snapshot, reader: { ...this.snapshot.reader, ...patch } };
    if (this.liveTimer !== null) return;
    this.liveTimer = setTimeout(() => {
      this.liveTimer = null;
      if (this.snapshot) void this.persist({ reader: this.snapshot.reader, kind: 'reader' });
    }, 140);
  }

  /** 立即落盘（松开滑块/数字框确认时）；取消未决节流避免重复写。 */
  async saveReaderNow(patch: Partial<ReaderSettings>): Promise<void> {
    if (this.liveTimer !== null) {
      clearTimeout(this.liveTimer);
      this.liveTimer = null;
    }
    await this.saveReader(patch);
  }

  /** 内部：合并保存并广播（shortcuts 始终带上当前生效表，避免覆盖）。
   *  并发保护：快速连续修改（如拖动字号滑块）时仅采纳**最后一次**请求的响应，
   *  防止较旧快照回灌覆盖更新值。 */
  private saveSeq = 0;
  private async persist(change: {
    app?: AppSettings;
    reader?: ReaderSettings;
    kind: 'app' | 'reader';
  }): Promise<void> {
    if (!this.snapshot) return;
    const seq = ++this.saveSeq;
    try {
      const updated = await ipc.saveSettings({
        app: change.app ?? this.snapshot.app,
        reader: change.reader ?? this.snapshot.reader,
        shortcuts: this.snapshot.shortcuts.bindings as Record<string, string>,
      });
      if (seq !== this.saveSeq) return;
      this.snapshot = updated;
      await emitTo('main', 'srt://settings-changed', { kind: change.kind });
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 设置项注册表（P0-4；窗口启动时载入一次，界面据此动态生成） */
  registry = $state<SettingSpec[]>([]);

  /** 载入设置项注册表 */
  async loadRegistry(): Promise<void> {
    try {
      this.registry = await ipc.getSettingsRegistry();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 重置设置（全部 / 分组 / 单项）并采纳返回快照 */
  async resetScope(scope: ResetScope): Promise<void> {
    try {
      this.snapshot = await ipc.resetSettings(scope);
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 导出全部设置到指定路径 */
  async exportTo(path: string): Promise<void> {
    try {
      await ipc.exportSettings(path);
      toasts.show(t('settings.exportDone'));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 从导出文件导入设置（强校验失败提示具体原因） */
  async importFrom(path: string): Promise<void> {
    try {
      this.snapshot = await ipc.importSettings(path);
      toasts.show(t('settings.importDone'));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }
}

/** 设置窗口单例（与主窗口互不影响：各自进程内模块实例） */
export const settings = new SettingsStore();
