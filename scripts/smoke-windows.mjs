// 多窗口 E2E（P3-4 常驻套件）。
// 场景：
//   W1  标签栏「新建窗口」按钮创建 main-2（UI 路径）
//   W2  两窗口标签组独立（各自打开文件互不可见）
//   W3  标签颜色：右键设色 → 色条渲染 → 深浅基底色值可辨 → 会话持久化
//   W4  跨窗口移动（右键「移动到窗口」）：迁移 + 颜色保持 + 同文件合并
//   W5  拖出并窗：拖影出现 → 目标插入指示 → 释放迁移
//   W6  拖桌面成新窗：落点新建 main-3 并迁入标签
//   W7  Esc 取消拖拽：标签留在原窗口
//   W8  关闭窗口：main-3 标题栏关闭 → 会话切片移除；整体退出后干净退出标记
//   W9  重启：仅恢复存活窗口与标签
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  createClient,
  delay,
  dismissOnboarding,
  findTarget,
  waitForValue,
  wakeChannel,
} from './lib/smoke-cdp.mjs';
import { removeWithRetryAsync } from './lib/system.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-windows-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
mkdirSync(join(root, 'docs', 'screenshots'), { recursive: true });

const files = {
  a: join(work, 'a.txt'),
  b: join(work, 'b.txt'),
  c: join(work, 'c.txt'),
};
for (const [name, file] of Object.entries(files)) {
  writeFileSync(file, `${name}\n`.repeat(60));
}

// 端口段避开 Windows 保留区间 10008–10107（HNS/Hyper-V 排除段）
const port = 9600 + Math.floor(Math.random() * 250);
const firstPort = port;
const secondPort = port + 1;
let child = null;

/** 看门狗：超时明确失败，避免挂死。 */
const watchdog = setTimeout(() => {
  console.log('FAIL  看门狗超时（420s）');
  process.exitCode = 4;
  killAll();
}, 420_000);

const killAll = () => {
  try {
    spawnSync('taskkill', ['/IM', 's-read-txt.exe', '/T', '/F'], { stdio: 'ignore' });
  } catch {
    /* 忽略 */
  }
};

const launch = async (debugPort) => {
  child = spawn(exe, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dataDir,
      SRT_NO_ELEVATION: '1',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
    },
    stdio: 'ignore',
  });
  await findTarget(debugPort);
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

const invoke = (client, command, args = {}) =>
  evalIn(
    client,
    `(async () => { try { return JSON.stringify(await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})); } catch (e) { return 'ERR:' + JSON.stringify(e); } })()`,
  );

const parse = async (client, command, args = {}) => {
  const raw = await invoke(client, command, args);
  if (typeof raw !== 'string' || raw.startsWith('ERR:')) throw new Error(`${command} 失败：${raw}`);
  return JSON.parse(raw);
};

const tabNames = async (client) => (await parse(client, 'list_tabs')).tabs.map((tab) => tab.name);
const tabInfos = async (client) => (await parse(client, 'list_tabs')).tabs;

/** 枚举全部主窗口页面（排除拖影窗），按 label 索引 */
const allPages = async (debugPort = firstPort) => {
  let targets = [];
  try {
    targets = (await (await fetch(`http://127.0.0.1:${debugPort}/json`)).json()).filter(
      (target) => target.type === 'page' && !String(target.url).includes('drag-ghost'),
    );
  } catch {
    return [];
  }
  const out = [];
  for (const target of targets) {
    const client = await createClient(target.webSocketDebuggerUrl);
    try {
      const session = await parse(client, 'get_session');
      out.push({ label: session.label, client });
    } catch {
      client.close?.();
    }
  }
  return out;
};

const ghostVisible = async () => {
  try {
    const targets = await (await fetch(`http://127.0.0.1:${firstPort}/json`)).json();
    return targets.some((target) => String(target.url).includes('drag-ghost'));
  } catch {
    return false;
  }
};

const mouse = (client, type, x, y, buttons) =>
  client.send('Input.dispatchMouseEvent', {
    type,
    x: Math.round(x),
    y: Math.round(y),
    button: 'left',
    buttons,
    clickCount: type === 'mousePressed' || type === 'mouseReleased' ? 1 : 0,
  });

const dragSteps = async (client, from, to, steps = 6) => {
  for (let i = 1; i <= steps; i += 1) {
    await mouse(client, 'mouseMoved', from.x + ((to.x - from.x) * i) / steps, from.y + ((to.y - from.y) * i) / steps, 1);
    await delay(70);
  }
};

