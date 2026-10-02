// 行文本 LRU 缓存：按「总字符预算」限量，防止超长行（8KB 分块）撑爆内存。
// 实现：Map 的插入序即访问序（命中后重插实现 LRU 提升）。

export class RowCache {
  /** 行号 → 文本（插入序 = 最近使用序，队首最旧） */
  private readonly entries = new Map<number, string>();
  /** 当前缓存字符总数 */
  private charCount = 0;
  /** 字符预算上限（默认 400 万字符 ≈ 8MB / UTF-16） */
  private readonly maxChars: number;

  constructor(maxChars = 4_000_000) {
    this.maxChars = Math.max(1, maxChars);
  }

  /** 读取（命中则提升为最近使用）。 */
  get(row: number): string | undefined {
    const text = this.entries.get(row);
    if (text === undefined) return undefined;
    this.entries.delete(row);
    this.entries.set(row, text);
    return text;
  }

  /** 是否已缓存。 */
  has(row: number): boolean {
    return this.entries.has(row);
  }

  /**
   * 写入（覆盖时修正字符计数），随后按 LRU 驱逐直到回到预算内。
   * 边界：单行自身超预算时保留该行（否则该行永远无法缓存），仅驱逐其他行。
   */
  set(row: number, text: string): void {
    const existing = this.entries.get(row);
    if (existing !== undefined) {
      this.charCount -= existing.length;
      this.entries.delete(row);
    }
    this.entries.set(row, text);
    this.charCount += text.length;
    while (this.charCount > this.maxChars) {
      const oldest = this.entries.keys().next();
      if (oldest.done) break;
      const key = oldest.value;
      if (key === row) break;
      const value = this.entries.get(key);
      if (value !== undefined) {
        this.charCount -= value.length;
        this.entries.delete(key);
      }
    }
  }

  /** 清空（编码切换/标签切换时调用）。 */
  clear(): void {
    this.entries.clear();
    this.charCount = 0;
  }

  /**
   * 失效 `row` 起（含）的缓存（编辑使行号/内容变化时调用）。
   * 说明：整行插入/删除会平移后续行号，从受影响行起全部失效是最小正确范围。
   */
  invalidateFrom(row: number): void {
    for (const [key, value] of this.entries) {
      if (key >= row) {
        this.charCount -= value.length;
        this.entries.delete(key);
      }
    }
  }

  /** 当前缓存行数（诊断用）。 */
  get size(): number {
    return this.entries.size;
  }
}
