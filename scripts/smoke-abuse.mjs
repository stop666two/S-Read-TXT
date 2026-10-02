#!/usr/bin/env node
// 对抗级 E2E 冒烟（阶段 4b 扩测）：真实应用 + CDP，模拟「不老实」的用户操作。
//
// 覆盖：空文件编辑、CR/BOM 边角、快速连打、撤销/重做狂按、全选替换、
// 退格/删除/回车狂按、emoji/RTL/零宽字符、大粘贴、超长行拒绝、脏关闭守卫、
// 外部修改冲突覆盖、空文件清空保存。
//
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-abuse.mjs [--exe <路径>] [--port <端口>] [--screenshot <路径>]

import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, findTarget, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exePath = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const port = Number(argValue('--port', String(9200 + Math.floor(Math.random() * 600))));
const screenshotPath = argValue('--screenshot', join(root, 'docs', 'screenshots', 'phase4b-abuse.png'));

const workDir = join(process.env.TEMP ?? '.', 'srt-smoke-abuse');
const runDataDir = join(workDir, `data-${Date.now()}`);

const files = {
  empty: join(workDir, 'empty.txt'),
  cr: join(workDir, 'cr.txt'),
  bom: join(workDir, 'bom.txt'),
  norm: join(workDir, 'norm.txt'),
  longline: join(workDir, 'longline.txt'),
  conflict: join(workDir, 'conflict.txt'),
};
const normOriginal = 'alpha\nbeta\ngamma\n';

const checks = [];
function check(name, passed, detail = '') {
  checks.push({ name, passed, detail });
  console.log(`${passed ? 'PASS' : 'FAIL'}  ${name}${detail ? `  ← ${detail}` : ''}`);
}

let evalJs = () => Promise.resolve(undefined);
let client = null;

// ---- 交互辅助 ----