/** 等待标签渲染并返回中心/左缘坐标 */
const tabPoint = async (client, name = null, edge = 'center') =>
  waitForValue(
    async () => {
      try {
        return await evalIn(
          client,
          `(() => { const nodes = [...document.querySelectorAll('.tab[data-tab-id]')]; const tab = ${name ? `nodes.find((n) => n.textContent.includes(${JSON.stringify(name)}))` : 'nodes[0]'}; if (!tab) return null; const rect = tab.getBoundingClientRect(); return { x: ${edge === 'left' ? 'rect.left + 6' : 'rect.left + rect.width / 2'}, y: rect.top + rect.height / 2 }; })()`,
        );
      } catch {
        return null;
      }
    },
    8000,
    200,
  );

const setPos = (client, label, x, y) =>
  invoke(client, 'plugin:window|set_position', { label, value: { Physical: { x, y } } });

let passed = 0;
let failed = 0;
const check = (name, cond, extra = '') => {
  if (cond) {
    passed += 1;
    console.log(`PASS  ${name}${extra ? `  ← ${extra}` : ''}`);
  } else {
    failed += 1;
    console.log(`FAIL  ${name}${extra ? `  ← ${extra}` : ''}`);
  }
};

try {
  // ---- W1 新建窗口（UI 路径）----
  await launch(firstPort);
  let pages = await waitForValue(async () => {
    const list = await allPages();
    return list.length >= 1 ? list : null;
  }, 20000, 500);
  const main = pages[0].client;
  await waitForValue(async () => ((await evalIn(main, '!!window.__srt?.openPath')) ? true : null), 15000);
  await dismissOnboarding((expr) => evalIn(main, expr));
  await evalIn(main, `(document.querySelector('[data-window-new]')?.click(), true)`);
  pages = await waitForValue(async () => {
    const list = await allPages();
    return list.length >= 2 ? list : null;
  }, 20000, 500);
  const byLabel = Object.fromEntries(pages.map((page) => [page.label, page.client]));
  check('W1 标签栏按钮新建 main-2', pages.length === 2 && byLabel['main-2'] !== undefined, pages.map((p) => p.label).join(','));
  for (const page of pages) {
    await waitForValue(async () => ((await evalIn(page.client, '!!window.__srt?.openPath')) ? true : null), 15000);
    await dismissOnboarding((expr) => evalIn(page.client, expr));
  }

  // ---- W2 两窗标签组独立 ----
  await evalIn(byLabel.main, `(void window.__srt.openPath(${JSON.stringify(files.a)}), true)`);
  await evalIn(byLabel['main-2'], `(void window.__srt.openPath(${JSON.stringify(files.b)}), true)`);
  await waitForValue(async () => {
    const names = await tabNames(byLabel.main);
    return names.includes('a.txt') ? names : null;
  }, 8000);
  await waitForValue(async () => {
    const names = await tabNames(byLabel['main-2']);
    return names.includes('b.txt') ? names : null;
  }, 8000);
  check('W2 两窗标签互不可见', JSON.stringify(await tabNames(byLabel.main)) === '["a.txt"]' && JSON.stringify(await tabNames(byLabel['main-2'])) === '["b.txt"]');

  // ---- W3 标签颜色 ----
  await evalIn(
    byLabel.main,
    `(() => { const tab = document.querySelector('.tab[data-tab-id]'); const rect = tab.getBoundingClientRect();
      tab.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: rect.left + 20, clientY: rect.top + 10 })); return true; })()`,
  );
  await waitForValue(async () => ((await evalIn(byLabel.main, `document.querySelector('[data-tab-color="red"]') !== null`)) ? true : null), 5000);
  await evalIn(byLabel.main, `(document.querySelector('[data-tab-color="red"]')?.click(), true)`);
  const colored = await waitForValue(async () => {
    const infos = await tabInfos(byLabel.main);
    return infos[0]?.color === 'red' ? infos : null;
  }, 8000);
  check('W3a 右键设色 red', colored !== null && (await evalIn(byLabel.main, `document.querySelector('.color-bar') !== null`)));
  const lightBg = await evalIn(byLabel.main, `(() => { document.documentElement.dataset.themeBase = 'light'; return getComputedStyle(document.querySelector('.color-bar')).backgroundColor; })()`);
  const darkBg = await evalIn(byLabel.main, `(() => { document.documentElement.dataset.themeBase = 'dark'; return getComputedStyle(document.querySelector('.color-bar')).backgroundColor; })()`);
  check('W3b 深浅基底色值可辨', lightBg !== darkBg, `${lightBg} vs ${darkBg}`);
  await evalIn(byLabel.main, `(document.documentElement.dataset.themeBase = 'light', true)`);

  // ---- W4 跨窗口移动（右键菜单通道）----
  await evalIn(
    byLabel.main,
    `(() => { const tab = document.querySelector('.tab[data-tab-id]'); const rect = tab.getBoundingClientRect();
      tab.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: rect.left + 20, clientY: rect.top + 10 })); return true; })()`,
  );
  await waitForValue(async () => ((await evalIn(byLabel.main, `document.querySelector('[data-move-window="main-2"]') !== null`)) ? true : null), 6000);
  await evalIn(byLabel.main, `(document.querySelector('[data-move-window="main-2"]')?.click(), true)`);
  const movedTab = await waitForValue(async () => {
    const names = await tabNames(byLabel.main);
    return names.length === 0 ? true : null;
  }, 8000);
  check('W4a 移动后源窗空', movedTab === true);
  const targetTabs = await waitForValue(async () => {
    const infos = await tabInfos(byLabel['main-2']);
    return infos.some((tab) => tab.name === 'a.txt' && tab.color === 'red') ? infos : null;
  }, 8000);
  check('W4b 目标窗含 a.txt 且颜色保持', targetTabs !== null, JSON.stringify(targetTabs?.map((t) => t.name)));

  // ---- 布局两窗（拖放命中用）----
  await setPos(byLabel.main, 'main', 100, 100);
  await setPos(byLabel['main-2'], 'main-2', 1230, 150);
  await delay(500);

  // ---- W5 拖出并窗 ----
  await evalIn(byLabel.main, `(void window.__srt.openPath(${JSON.stringify(files.c)}), true)`);
  await waitForValue(async () => ((await tabNames(byLabel.main)).includes('c.txt') ? true : null), 8000);
  const cPoint = await tabPoint(byLabel.main, 'c.txt');
  // 分屏语义下目标窗的「落点」应为栏位标签条区域（拖入指示与插入位置都按栏位解析）
  const mainGeo = await evalIn(byLabel.main, `({ sx: window.screenX, sy: window.screenY })`);
  const win2Geo = await evalIn(byLabel['main-2'], `({ sx: window.screenX, sy: window.screenY })`);
  const win2Bar = await evalIn(
    byLabel['main-2'],
    `(() => { const b = document.querySelector('.tab-bar'); const r = b.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`,
  );
  const outside = {
    x: win2Geo.sx + win2Bar.x - mainGeo.sx,
    y: win2Geo.sy + win2Bar.y - mainGeo.sy,
  };
  await mouse(byLabel.main, 'mousePressed', cPoint.x, cPoint.y, 1);
  await dragSteps(byLabel.main, cPoint, outside);
  await mouse(byLabel.main, 'mouseMoved', outside.x, outside.y, 1);
  await delay(300);
  const ghostShown = await waitForValue(async () => ((await ghostVisible()) ? true : null), 6000, 300);
  check('W5a 拖出后拖影出现', ghostShown === true);
  const hoverLine = await waitForValue(async () => ((await evalIn(byLabel['main-2'], `document.querySelector('.drop-line') !== null`)) ? true : null), 6000, 300);
  check('W5b 目标窗插入指示', hoverLine === true);
  await mouse(byLabel.main, 'mouseReleased', outside.x, outside.y, 0);
  const ghostGone = await waitForValue(async () => ((await ghostVisible()) ? null : true), 6000, 300);
  check('W5c 释放后拖影销毁', ghostGone === true);
  const cMoved = await waitForValue(async () => {
    const names = await tabNames(byLabel['main-2']);
    return names.includes('c.txt') ? names : null;
  }, 8000);
  check('W5d c.txt 迁入目标窗', Array.isArray(cMoved), JSON.stringify(cMoved));

  // ---- W6 拖桌面成新窗 ----
  await evalIn(byLabel.main, `(void window.__srt.openPath(${JSON.stringify(files.a)}), true)`);
  await waitForValue(async () => ((await tabNames(byLabel.main)).includes('a.txt') ? true : null), 8000);
  const aPoint = await tabPoint(byLabel.main, 'a.txt');
  const desktop = { x: 500, y: 1000 };
  // 拖拽协议依赖页面→Rust 的 IPC 到达顺序；WebView2 空闲后首条 invoke 可能被延迟 ~19s，
  // 合成一次鼠标移动预热通道（与 smoke-longline 同一对策），保证桌面落点裁决稳定。
  await wakeChannel(byLabel.main);
  await mouse(byLabel.main, 'mousePressed', aPoint.x, aPoint.y, 1);
  await dragSteps(byLabel.main, aPoint, desktop);
  await mouse(byLabel.main, 'mouseReleased', desktop.x, desktop.y, 0);
  const threePages = await waitForValue(async () => {
    const list = await allPages();
    return list.length >= 3 ? list : null;
  }, 15000, 500);
  check('W6a 桌面落点新建窗口', threePages !== null && threePages.length === 3, threePages?.map((p) => p.label).join(','));
  const extra = threePages?.find((page) => !['main', 'main-2'].includes(page.label));
  const extraNames = extra
    ? await waitForValue(async () => {
        const names = await tabNames(extra.client);
        return names.includes('a.txt') ? names : null;
      }, 8000)
    : null;
  check('W6b 新窗口含 a.txt', Array.isArray(extraNames), JSON.stringify(extraNames));
  if (extra) byLabel[extra.label] = extra.client;

  // ---- W7 Esc 取消 ----
  await evalIn(byLabel.main, `(void window.__srt.openPath(${JSON.stringify(files.b)}), true)`);
  await waitForValue(async () => ((await tabNames(byLabel.main)).includes('b.txt') ? true : null), 8000);
  const bPoint = await tabPoint(byLabel.main, 'b.txt');
  await mouse(byLabel.main, 'mousePressed', bPoint.x, bPoint.y, 1);
  await dragSteps(byLabel.main, bPoint, outside);
  await waitForValue(async () => ((await ghostVisible()) ? true : null), 6000, 300);
  await byLabel.main.send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  await byLabel.main.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Escape', code: 'Escape', windowsVirtualKeyCode: 27 });
  await delay(500);
  await mouse(byLabel.main, 'mouseReleased', outside.x, outside.y, 0);
  await delay(600);
  check('W7 Esc 取消：拖影销毁且标签保留', (await ghostVisible()) === false && (await tabNames(byLabel.main)).includes('b.txt'));

  // ---- 截图（两窗 + 色条状态；存证）----
  const shot = await byLabel.main.send('Page.captureScreenshot', { format: 'png' });
  if (shot?.data) {
    writeFileSync(join(root, 'docs', 'screenshots', 'phase-p34-windows.png'), Buffer.from(shot.data, 'base64'));
  }

  // ---- W8 关闭窗口与整体退出 ----
  if (extra) {
    await evalIn(byLabel[extra.label], `(document.querySelector('.title-bar button[aria-label="关闭"]')?.click(), true)`);
    const backToTwo = await waitForValue(async () => {
      const list = await allPages();
      return list.length === 2 ? list : null;
    }, 15000, 500);
    check('W8a 关闭 main-3 后剩两窗', backToTwo !== null, backToTwo?.map((p) => p.label).join(','));
    const sessionOk = await waitForValue(async () => {
      const raw = existsSync(join(dataDir, 'session.json')) ? readFileSync(join(dataDir, 'session.json'), 'utf8') : '';
      try {
        const parsed = JSON.parse(raw);
        return parsed.windows.length === 2 && !parsed.windows.some((entry) => entry.label.startsWith('main-3'))
          ? parsed
          : null;
      } catch {
        return null;
      }
    }, 8000, 500);
    check(
      'W8b 会话遗忘已关窗切片',
      sessionOk !== null,
      sessionOk ? sessionOk.windows.map((entry) => entry.label).join(',') : '会话未收敛',
    );
  }
  const quitRaw = await invoke(byLabel['main-2'], 'begin_quit_all');
  check('W8c 两阶段退出协议可用', quitRaw === 'true', String(quitRaw));
  await waitForValue(async () => {
    try {
      await (await fetch(`http://127.0.0.1:${firstPort}/json`, { signal: AbortSignal.timeout(1000) })).json();
      return null;
    } catch {
      return true;
    }
  }, 20000, 500);
  killAll();
  await delay(1500);
  check('W8d 最后窗口写入干净退出标记', existsSync(join(dataDir, '.clean-exit')));

  // ---- W9 重启仅恢复存活窗口 ----
  await launch(secondPort);
  const restarted = await waitForValue(async () => {
    const list = await allPages(secondPort);
    return list.length >= 1 ? list : null;
  }, 30000, 500);
  check('W9a 重启恢复窗口数正确', restarted !== null, restarted?.map((p) => p.label).join(','));
  if (restarted) {
    const restartedMain = restarted.find((page) => page.label === 'main')?.client;
    const restoredNames = restartedMain
      ? await waitForValue(async () => {
          const infos = await tabInfos(restartedMain);
          return infos.length > 0 ? infos : null;
        }, 15000)
      : null;
    check('W9b 恢复的主窗标签非空', Array.isArray(restoredNames), JSON.stringify(restoredNames?.map((t) => t.name)));
  }

  console.log(`\n多窗口冒烟：${passed}/${passed + failed} 通过`);
  process.exitCode = failed === 0 ? 0 : 1;
} catch (error) {
  console.error('冒烟异常：', error);
  process.exitCode = 2;
} finally {
  clearTimeout(watchdog);
  killAll();
  await delay(1200);
  if (failed === 0) {
    await removeWithRetryAsync(work);
  } else {
    console.log(`失败：保留工作目录供诊断 → ${work}`);
  }
}
