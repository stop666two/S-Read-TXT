/** 跨组件跳转请求（工作区搜索命中定位）。
 *
 * 设计：工作区弹窗位于 App 顶层，命中跳转涉及「切换活动标签 → 滚动定位 →
 * 编辑态设置选区」三类动作，分散在 ReaderView（滚动/虚拟化）与 EditLayer
 * （选区/焦点）。为避免 prop 层层透传，用一个模块级 store 广播：
 * 消费方按 `(tabId, seq)` 判定是否由自己处理（seq 单调递增，保证同位置
 * 重复跳转也会触发一次定位）。
 */

/** 跳转请求（seq=0 表示无请求）。 */
class JumpStore {
  /** 请求序号（单调递增；0 = 初始无请求） */
  seq = $state(0);
  /** 目标标签 id（null = 无） */
  tabId = $state<number | null>(null);
  /** 目标显示行 */
  row = $state(0);
  /** 命中区间起点（段内 UTF-16） */
  from = $state(0);
  /** 命中区间终点（段内 UTF-16） */
  to = $state(0);

  /** 发出定位请求（由 App 在命中点击时调用）。 */
  request(tabId: number, row: number, from: number, to: number): void {
    this.seq += 1;
    this.tabId = tabId;
    this.row = row;
    this.from = from;
    this.to = to;
  }
}

/** 全局单例（应用生命周期内常驻）。 */
export const jumpStore = new JumpStore();
