// 「数据目录不可写」引导的共享状态。
// 为什么是 .svelte.ts：使用 Runes（$state）保存跨组件/钩子共享的弹窗状态；
// App 渲染引导弹窗、main.ts 的自动化钩子与 App 走同一条 apply 路径（E2E 即真实行为）。

import { ipc, type DataDirStatus } from '../ipc';

/** 创建数据目录引导状态（模块级单例）。 */
function createDataDirStore() {
  /** 非空时展示引导弹窗（启动探测到不可写时进入） */
  let issue = $state<DataDirStatus | null>(null);

  return {
    /** 当前引导状态（只读；模板/派生取值）。 */
    get issue(): DataDirStatus | null {
      return issue;
    },

    /** 启动探测：不可写则进入引导（失败不阻塞启动）。 */
    async check(): Promise<void> {
      try {
        const status = await ipc.dataDirStatus();
        if (!status.writable) issue = status;
      } catch (error) {
        // 探测失败不阻塞启动（后续各保存路径会各自报错并提示）
        if (import.meta.env.DEV) console.error('[data-dir] 探测失败', error);
      }
    },

    /** 应用所选目录（真实 UI 选择与自动化钩子共用）；成功则关闭引导。 */
    async apply(dir: string): Promise<DataDirStatus> {
      const status = await ipc.setDataDir(dir);
      if (status.writable) issue = null;
      return status;
    },

    /** 仅本次只读运行：关闭引导（不保存任何数据）。 */
    skip(): void {
      issue = null;
    },
  };
}

/** 全局单例（App 与自动化钩子共用）。 */
export const dataDirStore = createDataDirStore();