/** 调用后端命令。 */
function invoke(name, args = {}) {
  return evalJs(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)}, ${JSON.stringify(args)})`,
  );
}

/** 当前活动标签（list_tabs）。 */
async function activeTab() {
  const view = await invoke('list_tabs');
  return view.tabs.find((tab) => tab.tabId === view.activeTabId) ?? null;
}

/** 点击选择器元素（返回是否命中）。 */
function click(selector) {
  return evalJs(
    `(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) return false; el.click(); return true; })()`,
  );
}

/** 发送真实键盘事件（rawKeyDown + keyUp）。 */
async function sendKey(key, code, vk, modifiers = 0) {
  await client.send('Input.dispatchKeyEvent', {
    type: 'rawKeyDown',
    key,
    code,
    windowsVirtualKeyCode: vk,
    nativeVirtualKeyCode: vk,
    modifiers,
  });
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    key,
    code,
    windowsVirtualKeyCode: vk,
    nativeVirtualKeyCode: vk,
    modifiers,
  });
}

const CTRL = 2;

/** 打开文件并等待首行内容出现。 */
async function openAndWait(path, expectedRow0) {
  await evalJs(`window.__srt.openPath(${JSON.stringify(path)})`);
  const text = await waitForValue(async () => {
    const row = await evalJs(`document.querySelector('.row')?.textContent ?? ''`);
    return row === expectedRow0 ? row : '';
  }, 8000);
  return text === expectedRow0;
}

/** 点击进入编辑模式并等待 editing 状态。 */
async function toggleEdit(expected) {
  await click('[aria-label="切换编辑模式"]');
  const info = await waitForValue(async () => {
    const tab = await activeTab();
    return tab && tab.editing === expected ? tab : null;
  }, 8000);
  return info;
}

/** 通过保存弹窗保存（保持当前编码）。 */
async function saveViaDialog() {
  await click('[aria-label="保存"]');
  const dialog = await waitForValue(
    async () => (await evalJs(`document.querySelector('[role="dialog"]') !== null`)) || null,
    4000,
  );
  if (!dialog) return false;
  await evalJs(
    `(() => { const dlg = document.querySelector('[role="dialog"]'); const btn = [...dlg.querySelectorAll('button')].find((b) => b.textContent.trim() === '保存'); btn?.click(); return !!btn; })()`,
  );
  return true;
}

/** 读取首行 DOM 文本。 */
function domRow0() {
  return evalJs(`document.querySelector('.row')?.textContent ?? ''`);
}

async function main() {
  mkdirSync(workDir, { recursive: true });
  writeFileSync(files.empty, '');
  writeFileSync(files.cr, 'a\rb\r');
  writeFileSync(files.bom, Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), Buffer.from('中文')]));
  writeFileSync(files.norm, normOriginal);
  writeFileSync(files.longline, 'x'.repeat(70 * 1024));
  writeFileSync(files.conflict, 'conflict-base\n');

  const child = spawn(exePath, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: runDataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });

  try {
    const wsUrl = await findTarget(port);
    client = await createClient(wsUrl);
    evalJs = async (expression) => {
      const result = await client.send('Runtime.evaluate', {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
      if (result.exceptionDetails) {
        const description =
          result.exceptionDetails.exception?.description ??
          result.exceptionDetails.text ??
          '未知异常';
        throw new Error(`页面执行异常：${description}`);
      }
      return result.result?.value;
    };
    for (let attempt = 0; attempt < 60; attempt += 1) {
      const ready = await evalJs('!!window.__srt?.openPath && !!window.__TAURI_INTERNALS__');
      if (ready) break;
      await delay(250);
    }

    // ---- A. 空文件：编辑 → 写入 → 保存 → 清空再保存 ----
    check('A1 打开空文件（1 行）', await openAndWait(files.empty, ''), '首行应为空');
    check('A2 进入编辑', (await toggleEdit(true))?.editing === true);
    await client.send('Input.insertText', { text: 'x' });
    const a3 = await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty ? { row: await domRow0(), dirty: tab.dirty } : null;
    }, 6000);
    check('A3 输入后内容与脏态', a3?.row === 'x' && a3.dirty === true, JSON.stringify(a3));
    check('A4 保存弹窗确认', await saveViaDialog());
    const a5 = await waitForValue(async () => {
      let disk = '';
      try {
        disk = readFileSync(files.empty, 'utf8');
      } catch {
        disk = '';
      }
      const tab = await activeTab();
      return disk === 'x' && tab?.dirty === false ? disk : null;
    }, 6000);
    check('A5 磁盘写入且脏态清除', a5 === 'x');
    // 全选清空 → 保存为空文件
    const a6Focus = await evalJs(
      `document.activeElement?.className || document.activeElement?.tagName || 'none'`,
    );
    await evalJs(
      `(window.__keylog6 = [], document.addEventListener('keydown', (e) => window.__keylog6.push(e.key), true), true)`,
    );
    await sendKey('a', 'KeyA', 65, CTRL);
    await delay(200);
    const a6Keylog = await evalJs(`window.__keylog6.join(',')`);
    const a6Selection = await evalJs(`document.querySelectorAll('.selection').length`);
    await sendKey('Backspace', 'Backspace', 8);
    await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty ? true : null;
    }, 5000);
    await saveViaDialog();
    const a6 = await waitForValue(async () => {
      const size = readFileSync(files.empty).length;
      return size === 0 ? 0 : null;
    }, 6000);
    check(
      'A6 全选清空后保存为 0 字节',
      a6 === 0,
      JSON.stringify({ focus: a6Focus, keylog: a6Keylog, selectionRects: a6Selection, size: readFileSync(files.empty).length }),
    );

    // ---- B. CR 换行文件：编辑后换行族保持 ----
    check('B1 打开 CR 文件（a/b 两行）', await openAndWait(files.cr, 'a'), '首行 a');
    check('B2 进入编辑', (await toggleEdit(true))?.editing === true);
    // 光标初始 (0,0)：行首插入
    await client.send('Input.insertText', { text: '头' });
    await waitForValue(async () => ((await domRow0()) === '头a' ? true : null), 5000);
    check('B3 保存', await saveViaDialog());
    const b4 = await waitForValue(async () => {
      const bytes = readFileSync(files.cr);
      return bytes.toString('utf8') === '头a\rb\r' ? true : null;
    }, 6000);
    check('B4 CR 换行族保持', b4 === true, readFileSync(files.cr).toString('utf8').replace(/\r/g, '\\r'));

    // ---- C. BOM 文件 ----
    check('C1 打开 BOM 文件（无可见 BOM）', await openAndWait(files.bom, '中文'), '首行 中文');
    check('C2 进入编辑', (await toggleEdit(true))?.editing === true);
    await client.send('Input.insertText', { text: '新' });
    await waitForValue(async () => ((await domRow0()) === '新中文' ? true : null), 5000);
    check('C3 保存', await saveViaDialog());
    const c4 = await waitForValue(async () => {
      const bytes = readFileSync(files.bom);
      return bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf &&
        bytes.toString('utf8').slice(1) === '新中文'
        ? true
        : null;
    }, 6000);
    check('C4 BOM 保留且内容正确', c4 === true);

    // ---- D~K. norm 文件的滥用序列 ----
    check('D1 打开 norm 文件', await openAndWait(files.norm, 'alpha'), '首行 alpha');
    check('D2 进入编辑', (await toggleEdit(true))?.editing === true);
    const fifty = '0123456789'.repeat(5);
    await client.send('Input.insertText', { text: fifty });
    const d3 = await waitForValue(async () => {
      const row = await domRow0();
      return row.length === 55 ? row : null; // 50 字符 + 原 alpha
    }, 6000);
    check('D3 快速连打 50 字符', d3 !== null, `length=${d3?.length}`);

    // E. 撤销狂按（30 次，远超步数）
    for (let index = 0; index < 30; index += 1) {
      await sendKey('z', 'KeyZ', 90, CTRL);
    }
    const e1 = await waitForValue(async () => {
      const row = await domRow0();
      const tab = await activeTab();
      return row === 'alpha' && tab?.dirty === false ? { row, dirty: tab.dirty } : null;
    }, 6000);
    check('E1 撤销狂按回到原文且干净', e1?.row === 'alpha' && e1.dirty === false, JSON.stringify(e1));

    // F. 重做狂按
    for (let index = 0; index < 30; index += 1) {
      await sendKey('y', 'KeyY', 89, CTRL);
    }
    const f1 = await waitForValue(async () => {
      const row = await domRow0();
      return row.length === 55 ? row : null;
    }, 6000);
    check('F1 重做狂按恢复 55 字符', f1 !== null, `length=${f1?.length}`);

    // G. 全选替换 → 撤销（带诊断探针）
    const gFocus = await evalJs(
      `document.activeElement?.className || document.activeElement?.tagName || 'none'`,
    );
    await evalJs(
      `(window.__keylogG = [], document.addEventListener('keydown', (e) => window.__keylogG.push(e.key), true), true)`,
    );
    await sendKey('a', 'KeyA', 65, CTRL);
    await delay(250);
    const gKeylog = await evalJs(`window.__keylogG.join(',')`);
    const gSelection = await evalJs(`document.querySelectorAll('.selection').length`);
    await client.send('Input.insertText', { text: '替换' });
    const g1 = await waitForValue(async () => {
      const view = await invoke('list_tabs');
      const tab = view.tabs.find((item) => item.tabId === view.activeTabId);
      const row = await domRow0();
      return tab?.rowsTotal === 1 && row === '替换' ? { rowsTotal: tab.rowsTotal } : null;
    }, 6000);
    check(
      'G1 全选替换为单行',
      g1?.rowsTotal === 1,
      JSON.stringify({ g1, focus: gFocus, keylog: gKeylog, selectionRects: gSelection, row0len: (await domRow0()).length }),
    );
    await sendKey('z', 'KeyZ', 90, CTRL);
    const g2 = await waitForValue(async () => {
      const view = await invoke('list_tabs');
      const tab = view.tabs.find((item) => item.tabId === view.activeTabId);
      const row = await domRow0();
      return tab?.rowsTotal === 3 && row.length === 55 ? { rowsTotal: tab.rowsTotal } : null;
    }, 6000);
    check(
      'G2 撤销恢复三行',
      g2?.rowsTotal === 3,
      JSON.stringify({ g2, row0: (await domRow0()).slice(0, 20), len: (await domRow0()).length }),
    );

    // H. 文档首退格/删除狂按（带焦点探针）
    await sendKey('Home', 'Home', 36, CTRL);
    await delay(150);
    const hFocus = await evalJs(
      `document.activeElement?.className || document.activeElement?.tagName || 'none'`,
    );
    for (let index = 0; index < 20; index += 1) {
      await sendKey('Backspace', 'Backspace', 8);
    }
    await delay(400);
    const h1 = await domRow0();
    check(
      'H1 文档首退格狂按无副作用',
      h1.length === 55,
      JSON.stringify({ length: h1.length, focus: hFocus, row0: h1.slice(0, 20) }),
    );
    for (let index = 0; index < 20; index += 1) {
      await sendKey('Delete', 'Delete', 46);
    }
    const h2 = await waitForValue(async () => {
      const row = await domRow0();
      return row.length === 35 ? row : null;
    }, 6000);
    check('H2 前向删除狂按精确删 20 字符', h2 !== null, `length=${h2?.length}`);

    // I. 回车狂按 → 撤销
    const beforeEnter = await activeTab();
    for (let index = 0; index < 10; index += 1) {
      await sendKey('Enter', 'Enter', 13);
    }
    const i1 = await waitForValue(async () => {
      const tab = await activeTab();
      return tab && tab.rowsTotal === beforeEnter.rowsTotal + 10 ? tab.rowsTotal : null;
    }, 8000);
    check('I1 回车狂按行数 +10', i1 === beforeEnter.rowsTotal + 10, `rows=${beforeEnter.rowsTotal}→${i1}`);
    for (let index = 0; index < 10; index += 1) {
      await sendKey('z', 'KeyZ', 90, CTRL);
    }
    const i2 = await waitForValue(async () => {
      const tab = await activeTab();
      return tab && tab.rowsTotal === beforeEnter.rowsTotal ? tab.rowsTotal : null;
    }, 8000);
    check('I2 撤销 10 次恢复行数', i2 === beforeEnter.rowsTotal);

    // J. 奇怪字符（emoji/RTL/零宽）→ 保存 → 磁盘校验
    const weird = '😀🚀\u200bمرحبا';
    await client.send('Input.insertText', { text: weird });
    await waitForValue(async () => ((await domRow0()).startsWith(weird) ? true : null), 6000);
    await saveViaDialog();
    const j1 = await waitForValue(async () => {
      const disk = readFileSync(files.norm, 'utf8');
      return disk.includes(weird) ? true : null;
    }, 6000);
    check('J1 emoji/RTL/零宽字符往返一致', j1 === true);

    // K. 大粘贴（5000 汉字 ≈ 15KB）
    const big = '汉'.repeat(5000);
    await client.send('Input.insertText', { text: big });
    await waitForValue(async () => ((await domRow0()).includes('汉汉汉') ? true : null), 8000);
    await saveViaDialog();
    const k1 = await waitForValue(async () => {
      const disk = readFileSync(files.norm, 'utf8');
      return disk.includes(big) ? true : null;
    }, 8000);
    check('K1 5000 字大粘贴保存', k1 === true);

    // ---- L. 超长行文件：拒绝进入编辑但可阅读 ----
    check('L1 打开超长行文件（可阅读）', await openAndWait(files.longline, 'x'.repeat(8192)), '首行应为 8KB 分块');
    await click('[aria-label="切换编辑模式"]');
    const l2 = await waitForValue(async () => {
      const tab = await activeTab();
      return tab && tab.editing === false ? tab : null;
    }, 4000);
    const l3 = await waitForValue(async () => {
      const text = await evalJs(
        `[...document.querySelectorAll('.toast .text')].map((n) => n.textContent).join('|')`,
      );
      return text.includes('超长行') ? text : null;
    }, 4000);
    check('L2 超长行拒绝编辑（保持只读）', l2 !== null && l2.editing === false);
    check('L3 显示超长行提示', l3 !== null, l3 ?? '(无提示)');

    // ---- M. 脏标签关闭守卫（norm 标签仍在脏态：K 已保存，先再改脏） ----
    // 切回 norm 标签（点击其标签元素），改脏
    await evalJs(
      `(() => { const tabs = [...document.querySelectorAll('.tab')]; const target = tabs.find((t) => t.textContent.includes('norm.txt')); target?.click(); return !!target; })()`,
    );
    await delay(300);
    await client.send('Input.insertText', { text: '脏' });
    await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty ? true : null;
    }, 5000);
    // 点击关闭按钮 → 三态弹窗 → 取消 → 标签保留
    await click('[aria-label="关闭 norm.txt"]');
    const m1 = await waitForValue(
      async () =>
        (await evalJs(
          `document.querySelector('[role="alertdialog"][aria-label="关闭标签"]') !== null`,
        )) || null,
      4000,
    );
    check('M1 脏标签关闭弹出三态确认', m1 === true);
    await evalJs(
      `(() => { const dlg = document.querySelector('[role="alertdialog"][aria-label="关闭标签"]'); const btn = [...dlg.querySelectorAll('button')].find((b) => b.textContent.trim() === '取消'); btn?.click(); return !!btn; })()`,
    );
    const m2 = await waitForValue(async () => {
      const view = await invoke('list_tabs');
      return view.tabs.some((tab) => tab.name === 'norm.txt') ? true : null;
    }, 4000);
    check('M2 取消后标签保留', m2 === true);
    // 再次关闭 → 不保存 → 标签消失且磁盘未变
    await click('[aria-label="关闭 norm.txt"]');
    await waitForValue(
      async () =>
        (await evalJs(
          `document.querySelector('[role="alertdialog"][aria-label="关闭标签"]') !== null`,
        )) || null,
      4000,
    );
    await evalJs(
      `(() => { const dlg = document.querySelector('[role="alertdialog"][aria-label="关闭标签"]'); const btn = [...dlg.querySelectorAll('button')].find((b) => b.textContent.trim() === '不保存'); btn?.click(); return !!btn; })()`,
    );
    const m3 = await waitForValue(async () => {
      const view = await invoke('list_tabs');
      return view.tabs.some((tab) => tab.name === 'norm.txt') ? null : true;
    }, 5000);
    check('M3 不保存关闭标签', m3 === true);

    // ---- P. 外部修改冲突：保存弹窗 → 覆盖 ----
    check('P1 打开冲突样本', await openAndWait(files.conflict, 'conflict-base'), '首行 conflict-base');
    check('P2 进入编辑', (await toggleEdit(true))?.editing === true);
    await client.send('Input.insertText', { text: 'A' });
    await waitForValue(async () => ((await domRow0()) === 'Aconflict-base' ? true : null), 5000);
    check('P3 首次保存', await saveViaDialog());
    await waitForValue(async () => {
      const tab = await activeTab();
      return tab?.dirty === false ? true : null;
    }, 5000);
    // 外部修改（模拟另一个程序保存；此刻路径已指向我们上次另存后的文件，未被映射）
    writeFileSync(files.conflict, 'external-change\n');
    await client.send('Input.insertText', { text: 'B' });
    await waitForValue(async () => ((await domRow0()) === 'ABconflict-base' ? true : null), 5000);
    // 保存必须走「编码询问」弹窗（工具栏按钮仅打开弹窗），确认后才触发冲突
    check('P4 二次保存弹窗确认', await saveViaDialog());
    const p5 = await waitForValue(
      async () =>
        (await evalJs(
          `document.querySelector('[role="alertdialog"][aria-label="文件已在外部被修改"]') !== null`,
        )) || null,
      5000,
    );
    check('P5 冲突弹窗出现', p5 === true);
    await evalJs(
      `(() => { const dlg = document.querySelector('[role="alertdialog"][aria-label="文件已在外部被修改"]'); if (!dlg) return 'no-dialog'; const btn = [...dlg.querySelectorAll('button')].find((b) => b.textContent.trim() === '覆盖保存'); if (!btn) return 'no-button'; btn.click(); return 'clicked'; })()`,
    );
    const p6 = await waitForValue(async () => {
      const disk = readFileSync(files.conflict, 'utf8');
      const tab = await activeTab();
      return disk === 'ABconflict-base\n' && tab?.dirty === false ? disk : null;
    }, 8000);
    check('P6 覆盖保存写入内容', p6 === 'ABconflict-base\n');

    // 截图（混乱之后的状态，供人工核验）
    try {
      const shot = await client.send('Page.captureScreenshot', { format: 'png' });
      mkdirSync(dirname(screenshotPath), { recursive: true });
      writeFileSync(screenshotPath, Buffer.from(shot.data, 'base64'));
      check('S1 截图已保存', true, screenshotPath);
    } catch (error) {
      check('S1 截图已保存', false, String(error));
    }
  } finally {
    client?.close();
    if (child.pid) {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    }
    try {
      rmSync(runDataDir, { recursive: true, force: true });
    } catch {
      // 清理失败不影响结论（位于系统临时目录）
    }
  }

  const failed = checks.filter((item) => !item.passed);
  console.log(`\n对抗冒烟结果：${checks.length - failed.length}/${checks.length} 通过`);
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch((error) => {
  console.error(`对抗冒烟失败：${error?.message ?? error}`);
  process.exit(3);
});
