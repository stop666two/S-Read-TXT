// 跨标签光标记忆：编辑层组件随标签切换重建，位置经本模块跨实例保留。
// 键 = 标签 id，值 = 最近一次光标位置（切回标签时恢复）。

import type { CaretPos } from './caret';

/** 标签 id → 光标位置（内存态；阶段 8 会话持久化时决定是否落盘）。 */
export const caretMemory = new Map<number, CaretPos>();
