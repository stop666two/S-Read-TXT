// 分屏 E2E（P3-5 常驻套件）。
// 场景：
//   S1  启动单栏（无分隔条 + 欢迎页）
//   S2  右分按钮 → 两栏 + 新栏空态
//   S3  拆分上限（4 栏）与超限提示
//   S4  栏内标签组独立
//   S5  激活栏作用域（Ctrl+W 只关激活栏标签）
//   S6  关闭栏位：标签无损并入相邻栏
//   S7  分隔条拖动改比例
//   S8  快捷键拆分（Ctrl+\ 右分 / Ctrl+Shift+\ 下分）
//   S9  视图菜单可用性（拆分/关栏禁态）
//   S10 栏间拖拽迁移（标签条 → 标签条）
//   S11 拖到栏内容区边缘 → 分屏
//   S12 Esc 取消拖拽
//   S13 跨窗拖入目标栏
//   S14 重启恢复（栏位/标签/激活项/比例）
//   S15 v2 旧会话迁移为单栏
//   S16 截图归档
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  createClient,
  delay,
  dismissOnboarding,
  findTarget,
  waitForValue,
} from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-split-${Date.now()}`);
const dataDir = join(work, 'data');
const migrateDir = join(work, 'migrate');
mkdirSync(dataDir, { recursive: true });
mkdirSync(migrateDir, { recursive: true });
mkdirSync(join(root, 'docs', 'screenshots'), { recursive: true });

const files = {
  a: join(work, 'a.txt'),
  b: join(work, 'b.txt'),
  m: join(work, 'm.txt'),
};
writeFileSync(files.a, 'alpha\n'.repeat(60));
writeFileSync(files.b, 'bravo\n'.repeat(50));
writeFileSync(files.m, 'legacy\n'.repeat(30));

// v2 旧会话（顶层 tabs）—— 验证迁移后单栏恢复
writeFileSync(
  join(migrateDir, 'session.json'),
  JSON.stringify({
    schemaVersion: 2,
    focusedLabel: 'main',
    windows: [
      {
        label: 'main',
        window: { width: 1100, height: 760, maximized: false },
        activeTabIndex: 0,
        tabs: [{ path: files.m, encoding: null, scrollRow: 0, editMode: false, color: null }],
      },
    ],
  }),
);

// 端口段避开 Windows 保留区间 10008–10107，并与既有套件错开
const port = 9300 + Math.floor(Math.random() * 80);
const secondPort = port + 1;

let child = null;
let passed = 0;
let failed = 0;
let step = '启动';

const check = (name, cond, extra = '') => {
  if (cond === true) {
    passed += 1;
    console.log(`PASS  ${name}${extra ? `  → ${extra}` : ''}`);
  } else {
    failed += 1;
    console.log(`FAIL  ${name}${extra ? `  → ${extra}` : ''}`);
  }
};

const watchdog = setTimeout(() => {
  console.log(`FAIL  看门狗超时（480s，当前步骤：${step}）`);
  process.exitCode = 4;
  killAll();
}, 480_000);

const killAll = () => {
  try {
    spawnSync('taskkill', ['/IM', 's-read-txt.exe', '/T', '/F'], { stdio: 'ignore' });
  } catch {
    /* 忽略 */
  }
};
killAll();
await delay(600);

const launch = async (debugPort, dir) => {
  child = spawn(exe, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dir,
      SRT_NO_ELEVATION: '1',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
    },
    stdio: 'ignore',
  });
  await findTarget(debugPort);
  const client = await waitForValue(async () => clientByLabel(debugPort, 'main'), 25000);
  if (!client) throw new Error('主窗口未就绪（CDP get_session 未返回 main）');
  await waitForValue(
    async () => ((await evalIn(client, '!!window.__srt?.openPath')) ? true : null),
    20000,
  );
  await dismissOnboarding((expr) => evalIn(client, expr));
  return client;
};

/** 按窗口 label 取页面客户端（多窗/重启后定位主窗用） */
const clientByLabel = async (debugPort, label) => {
  let targets = [];
  try {
    targets = (await (await fetch(`http://127.0.0.1:${debugPort}/json`)).json()).filter(
      (t) => t.type === 'page' && !String(t.url).includes('drag-ghost'),
    );
  } catch {
    return null;
  }
  for (const target of targets) {
    const c = await createClient(target.webSocketDebuggerUrl);
    try {
      const got = await evalIn(
        c,
        `(async () => { try { return (await window.__TAURI_INTERNALS__.invoke('get_session')).label; } catch { return null; } })()`,
      );
      if (got === label) return c;
    } catch {
      /* 非本应用页面等 */
    }
    c.close?.();
  }
  return null;
};

