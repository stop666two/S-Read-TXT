// 行高模型：实测缓存 + 平均估计 + 分块统计（纯逻辑，供虚拟滚动使用）。
//
// 设计取舍：不缓存全部行高（100MB 文件可达数百万行，全量数组会占用数十 MB 内存），
// 只缓存「渲染过并实测」的行高（通常几百行），未测量行一律用当前平均高度估计；
// 分块（256 行）统计使 offsetOf / rowAtOffset 的复杂度为 O(行数/256 + 256)，
// 对 500 万行文件每帧调用数十次仍为亚毫秒级。

/** 分块行数（统计粒度） */
export const CHUNK_SIZE = 256;

export class HeightModel {
  /** 行号 → 实测高度（px；仅包含已渲染测量过的行） */
  private readonly heights = new Map<number, number>();
  /** 实测高度总和与数量（用于维护平均值） */
  private measuredSum = 0;
  private measuredCount = 0;
  /** 基准行高（clear 后恢复的初始估计值） */
  private readonly baseHeight: number;
  /** 当前估计行高（未测量行的取值；随实测自动校准） */
  private estimate: number;

  /** 构造：baseLineHeight 为初始估计行高（如 16px × 1.8 ≈ 29px）。 */
  constructor(baseLineHeight: number) {
    this.baseHeight = Math.max(1, baseLineHeight);
    this.estimate = this.baseHeight;
  }

  /** 当前估计行高（px）。 */
  get estimated(): number {
    return this.estimate;
  }

  /** 已实测行数（诊断用）。 */
  get measuredRows(): number {
    return this.measuredCount;
  }

  /**
   * 记录一次行高实测。
   * 边界：非正高度忽略；重复测量覆盖并同步维护均值。
   */
  measure(row: number, height: number): void {
    if (!(height > 0)) return;
    const previous = this.heights.get(row);
    if (previous === undefined) {
      this.measuredCount += 1;
      this.measuredSum += height;
    } else {
      this.measuredSum += height - previous;
    }
    this.heights.set(row, height);
    this.estimate = this.measuredSum / this.measuredCount;
  }

  /** 单行高度（实测优先，否则估计）。 */
  heightOf(row: number): number {
    return this.heights.get(row) ?? this.estimate;
  }

  /** 指定行之前的累计高度（px）。row 会被限制在 [0, totalRows]。 */
  offsetOf(row: number, totalRows: number): number {
    if (row <= 0 || totalRows <= 0) return 0;
    const target = Math.min(row, totalRows);
    let offset = 0;
    const fullChunks = Math.floor(target / CHUNK_SIZE);
    for (let chunk = 0; chunk < fullChunks; chunk += 1) {
      offset += this.chunkHeight(chunk);
    }
    const start = fullChunks * CHUNK_SIZE;
    for (let r = start; r < target; r += 1) {
      offset += this.heightOf(r);
    }
    return offset;
  }

  /**
   * 累计高度 → 所在行（返回行号；行内偏移由调用方按需保留）。
   * 边界：offset ≤ 0 → 0；超出总量 → totalRows - 1；totalRows ≤ 0 → 0。
   */
  rowAtOffset(offset: number, totalRows: number): number {
    if (offset <= 0 || totalRows <= 0) return 0;
    let remaining = offset;
    const chunkCount = Math.ceil(totalRows / CHUNK_SIZE);
    for (let chunk = 0; chunk < chunkCount; chunk += 1) {
      const height = this.chunkHeight(chunk);
      if (remaining < height) {
        const start = chunk * CHUNK_SIZE;
        const end = Math.min(start + CHUNK_SIZE, totalRows);
        for (let r = start; r < end; r += 1) {
          const rowHeight = this.heightOf(r);
          if (remaining < rowHeight) return r;
          remaining -= rowHeight;
        }
        return Math.max(0, end - 1);
      }
      remaining -= height;
    }
    return totalRows - 1;
  }

  /** 全内容估算高度（滚动条范围依据）。 */
  totalHeight(totalRows: number): number {
    return this.offsetOf(totalRows, totalRows);
  }

  /** 清空实测（字号/列宽变化、编码切换、标签切换时调用），估计值恢复基准。 */
  clear(): void {
    this.heights.clear();
    this.measuredSum = 0;
    this.measuredCount = 0;
    this.estimate = this.baseHeight;
  }

  /** 单块高度 = 块内实测之和 + 未测量行数 × 估计。 */
  private chunkHeight(chunk: number): number {
    const start = chunk * CHUNK_SIZE;
    const end = start + CHUNK_SIZE;
    let sum = 0;
    let measuredInChunk = 0;
    for (let r = start; r < end; r += 1) {
      const height = this.heights.get(r);
      if (height !== undefined) {
        sum += height;
        measuredInChunk += 1;
      }
    }
    return sum + (CHUNK_SIZE - measuredInChunk) * this.estimate;
  }
}
