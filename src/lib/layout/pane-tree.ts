// 分屏布局树：窗口内栏位的树形切分（叶子=栏位键，分支=行/列与权重）。
// 纯函数实现，供 App 状态更新、PaneTree 渲染与会话 v3 序列化共用。

export type PaneSplitDir = 'row' | 'column';

export type PaneLayout =
  | { type: 'leaf'; pane: string }
  | { type: 'split'; dir: PaneSplitDir; sizes: number[]; children: PaneLayout[] };

/** 单窗口栏位上限（与后端 MAX_PANES 一致）。 */
export const MAX_PANES = 4;

/** 权重下限（百分比）：分隔条拖动时相邻子项各自保留的最小份额。 */
export const MIN_SIZE = 15;

/** 叶子节点。 */
export function leaf(pane: string): PaneLayout {
  return { type: 'leaf', pane };
}

/** 分支节点：两个子树等分。 */
export function makeSplit(dir: PaneSplitDir, first: PaneLayout, second: PaneLayout): PaneLayout {
  return { type: 'split', dir, sizes: [50, 50], children: [first, second] };
}

/** 前序收集全部叶子栏位键。 */
export function collectLeaves(node: PaneLayout, out: string[] = []): string[] {
  if (node.type === 'leaf') {
    out.push(node.pane);
    return out;
  }
  for (const child of node.children) collectLeaves(child, out);
  return out;
}

/** 替换指定叶子（make 接收原叶子键，返回替换后的子树）。 */
export function replaceLeaf(
  node: PaneLayout,
  target: string,
  make: (pane: string) => PaneLayout,
): PaneLayout {
  if (node.type === 'leaf') return node.pane === target ? make(target) : node;
  return { ...node, children: node.children.map((child) => replaceLeaf(child, target, make)) };
}

/** 移除指定叶子并折叠单孩子分支；根被移除时返回 null。 */
export function removeLeaf(node: PaneLayout, target: string): PaneLayout | null {
  if (node.type === 'leaf') return node.pane === target ? null : node;
  const kept: PaneLayout[] = [];
  const keptSizes: number[] = [];
  node.children.forEach((child, index) => {
    const next = removeLeaf(child, target);
    if (next !== null) {
      kept.push(next);
      keptSizes.push(node.sizes[index] ?? 0);
    }
  });
  if (kept.length === 0) return null;
  if (kept.length === 1) return kept[0];
  return { ...node, sizes: normalizeSizes(keptSizes), children: kept };
}

/** 目标叶子的最近同级首个其他叶子（优先最内层分支）；根叶返回 null。 */
export function siblingLeaf(node: PaneLayout, target: string): string | null {
  if (node.type === 'leaf') return null;
  const hit = node.children.findIndex((child) => collectLeaves(child).includes(target));
  if (hit < 0) return null;
  const deeper = siblingLeaf(node.children[hit], target);
  if (deeper !== null) return deeper;
  for (let i = 0; i < node.children.length; i += 1) {
    if (i === hit) continue;
    const leaves = collectLeaves(node.children[i]);
    if (leaves.length > 0) return leaves[0];
  }
  return null;
}

/** 目标叶子的路径（从根到叶子的孩子下标序列；未找到返回 null）。 */
export function pathToLeaf(node: PaneLayout, target: string, path: number[] = []): number[] | null {
  if (node.type === 'leaf') return node.pane === target ? [...path] : null;
  for (let i = 0; i < node.children.length; i += 1) {
    const found = pathToLeaf(node.children[i], target, [...path, i]);
    if (found !== null) return found;
  }
  return null;
}

/** 按路径更新分支的权重（路径指向的节点必须是分支；未命中时原样返回）。 */
export function updateSizes(node: PaneLayout, path: number[], sizes: number[]): PaneLayout {
  if (path.length === 0) {
    return node.type === 'split' ? { ...node, sizes: normalizeSizes(sizes) } : node;
  }
  if (node.type === 'leaf') return node;
  const [index, ...rest] = path;
  return {
    ...node,
    children: node.children.map((child, i) =>
      i === index ? updateSizes(child, rest, sizes) : child,
    ),
  };
}

/** 权重归一化：过滤非法值，保持比例缩放到总计 100。 */
export function normalizeSizes(sizes: number[]): number[] {
  const clean = sizes.map((size) => (Number.isFinite(size) && size > 0 ? size : 0));
  const total = clean.reduce((sum, size) => sum + size, 0);
  if (total <= 0) return clean.map(() => 100 / clean.length);
  return clean.map((size) => (size / total) * 100);
}