const evalIn = async (client, expression) => {
  const result = await client.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result?.value;
};

const parse = async (client, command, args = {}) => {
  const raw = await evalIn(
    client,
    `(async () => { try { return JSON.stringify(await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})); } catch (e) { return 'ERR:' + JSON.stringify(e); } })()`,
  );
  if (typeof raw !== 'string' || raw.startsWith('ERR:')) throw new Error(`${command} 失败：${raw}`);
  return JSON.parse(raw);
};

const paneKeys = (client) =>
  evalIn(
    client,
    `[...document.querySelectorAll('.pane-cell[data-pane]')].map((n) => n.dataset.pane)`,
  );
const paneTabs = async (client, pane) => parse(client, 'list_tabs', { pane });

const mouse = (client, type, x, y, buttons) =>
  client.send('Input.dispatchMouseEvent', {
    type,
    x: Math.round(x),
    y: Math.round(y),
    button: 'left',
    buttons,
    clickCount: type === 'mousePressed' || type === 'mouseReleased' ? 1 : 0,
  });

const dragSteps = async (client, from, to, steps = 8) => {
  for (let i = 1; i <= steps; i += 1) {
    await mouse(
      client,
      'mouseMoved',
      from.x + ((to.x - from.x) * i) / steps,
      from.y + ((to.y - from.y) * i) / steps,
      1,
    );
    await delay(70);
  }
};

/** 标签中心坐标（按名称） */
const tabPoint = (client, pane, name) =>
  waitForValue(
    async () => {
      const point = await evalIn(
        client,
        `(() => { const n = [...document.querySelectorAll('.pane-cell[data-pane="${pane}"] .tab[data-tab-id]')].find((t) => t.textContent.includes(${JSON.stringify(name)})); if (!n) return null; const r = n.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`,
      );
      return point ?? null;
    },
    8000,
  );

/** 栏内元素中心（选择器相对 pane-cell） */
const cellPoint = (client, pane, selector, biasX = 0.5, biasY = 0.5) =>
  evalIn(
    client,
    `(() => { const n = document.querySelector('.pane-cell[data-pane="${pane}"] ${selector}'); if (!n) return null; const r = n.getBoundingClientRect(); return { x: r.left + r.width * ${biasX}, y: r.top + r.height * ${biasY}, w: r.width, h: r.height, left: r.left, right: r.right, top: r.top, bottom: r.bottom }; })()`,
  );

const keyChord = async (client, key, code, vk, modifiers) => {
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
};

