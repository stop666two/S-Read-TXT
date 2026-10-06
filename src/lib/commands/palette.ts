import type { MessageKey } from '../i18n/zh-CN';

// 命令面板模型与检索（纯逻辑，供组件与单测共用）。
// 命令由 App 层构建（闭包捕获各操作处理器）；此处只定义结构与检索规则。

/** 单条命令 */
export interface PaletteCommand {
  /** 稳定标识（如 file.open；用于测试定位与去重断言） */
  id: string;
  /** 名称语言包键 */
  titleKey: MessageKey;
  /** 分组语言包键（检索与展示分组用） */
  groupKey: MessageKey;
  /** 快捷键提示（组合键字符串；无绑定为空串） */
  shortcutHint: string;
  /** 当前是否可执行（编辑态动作等在阅读态禁用） */
  enabled: boolean;
  /** 不可执行原因语言包键（enabled 为 false 时展示） */
  disabledReasonKey?: MessageKey;
  /** 执行动作 */
  run: () => void;
}

/** 键盘导航：环绕取下一个下标（空列表返回 -1）。 */
export function nextIndex(current: number, length: number, delta: number): number {
  if (length <= 0) return -1;
  const base = current < 0 ? 0 : current;
  return (base + delta + length) % length;
}

/** 检索：空查询返回全部；否则按空白分词，标题/分组/标识需全部命中（大小写不敏感）。 */
export function filterCommands(
  commands: readonly PaletteCommand[],
  query: string,
  translate: (key: MessageKey) => string,
): PaletteCommand[] {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) return [...commands];
  const terms = trimmed.split(/\s+/);
  return commands.filter((command) => {
    const haystack = `${translate(command.titleKey)} ${translate(command.groupKey)} ${command.id}`.toLowerCase();
    return terms.every((term) => haystack.includes(term));
  });
}
