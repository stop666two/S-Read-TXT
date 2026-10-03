// 设置窗口共享保存仓库（src/windows/settings/store.svelte.ts）
// 职责：载入聚合快照；任意页签的修改经「读快照 → 合并补丁 → 保存 → 广播」统一落盘。
// 广播事件 `srt://settings-changed` 供主窗口热刷新（无需重启）。

// 广播事件 `srt://settings-changed` 由后端 `save_settings` 统一发出（无需本层再发）。

import { t } from '../../lib/i18n/index.svelte';
import { formatBytes } from '../../lib/format';
import {
  describeIpcError,
  ipc,
  toIpcError,
  type AppSettings,
  type DiskUsageReport,
  type ReaderSettings,
  type ResetScope,
  type SettingSpec,
  type SettingsSnapshot,
  type ThemeSummary,
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

  /** 磁盘占用（P0-8；打开「常规」页时载入） */
  disk = $state<DiskUsageReport | null>(null);

  /** 主题清单（P0-5；主题行与设置窗口主题应用共用） */
  themes = $state<ThemeSummary[]>([]);

  /** 载入主题清单（内置 + 用户主题） */
  async loadThemes(): Promise<void> {
    try {
      this.themes = await ipc.listThemes();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 选择主题（修改即存；主窗口经广播热更新） */
  async pickTheme(id: string): Promise<void> {
    await this.saveReader({ theme: id });
  }

  /** 导入用户主题（强校验失败提示具体原因；成功后刷新清单） */
  async importThemeFrom(path: string): Promise<void> {
    try {
      const summary = await ipc.importTheme(path);
      await this.loadThemes();
      toasts.show(t('theme.imported', { name: summary.name }));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 导出主题到指定路径（system 由调用方先解析为实际主题 id） */
  async exportThemeTo(path: string, id: string): Promise<void> {
    try {
      await ipc.exportTheme(id, path);
      toasts.show(t('theme.exported'));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 删除用户主题（内置拒绝）；若删除的是当前主题则回退跟随系统 */
  async removeTheme(id: string): Promise<void> {
    try {
      await ipc.removeTheme(id);
      if (this.snapshot?.reader.theme === id) {
        await this.saveReader({ theme: 'system' });
      }
      await this.loadThemes();
      toasts.show(t('theme.removed'));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 设置背景图文件并启用（设置窗口选择图片后调用） */
  async pickBackgroundFile(path: string): Promise<void> {
    try {
      const entry = await ipc.setBackgroundFile(path);
      await this.saveReader({
        background: { ...this.readerBackground(), file: entry.fileName, enabled: true },
      });
      toasts.show(t('background.replaced', { name: entry.label }));
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 移除背景图（尽力删除文件；设置回退为未启用） */
  async clearBackground(): Promise<void> {
    const file = this.readerBackground().file ?? null;
    try {
      if (file) await ipc.clearBackgroundFile(file);
    } catch {
      // 尽力清理：文件缺失/非法名不影响设置回退
    }
    await this.saveReader({
      background: { ...this.readerBackground(), file: null, enabled: false },
    });
    toasts.show(t('background.cleared'));
  }

  /** 当前背景图设置（快照缺失时用默认值） */
  private readerBackground(): ReaderSettings['background'] {
    return (
      this.snapshot?.reader.background ?? {
        enabled: false,
        file: null,
        opacity: 40,
        fill: 'cover',
        blur: 0,
        dim: 0,
      }
    );
  }

  /** 载入磁盘占用分项 */
  async loadDisk(): Promise<void> {
    try {
      this.disk = await ipc.getDiskUsage();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 清理缓存范围并刷新占用（被占用文件计入 skipped，提示中说明） */
  async clearCache(scope: 'logs' | 'webview' | 'backups'): Promise<void> {
    try {
      const result = await ipc.clearCache(scope);
      if (result.skipped > 0) {
        toasts.show(
          t('settings.disk.clearedPartial', {
            bytes: formatBytes(result.clearedBytes),
            skipped: result.skipped,
          }),
        );
      } else {
        toasts.show(t('settings.disk.cleared', { bytes: formatBytes(result.clearedBytes) }));
      }
      await this.loadDisk();
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }
}

/** 设置窗口单例（与主窗口互不影响：各自进程内模块实例） */
export const settings = new SettingsStore();
