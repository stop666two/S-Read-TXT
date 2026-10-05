// 标注状态：按标签缓存书签/高亮/注释，供阅读区渲染与面板共享。
// 说明：读接口轻量（单文件 JSON），按需加载、变更后由命令返回值整体替换缓存，
// 保证与后端（含锚点重定位结果）始终一致；编辑保存后经 refreshSoon 防抖重载。
import { ipc, type FileAnnotations, type NoteKind } from '../ipc';

class AnnotationsStore {
  private map = $state<Record<number, FileAnnotations>>({});
  private timers: Record<number, number> = {};

  /** 标签的标注快照（未加载时 null）。 */
  forTab(tabId: number): FileAnnotations | null {
    return this.map[tabId] ?? null;
  }

  /** 加载（或刷新）标签标注；失败静默（标注非关键路径）。 */
  async load(tabId: number): Promise<void> {
    if (!tabId) return;
    try {
      this.set(tabId, await ipc.listAnnotations(tabId));
    } catch {
      // 标注不可用不应阻塞阅读
    }
  }

  /** 防抖刷新（编辑应用后锚点可能移动，重定位结果需要回读）。 */
  refreshSoon(tabId: number, delayMs = 800): void {
    if (!tabId) return;
    const timer = this.timers[tabId];
    if (timer !== undefined) window.clearTimeout(timer);
    this.timers[tabId] = window.setTimeout(() => {
      delete this.timers[tabId];
      void this.load(tabId);
    }, delayMs);
  }

  /** 标签关闭时清理缓存与定时器。 */
  drop(tabId: number): void {
    const timer = this.timers[tabId];
    if (timer !== undefined) {
      window.clearTimeout(timer);
      delete this.timers[tabId];
    }
    if (tabId in this.map) {
      const next = { ...this.map };
      delete next[tabId];
      this.map = next;
    }
  }

  private set(tabId: number, data: FileAnnotations): void {
    this.map = { ...this.map, [tabId]: data };
  }

  async addBookmark(tabId: number, row: number, utf16: number, label: string | null = null): Promise<void> {
    this.set(tabId, await ipc.addAnnotationBookmark(tabId, row, utf16, label));
  }

  async removeBookmark(tabId: number, id: number): Promise<void> {
    this.set(tabId, await ipc.removeAnnotationBookmark(tabId, id));
  }

  async addHighlight(
    tabId: number,
    row: number,
    startUtf16: number,
    endUtf16: number,
    color: string | null = null,
    note: string | null = null,
  ): Promise<void> {
    this.set(tabId, await ipc.addAnnotationHighlight(tabId, row, startUtf16, endUtf16, color, note));
  }

  async removeHighlight(tabId: number, id: number): Promise<void> {
    this.set(tabId, await ipc.removeAnnotationHighlight(tabId, id));
  }

  async addNote(
    tabId: number,
    row: number,
    utf16: number,
    endUtf16: number | null,
    text: string,
    kind: NoteKind,
  ): Promise<void> {
    this.set(tabId, await ipc.addAnnotationNote(tabId, row, utf16, endUtf16, text, kind));
  }

  async updateNote(tabId: number, id: number, text: string, done: boolean): Promise<void> {
    this.set(tabId, await ipc.updateAnnotationNote(tabId, id, text, done));
  }

  async removeNote(tabId: number, id: number): Promise<void> {
    this.set(tabId, await ipc.removeAnnotationNote(tabId, id));
  }

  async clear(tabId: number): Promise<void> {
    this.set(tabId, await ipc.clearAnnotations(tabId));
  }
}

export const annotations = new AnnotationsStore();