try {
  // ---- S1 启动单栏 ----
  step = 'S1 启动单栏';
  let client = await launch(port, dataDir);
  const initial = await waitForValue(async () => {
    const keys = await paneKeys(client);
    return keys.length === 1 ? keys : null;
  }, 12000);
  const noSplitter = await evalIn(client, `document.querySelector('.splitter') === null`);
  const welcome = await evalIn(client, `document.querySelector('.empty') !== null`);
  check('S1 启动单栏且无分隔条', initial?.length === 1 && noSplitter === true && welcome === true, `panes=${JSON.stringify(initial)}`);

  // ---- S2 右分按钮 → 两栏 + 新栏空态 ----
  step = 'S2 右分按钮';
  await evalIn(client, `(async () => { await window.__srt.openPath(${JSON.stringify(files.a)}); return true; })()`);
  await delay(300);
  await evalIn(client, `(document.querySelector('[data-pane="main#1"] [data-pane-split-right]')?.click(), true)`);
  const twoPanes = await waitForValue(async () => {
    const keys = await paneKeys(client);
    return keys.length === 2 ? keys : null;
  }, 8000);
  const splitterVisible = await evalIn(client, `document.querySelectorAll('.splitter').length === 1`);
  const emptyHint = await evalIn(client, `document.querySelector('[data-pane="main#2"] .pane-empty') !== null`);
  check('S2 右分生成两栏与分隔条', twoPanes?.length === 2 && splitterVisible === true, `panes=${JSON.stringify(twoPanes)}`);
  check('S2b 新栏显示空态提示', emptyHint === true);
  // 栏2 打开 b.txt（后续独立性/作用域用）
  await evalIn(client, `(() => { document.querySelector('[data-pane="main#2"]')?.click(); return true; })()`);
  await evalIn(client, `(async () => { await window.__srt.openPath(${JSON.stringify(files.b)}); return true; })()`);
  await delay(300);

  // ---- S3 拆分上限 ----
  step = 'S3 拆分上限';
  await evalIn(client, `(document.querySelector('[data-pane-active="true"] [data-pane-split-right]')?.click(), true)`);
  await delay(400);
  await evalIn(client, `(document.querySelector('[data-pane-active="true"] [data-pane-split-right]')?.click(), true)`);
  await delay(400);
  const fourPanes = await paneKeys(client);
  await evalIn(client, `(document.querySelector('[data-pane-active="true"] [data-pane-split-right]')?.click(), true)`);
  await delay(400);
  const stillFour = await paneKeys(client);
  const limitToast = await waitForValue(async () => {
    const text = await evalIn(client, `[...document.querySelectorAll('.toast')].map((n) => n.textContent).join('|')`);
    return text.includes('最多支持') ? text : null;
  }, 4000);
  check('S3 连续拆到 4 栏', fourPanes.length === 4, `panes=${fourPanes.join(',')}`);
  check('S3b 第 5 栏被拒并提示', stillFour.length === 4 && limitToast !== null, `toast=${limitToast}`);
  // 收拢回 2 栏：关闭多余的空栏
  for (let i = 0; i < 2; i += 1) {
    await evalIn(client, `(() => { const keys = [...document.querySelectorAll('.pane-cell[data-pane]')].map((n) => n.dataset.pane); const key = keys[keys.length - 1]; const cell = document.querySelector('.pane-cell[data-pane="' + key + '"]'); cell?.querySelector('[data-pane-close]')?.click(); return key; })()`);
    await delay(400);
  }

  // ---- S4 栏内标签组独立 ----
  step = 'S4 栏独立';
  const p1Tabs = (await paneTabs(client, 'main#1')).tabs.map((t) => t.name);
  const p2Tabs = (await paneTabs(client, 'main#2')).tabs.map((t) => t.name);
  check('S4 两栏标签组互不可见', p1Tabs.join(',') === 'a.txt' && p2Tabs.join(',') === 'b.txt', `p1=${p1Tabs} p2=${p2Tabs}`);

  // ---- S5 激活栏作用域 ----
  step = 'S5 激活栏作用域';
  await evalIn(client, `(() => { document.querySelector('[data-pane="main#2"]')?.click(); return true; })()`);
  await delay(250);
  const activePane = await evalIn(client, `document.querySelector('.pane-cell[data-pane-active="true"]')?.dataset.pane ?? ''`);
  await keyChord(client, 'w', 'KeyW', 87, 2);
  const p2Empty = await waitForValue(async () => {
    const tabs = (await paneTabs(client, 'main#2')).tabs;
    return tabs.length === 0 ? true : null;
  }, 6000);
  const p1Safe = (await paneTabs(client, 'main#1')).tabs.map((t) => t.name);
  check('S5 点击切换激活栏', activePane === 'main#2', `active=${activePane}`);
  check('S5b Ctrl+W 仅关激活栏标签', p2Empty === true && p1Safe.join(',') === 'a.txt', `p1=${p1Safe}`);
  // 重新打开 b.txt 于栏2
  await evalIn(client, `(async () => { await window.__srt.openPath(${JSON.stringify(files.b)}); return true; })()`);
  await delay(300);

  // ---- S6 关闭栏位并入相邻栏 ----
  step = 'S6 关闭栏位';
  await evalIn(client, `(document.querySelector('[data-pane="main#2"] [data-pane-close]')?.click(), true)`);
  const afterClose = await waitForValue(async () => {
    const keys = await paneKeys(client);
    return keys.length === 1 ? keys : null;
  }, 6000);
  const merged = (await paneTabs(client, 'main#1')).tabs.map((t) => t.name).sort();
  check('S6 关栏后仅剩一栏', afterClose?.length === 1, `panes=${JSON.stringify(afterClose)}`);
  check('S6b 栏内标签并入相邻栏', merged.join(',') === 'a.txt,b.txt', `tabs=${merged}`);
  // 恢复两栏供拖拽场景使用
  await evalIn(client, `(document.querySelector('[data-pane="main#1"] [data-pane-split-right]')?.click(), true)`);
  await delay(400);
  await evalIn(client, `(() => { document.querySelector('[data-pane="main#2"]')?.click(); return true; })()`);
  await evalIn(client, `(async () => { await window.__srt.openPath(${JSON.stringify(files.b)}); return true; })()`);
  await delay(300);

  // ---- S7 分隔条拖动 ----
  step = 'S7 分隔条拖动';
  const before = await evalIn(client, `(() => { const a = document.querySelector('[data-pane="main#1"]').getBoundingClientRect().width; const b = document.querySelector('[data-pane="main#2"]').getBoundingClientRect().width; return { a, b }; })()`);
  const splitter = await evalIn(client, `(() => { const s = document.querySelector('.splitter'); const r = s.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
  await mouse(client, 'mousePressed', splitter.x, splitter.y, 1);
  await dragSteps(client, { x: splitter.x, y: splitter.y }, { x: splitter.x + 110, y: splitter.y }, 6);
  await mouse(client, 'mouseReleased', splitter.x + 110, splitter.y, 0);
  await delay(400);
  const after = await evalIn(client, `(() => { const a = document.querySelector('[data-pane="main#1"]').getBoundingClientRect().width; const b = document.querySelector('[data-pane="main#2"]').getBoundingClientRect().width; return { a, b }; })()`);
  check('S7 分隔条拖动改变比例', after.a - after.b > before.a - before.b + 60, `before=${Math.round(before.a)}/${Math.round(before.b)} after=${Math.round(after.a)}/${Math.round(after.b)}`);

  // ---- S8 快捷键拆分 ----
  step = 'S8 快捷键拆分';
  const panesBeforeShortcut = (await paneKeys(client)).length;
  await keyChord(client, '\\', 'Backslash', 220, 2);
  const afterRight = await waitForValue(async () => {
    const keys = await paneKeys(client);
    return keys.length === panesBeforeShortcut + 1 ? keys : null;
  }, 6000);
  const dirRow = await evalIn(client, `document.querySelector('.pane-split')?.classList.contains('row') === true`);
  check('S8 Ctrl+\\ 右分生成新栏', afterRight !== null, `panes=${JSON.stringify(afterRight)}`);
  check('S8b 右分为水平方向', dirRow === true);
  await keyChord(client, '\\', 'Backslash', 220, 10);
  const afterDown = await waitForValue(async () => {
    const keys = await paneKeys(client);
    return keys.length === panesBeforeShortcut + 2 ? keys : null;
  }, 6000);
  const dirColumn = await evalIn(client, `[...document.querySelectorAll('.pane-split')].some((n) => n.classList.contains('column'))`);
  check('S8c Ctrl+Shift+\\ 下分生成新栏', afterDown !== null, `panes=${JSON.stringify(afterDown)}`);
  check('S8d 下分为垂直方向', dirColumn === true);
  // 收拢到 2 栏
  for (let i = 0; i < 2; i += 1) {
    await evalIn(client, `(() => { const keys = [...document.querySelectorAll('.pane-cell[data-pane]')].map((n) => n.dataset.pane); const key = keys[keys.length - 1]; const cell = document.querySelector('.pane-cell[data-pane="' + key + '"]'); cell?.querySelector('[data-pane-close]')?.click(); return key; })()`);
    await delay(350);
  }

  // ---- S9 视图菜单可用性 ----
  step = 'S9 视图菜单';
  const menuState = await waitForValue(async () => {
    await evalIn(client, `(document.body.click(), true)`);
    await delay(80);
    await evalIn(client, `(() => { const t = [...document.querySelectorAll('.menu-bar .title')].find((n) => n.textContent.trim() === '查看'); t?.click(); return !!t; })()`);
    const state = await evalIn(
      client,
      `(() => { const items = [...document.querySelectorAll('.menu-bar .item')]; const find = (s) => items.find((n) => n.textContent.includes(s)); if (!find('向右拆分') && !find('关闭栏位')) return null; return { split: find('向右拆分') ? !find('向右拆分').disabled : null, close: find('关闭栏位') ? !find('关闭栏位').disabled : null }; })()`,
    );
    await evalIn(client, `(document.body.click(), true)`);
    return state;
  }, 4000);
  check('S9 多栏时菜单拆分/关栏可用', menuState?.split === true && menuState?.close === true, JSON.stringify(menuState));

  // ---- S10 栏间拖拽迁移 ----
  step = 'S10 栏间拖拽';
  const fromBar = await tabPoint(client, 'main#1', 'a.txt');
  const toBar = await cellPoint(client, 'main#2', '.tab-bar', 0.4, 0.5);
  await mouse(client, 'mousePressed', fromBar.x, fromBar.y, 1);
  await dragSteps(client, fromBar, { x: toBar.x, y: toBar.y });
  const p2DuringHover = await evalIn(client, `document.querySelector('.pane-cell[data-pane="main#2"]')?.classList.contains('drop-target') ?? null`);
  await mouse(client, 'mouseReleased', toBar.x, toBar.y, 0);
  const moved = await waitForValue(async () => {
    const tabs = (await paneTabs(client, 'main#2')).tabs.map((t) => t.name);
    return tabs.includes('a.txt') ? tabs : null;
  }, 6000);
  const p1Left = (await paneTabs(client, 'main#1')).tabs.map((t) => t.name);
  check('S10 拖到另一栏标签条完成迁移', moved !== null && !p1Left.includes('a.txt'), `p2=${moved} p1=${p1Left}`);
  void p2DuringHover;

  // ---- S11 拖到内容区边缘分屏 ----
  step = 'S11 边缘分屏';
  const panesBeforeEdge = (await paneKeys(client)).length;
  const fromBar2 = await tabPoint(client, 'main#2', 'a.txt');
  const reader1 = await cellPoint(client, 'main#1', '.reader', 1, 0.5);
  const edgeTarget = { x: reader1.right - 30, y: reader1.y };
  await mouse(client, 'mousePressed', fromBar2.x, fromBar2.y, 1);
  await dragSteps(client, fromBar2, edgeTarget);
  await mouse(client, 'mouseReleased', edgeTarget.x, edgeTarget.y, 0);
  const edgeSplit = await waitForValue(async () => {
    const keys = await paneKeys(client);
    return keys.length === panesBeforeEdge + 1 ? keys : null;
  }, 6000);
  let edgeHasTab = false;
  if (edgeSplit) {
    for (const key of edgeSplit) {
      const tabs = (await paneTabs(client, key)).tabs.map((t) => t.name);
      if (tabs.includes('a.txt') && key !== 'main#1') edgeHasTab = true;
    }
  }
  check('S11 拖到内容区右缘生成新栏', edgeSplit !== null, `panes=${JSON.stringify(edgeSplit)}`);
  check('S11b 新栏包含被拖标签', edgeHasTab === true);
  // 将 a.txt 拖回 main#1 便于后续场景
  const backSource = edgeSplit?.find((key) => key !== 'main#1');
  if (backSource) {
    const from = await tabPoint(client, backSource, 'a.txt');
    const back = await cellPoint(client, 'main#1', '.tab-bar', 0.5, 0.5);
    await mouse(client, 'mousePressed', from.x, from.y, 1);
    await dragSteps(client, from, { x: back.x, y: back.y });
    await mouse(client, 'mouseReleased', back.x, back.y, 0);
    await delay(500);
    await evalIn(client, `(document.querySelector('.pane-cell[data-pane="${backSource}"] [data-pane-close]')?.click(), true)`);
    await delay(400);
  }

  // ---- S12 Esc 取消 ----
  step = 'S12 Esc 取消';
  const panesBeforeEsc = (await paneKeys(client)).length;
  const escFrom = await tabPoint(client, 'main#1', 'a.txt');
  const escTo = await cellPoint(client, 'main#2', '.reader', 0.9, 0.5);
  await mouse(client, 'mousePressed', escFrom.x, escFrom.y, 1);
  await dragSteps(client, escFrom, { x: escTo.x, y: escTo.y });
  await client.send('Input.dispatchKeyEvent', {
    type: 'rawKeyDown',
    key: 'Escape',
    code: 'Escape',
    windowsVirtualKeyCode: 27,
    nativeVirtualKeyCode: 27,
  });
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    key: 'Escape',
    code: 'Escape',
    windowsVirtualKeyCode: 27,
    nativeVirtualKeyCode: 27,
  });
  await mouse(client, 'mouseReleased', escTo.x, escTo.y, 0);
  await delay(500);
  const panesAfterEsc = (await paneKeys(client)).length;
  const ownerAfterEsc = (await paneTabs(client, 'main#1')).tabs.map((t) => t.name);
  check('S12 Esc 取消不改布局且标签保留', panesAfterEsc === panesBeforeEsc && ownerAfterEsc.includes('a.txt'), `panes=${panesAfterEsc}`);

  // ---- S13 跨窗拖入目标栏 ----
  step = 'S13 跨窗拖入';
  await evalIn(client, `(async () => { await window.__TAURI_INTERNALS__.invoke('new_window', {}); return true; })()`);
  const second = await waitForValue(async () => clientByLabel(port, 'main-2'), 15000);
  let crossOk = null;
  if (second) {
    await dismissOnboarding((expr) => evalIn(second, expr));
    await parse(client, 'plugin:window|set_position', { label: 'main', value: { Physical: { x: 100, y: 100 } } });
    await parse(second, 'plugin:window|set_position', { label: 'main-2', value: { Physical: { x: 1300, y: 150 } } });
    await delay(400);
    const fromX = await tabPoint(client, 'main#1', 'a.txt');
    const mainGeo = await evalIn(client, `({ sx: window.screenX, sy: window.screenY })`);
    const win2Geo = await evalIn(second, `({ sx: window.screenX, sy: window.screenY })`);
    const win2Bar = await evalIn(second, `(() => { const bar = document.querySelector('.tab-bar'); const r = bar.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
    const outside = { x: win2Geo.sx + win2Bar.x - mainGeo.sx, y: win2Geo.sy + win2Bar.y - mainGeo.sy };
    await mouse(client, 'mousePressed', fromX.x, fromX.y, 1);
    await dragSteps(client, fromX, outside, 10);
    await mouse(client, 'mouseMoved', outside.x, outside.y, 1);
    await delay(300);
    await mouse(client, 'mouseReleased', outside.x, outside.y, 0);
    crossOk = await waitForValue(async () => {
      const raw = await evalIn(second, `(async () => { try { const v = await window.__TAURI_INTERNALS__.invoke('list_tabs'); return JSON.stringify(v.tabs.map((t) => t.name)); } catch { return null; } })()`);
      return typeof raw === 'string' && raw.includes('a.txt') ? raw : null;
    }, 8000);
  }
  check('S13 跨窗拖入目标栏标签条', typeof crossOk === 'string' && crossOk.includes('a.txt'), String(crossOk));

  // ---- S14 重启恢复 ----
  step = 'S14 重启恢复';
  // 收拢到恰好两栏（异常路径防御），再按当前实际状态取样对比（不预设标签集合）
  for (let guard = 0; guard < 4; guard += 1) {
    const keys = await paneKeys(client);
    if (keys.length <= 2) break;
    await evalIn(client, `(() => { const keys = [...document.querySelectorAll('.pane-cell[data-pane]')].map((n) => n.dataset.pane); const key = keys[keys.length - 1]; document.querySelector('.pane-cell[data-pane="' + key + '"] [data-pane-close]')?.click(); return key; })()`);
    await delay(400);
  }
  // 确保两栏都有内容（栏1 打开 a；栏2 至少保留已有标签）
  await evalIn(client, `(() => { document.querySelector('[data-pane="main#1"]')?.click(); return true; })()`);
  await waitForValue(async () => {
    const keys = await paneKeys(client);
    return keys.includes('main#1') && keys.includes('main#2') ? true : null;
  }, 5000);
  await evalIn(client, `(async () => { await window.__srt.openPath(${JSON.stringify(files.a)}); return true; })()`);
  await delay(400);
  await evalIn(client, `(() => { document.querySelector('[data-pane="main#2"]')?.click(); return true; })()`);
  await delay(250);
  const keysBefore = await paneKeys(client);
  const stateBefore = {};
  for (const key of keysBefore) {
    const view = await paneTabs(client, key);
    stateBefore[key] = {
      names: view.tabs.map((t) => t.name),
      activeName: view.tabs.find((t) => t.tabId === view.activeTabId)?.name ?? null,
    };
  }
  const ratioBefore = await evalIn(client, `(() => { const a = document.querySelector('[data-pane="main#1"]').getBoundingClientRect().width; const b = document.querySelector('[data-pane="main#2"]').getBoundingClientRect().width; return { a, b }; })()`);
  await delay(3500);
  killAll();
  await delay(1200);

  const client2 = await launch(secondPort, dataDir);
  const restarted = await waitForValue(async () => {
    const keys = await paneKeys(client2);
    return keys.length === keysBefore.length ? keys : null;
  }, 15000);
  let stateAfter = null;
  if (restarted) {
    stateAfter = {};
    for (const key of restarted) {
      const view = await paneTabs(client2, key);
      stateAfter[key] = {
        names: view.tabs.map((t) => t.name),
        activeName: view.tabs.find((t) => t.tabId === view.activeTabId)?.name ?? null,
      };
    }
  }
  const ratioAfter = restarted
    ? await evalIn(client2, `(() => { const a = document.querySelector('[data-pane="main#1"]').getBoundingClientRect().width; const b = document.querySelector('[data-pane="main#2"]').getBoundingClientRect().width; return { a, b }; })()`)
    : null;
  const stateEqual =
    stateAfter !== null &&
    JSON.stringify(Object.fromEntries(Object.entries(stateBefore).sort())) ===
      JSON.stringify(Object.fromEntries(Object.entries(stateAfter).sort()));
  check('S14 重启恢复两栏', restarted?.length === keysBefore.length, `panes=${JSON.stringify(restarted)}`);
  check('S14b 每栏标签与激活项一致', stateEqual, `before=${JSON.stringify(stateBefore)} after=${JSON.stringify(stateAfter)}`);
  check(
    'S14c 分隔比例恢复（±60px）',
    ratioAfter !== null && Math.abs(ratioAfter.a - ratioBefore.a) <= 60,
    `before=${Math.round(ratioBefore.a)} after=${ratioAfter ? Math.round(ratioAfter.a) : 'n/a'}`,
  );

  // ---- S16 截图（在会话仍存活时截取两栏布局） ----
  step = 'S16 截图';
  const shotPath = join(root, 'docs', 'screenshots', 'phase-p35-split.png');
  let shotSaved = false;
  try {
    const shotRes = await client2.send('Page.captureScreenshot', { format: 'png' });
    if (typeof shotRes?.data === 'string' && shotRes.data.length > 100) {
      writeFileSync(shotPath, Buffer.from(shotRes.data, 'base64'));
      shotSaved = true;
    }
  } catch {
    /* 截图失败按断言处理 */
  }

  // ---- S15 v2 迁移 ----
  step = 'S15 v2 迁移';
  killAll();
  await delay(1200);
  const client3 = await launch(port, migrateDir);
  const migrated = await waitForValue(async () => {
    const keys = await paneKeys(client3);
    const names = (await paneTabs(client3, 'main#1')).tabs.map((t) => t.name);
    return keys.length === 1 && names.includes('m.txt') ? names : null;
  }, 12000);
  check('S15 v2 旧会话迁移为单栏恢复', Array.isArray(migrated), JSON.stringify(migrated));

  check('S16 分屏截图已保存', shotSaved && existsSync(shotPath), shotPath);

  console.log(`\n分屏冒烟：${passed}/${passed + failed} 通过`);
  process.exitCode = failed === 0 ? 0 : 1;
} catch (error) {
  console.error('冒烟异常：', error);
  process.exitCode = 2;
} finally {
  clearTimeout(watchdog);
  killAll();
  await delay(800);
  if (failed === 0) {
    rmSync(work, { recursive: true, force: true });
  } else {
    console.log(`失败：保留工作目录供诊断 → ${work}`);
  }
}
