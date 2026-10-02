// Toast 通知存储（Svelte 5 Runes；模块级单例，跨组件共享）。
// 生命周期：show 后自动定时消失；重复调用互不干扰；错误类停留更久。

/** 通知类型（语义：信息 / 警告 / 错误；颜色编码含义而非顺序）。 */
export type ToastKind = 'info' | 'warn' | 'error';

/** 单条通知。 */
export interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
}

class ToastStore {
  /** 当前通知列表（新条目追加在尾部，界面右下角堆叠展示） */
  items = $state<ToastItem[]>([]);
  /** 自增 id 生成器 */
  #nextId = 1;
  /** 自动消失定时器（按 id 记录，手动关闭时清理） */
  #timers = new Map<number, ReturnType<typeof setTimeout>>();

  /** 展示一条通知（默认 3.2 秒后自动消失）。 */
  show(message: string, kind: ToastKind = 'info', durationMs = 3200): void {
    const id = this.#nextId;
    this.#nextId += 1;
    this.items = [...this.items, { id, kind, message }];
    this.#timers.set(
      id,
      setTimeout(() => this.dismiss(id), durationMs),
    );
  }

  /** 错误通知快捷方式（停留 5 秒，便于阅读原因）。 */
  error(message: string): void {
    this.show(message, 'error', 5000);
  }

  /** 手动关闭（幂等：重复关闭同一条目无副作用）。 */
  dismiss(id: number): void {
    const timer = this.#timers.get(id);
    if (timer !== undefined) {
      clearTimeout(timer);
      this.#timers.delete(id);
    }
    this.items = this.items.filter((item) => item.id !== id);
  }
}

/** 全局通知单例。 */
export const toasts = new ToastStore();
