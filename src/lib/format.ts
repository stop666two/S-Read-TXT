// 展示格式化工具（纯函数，便于单元测试）。

/**
 * 字节数 → 人类可读文本（B / KB / MB / GB，保留 1 位小数）。
 * 边界：负数按 0 处理；0 → "0 B"。
 */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'] as const;
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const digits = unitIndex === 0 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unitIndex]}`;
}
