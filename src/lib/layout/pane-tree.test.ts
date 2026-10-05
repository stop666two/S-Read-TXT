import { describe, expect, it } from 'vitest';

import {
  collectLeaves,
  leaf,
  makeSplit,
  normalizeSizes,
  pathToLeaf,
  removeLeaf,
  replaceLeaf,
  siblingLeaf,
  updateSizes,
} from './pane-tree';

const tree = makeSplit(
  'row',
  leaf('main#1'),
  makeSplit('column', leaf('main#2'), leaf('main#3')),
);

describe('布局树纯函数', () => {
  it('collectLeaves 前序遍历全部叶子', () => {
    expect(collectLeaves(tree)).toEqual(['main#1', 'main#2', 'main#3']);
    expect(collectLeaves(leaf('main#9'))).toEqual(['main#9']);
  });

  it('replaceLeaf 仅替换目标叶子并保留结构', () => {
    const next = replaceLeaf(tree, 'main#2', (pane) => makeSplit('row', leaf(pane), leaf('main#4')));
    expect(collectLeaves(next)).toEqual(['main#1', 'main#2', 'main#4', 'main#3']);
    expect(next.type).toBe('split');
    expect(collectLeaves(tree)).toEqual(['main#1', 'main#2', 'main#3']);
  });

  it('removeLeaf 折叠单孩子分支并归一化权重', () => {
    const afterLeaf = removeLeaf(tree, 'main#3');
    expect(afterLeaf).toEqual(makeSplit('row', leaf('main#1'), leaf('main#2')));
    const afterAll = removeLeaf(removeLeaf(afterLeaf!, 'main#1')!, 'main#2');
    expect(afterAll).toBeNull();
  });

  it('removeLeaf 多孩子时剔除对应权重并保持比例', () => {
    const three = {
      type: 'split' as const,
      dir: 'column' as const,
      sizes: [20, 30, 50],
      children: [leaf('a'), leaf('b'), leaf('c')],
    };
    const next = removeLeaf(three, 'b');
    expect(next).toEqual({
      type: 'split',
      dir: 'column',
      sizes: normalizeSizes([20, 50]),
      children: [leaf('a'), leaf('c')],
    });
  });

  it('siblingLeaf 返回同级首个其他叶子，根叶返回 null', () => {
    expect(siblingLeaf(tree, 'main#3')).toBe('main#2');
    expect(siblingLeaf(tree, 'main#2')).toBe('main#3');
    expect(siblingLeaf(tree, 'main#1')).toBe('main#2');
    expect(siblingLeaf(leaf('main#1'), 'main#1')).toBeNull();
  });

  it('pathToLeaf 与 updateSizes 对应', () => {
    expect(pathToLeaf(tree, 'main#3')).toEqual([1, 1]);
    expect(pathToLeaf(tree, 'main#9')).toBeNull();
    const next = updateSizes(tree, [1], [70, 30]);
    expect(next.type).toBe('split');
    if (next.type === 'split') {
      expect(next.sizes).toEqual([50, 50]);
      expect(next.children[1]).toMatchObject({ type: 'split', sizes: [70, 30] });
    }
    expect(updateSizes(tree, [], [80, 20])).toMatchObject({ sizes: [80, 20] });
  });

  it('normalizeSizes 过滤非法值并按比例缩放到 100', () => {
    expect(normalizeSizes([1, 3])).toEqual([25, 75]);
    expect(normalizeSizes([-1, Number.NaN, 2])).toEqual([0, 0, 100]);
    expect(normalizeSizes([0, 0])).toEqual([50, 50]);
  });
});
