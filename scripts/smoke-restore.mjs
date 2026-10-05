// 会话恢复内容冒烟（P3-7）：
// 设置项存在性与总开关/细项语义——折叠锚点（含失效丢弃）、光标恢复与越界裁剪、
// 滚动关闭、布局关闭合并单栏、总开关关闭仍恢复窗口数量与几何。
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createClient, delay, dismissOnboarding, findTarget, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-restore-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });

const results = [];
let step = '启动';
const check = (name, cond, extra = '') => {
  results.push(cond === true);
  console.log(`${cond === true ? 'PASS' : 'FAIL'}  ${name}${extra ? `  → ${extra}` : ''}`);
};

const writeFile = (name, text) => {
  const path = join(work, name);
  writeFileSync(path, text);
  return path;
};
const foldText = ['root', '  child 1', '  child 2', 'root2', '  child 3'].join('\n') + '\n';
const fileFold = writeFile('fold.txt', foldText);
const fileCaret = writeFile('caret.txt', Array.from({ length: 80 }, (_, i) => `line ${i + 1}`).join('\n') + '\n');
const fileScroll = writeFile('scroll.txt', Array.from({ length: 200 }, (_, i) => `s ${i + 1}`).join('\n') + '\n');
const fileOther = writeFile('other.txt', 'other 1\nother 2\n');

let port = 9740 + Math.floor(Math.random() * 160);
let child = null;

const listProcs = () =>
  String(spawnSync('tasklist', ['/FI', 'IMAGENAME eq s-read-txt.exe', '/NH'], { encoding: 'utf8' }).stdout);
const hasApp = () => listProcs().includes('s-read-txt.exe');
const forceKillAll = () => {
  spawnSync('taskkill', ['/IM', 's-read-txt.exe', '/T', '/F'], { stdio: 'ignore' });
};
const waitNoProc = async (timeout = 15000) => {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (!hasApp()) return true;
    await delay(250);
  }
  return !hasApp();
};
/** 优雅退出：多窗口走退出协调，单窗口走标题栏关闭（避免强杀导致 WebView2 数据目录锁残留）。 */
const quitGracefully = async () => {
  if (!main) return;
  try {
    const count = await invoke(main, 'main_window_count');
    if (count > 1) {
      await invoke(main, 'begin_quit_all');
    } else {
      await evalIn(main, `(document.querySelector('.title-bar button[aria-label="关闭"]')?.click(), true)`);
    }
  } catch {
    // 退出过程连接断开属正常
  }
  await waitNoProc(12000);
};
const killApp = async () => {
  await quitGracefully();
  if (hasApp() && child) {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  }
  if (hasApp()) forceKillAll();
  await waitNoProc();
  child = null;
  // WebView2 数据目录锁释放余量（避免下一实例浏览器进程创建失败）
  await delay(1200);
};
const watchdog = setTimeout(() => {
  console.log(`FAIL  看门狗超时 @${step}`);
  process.exitCode = 4;
  void killApp();
}, 900_000);
process.on('exit', () => {
  spawnSync('taskkill', ['/IM', 's-read-txt.exe', '/T', '/F'], { stdio: 'ignore' });
});

let main = null;
let settings = null;

const evalIn = async (client, expression) => {
  const result = await client.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result?.value;
};
const invoke = async (client, command, args = {}) => {
  const raw = await evalIn(
    client,
    `(async () => { try { return JSON.stringify(await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})); } catch (e) { return 'ERR:' + JSON.stringify(e); } })()`,
  );
  if (typeof raw !== 'string' || raw.startsWith('ERR:')) throw new Error(`${command} 失败：${raw}`);
  return JSON.parse(raw);
};
let debugPort = 0;
const candidatePorts = [];
const targetsAt = async (p) => {
  try {
    const response = await fetch(`http://127.0.0.1:${p}/json`, { signal: AbortSignal.timeout(1200) });
    return await response.json();
  } catch {
    return null;
  }
};
const targets = async () => (await targetsAt(debugPort)) ?? [];
const isMainTarget = (t) =>
  t.type === 'page' &&
  t.url.includes('tauri.localhost') &&
  !/(settings|drag-ghost|compare)\.html/.test(t.url);
const currentLabel = (client) =>
  evalIn(client, `window.__TAURI_INTERNALS__.metadata?.currentWindow?.label ?? ''`);
/** 连接指定 label 的主窗口：轮询最近若干调试端口（WebView2 可能复用旧浏览器进程导致端口漂移）。 */
const connectWindow = async (label, timeoutMs = 60000) => {
  const deadline = Date.now() + timeoutMs;
  let lastReason = '无页面目标';
  while (Date.now() < deadline) {
    const ports = [...new Set([port, ...candidatePorts])];
    for (const p of ports) {
      const list = (await targetsAt(p))?.filter(isMainTarget) ?? [];
      for (const item of list) {
        let client = null;
        try {
          client = await createClient(item.webSocketDebuggerUrl);
          const ready = await waitForValue(async () => {
            try {
              return (await evalIn(client, '!!window.__srt?.openPath')) ? true : null;
            } catch {
              return null;
            }
          }, 3000);
          if (ready !== true) {
            client.close();
            continue;
          }
          const got = await currentLabel(client);
          if (got === label) {
            debugPort = p;
            await dismissOnboarding((expr) => evalIn(client, expr));
            return client;
          }
          lastReason = `port=${p} label=${got || '(未知)'}`;
          client.close();
        } catch (error) {
          lastReason = String(error).slice(0, 100);
          try {
            client?.close();
          } catch {
            // 忽略：连接清理失败不阻断
          }
        }
      }
    }
    await delay(1000);
  }
  throw new Error(`未找到窗口 ${label}（${lastReason}）`);
};
const launch = async (dir) => {
  // 前置保证：无残留实例（单实例转发会把参数转给旧实例并立即退出，导致后续全部连接错对象）
  if (hasApp()) {
    forceKillAll();
    await waitNoProc();
  }
  port += 1;
  candidatePorts.unshift(port);
  child = spawn(exe, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dir,
      SRT_NO_ELEVATION: '1',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });
  let exited = false;
  child.once('exit', () => {
    exited = true;
  });
  await delay(600);
  if (exited) {
    // 新实例秒退 = 被单实例转发（旧实例未退出）：强制清场并明确报错
    forceKillAll();
    await waitNoProc();
    throw new Error('新实例被单实例转发（旧实例未退出）');
  }
  return connectWindow('main');
};
const openPath = (client, path) =>
  evalIn(client, `(async () => { await window.__srt.openPath(${JSON.stringify(path)}); return true; })()`);
const listTabs = (client, pane) => invoke(client, 'list_tabs', pane ? { pane } : {});
const rowText = (client, row) =>
  evalIn(client, `document.querySelector('.row[data-row="${row}"]')?.textContent ?? ''`);
const rowTexts = (client) =>
  evalIn(client, `[...document.querySelectorAll('.row .txt')].map((n) => n.textContent)`);
/** 折叠生效判定：子行不可见而后续标题可见。折叠时 data-row 为显示行索引，故按文本判断。 */
const foldApplied = async (client) => {
  const texts = await rowTexts(client);
  return Array.isArray(texts) && !texts.includes('  child 1') && texts.includes('root2');
};
const statusText = (client) => evalIn(client, `document.querySelector('.status-bar')?.textContent ?? ''`);
const saveWait = () => delay(3400);

const openSettings = async () => {
  await evalIn(main, `(document.querySelector('button[title="设置"]')?.click(), true)`);
  settings = await createClient(await findTarget(debugPort, 'settings.html'));
  // 等待注册表渲染出目标行（仅等 [data-setting] 泛查询会撞上分段加载时序）
  const ready = await waitForValue(
    async () =>
      (await evalIn(
        settings,
        `!!document.querySelector('[data-setting="app.startup.restoreItems.layout"]')`,
      ))
        ? true
        : null,
    12000,
  );
  if (ready !== true) throw new Error('设置窗口未就绪');
};
const closeSettings = async () => {
  await settings.send('Runtime.evaluate', {
    expression: `document.querySelector('.title-bar button[aria-label="关闭"]')?.click() ?? true`,
    returnByValue: true,
  });
  settings.close();
  settings = null;
  await delay(600);
};
const setToggle = async (id, value) => {
  const got = await evalIn(
    settings,
    `(() => { const el = document.querySelector('[data-setting="${id}"]'); el.checked = ${value ? 'true' : 'false'}; el.dispatchEvent(new Event('change', { bubbles: true })); return el.checked; })()`,
  );
  await delay(300);
  return got;
};
const setSelect = async (id, value) => {
  await evalIn(
    settings,
    `(() => { const el = document.querySelector('[data-setting="${id}"]'); el.value = ${JSON.stringify(value)}; el.dispatchEvent(new Event('change', { bubbles: true })); return el.value; })()`,
  );
  await delay(300);
};
const key = async (client, keyName, code, vk, modifiers = 0) => {
  await client.send('Input.dispatchKeyEvent', {
    type: 'rawKeyDown',
    key: keyName,
    code,
    windowsVirtualKeyCode: vk,
    nativeVirtualKeyCode: vk,
    modifiers,
  });
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    key: keyName,
    code,
    windowsVirtualKeyCode: vk,
    nativeVirtualKeyCode: vk,
    modifiers,
  });
};

try {
  // ---------- 第一阶段：开关存在性与折叠开启 ----------
  step = 'U1 设置项存在';
  main = await launch(dataDir);
  await openSettings();
  const toggles = await evalIn(
    settings,
    `['caret','scroll','folds','layout'].map((k) => !!document.querySelector('[data-setting="app.startup.restoreItems.' + k + '"]'))`,
  );
  check('U1a 设置窗出现 4 项会话恢复开关', Array.isArray(toggles) && toggles.every(Boolean), JSON.stringify(toggles));
  await setToggle('app.startup.restoreItems.caret', true);
  await setToggle('app.startup.restoreItems.folds', true);
  // 折叠设置在「阅读」页签（app.display 组）
  await evalIn(settings, `(document.querySelectorAll('.tabs .tab')[1]?.click(), true)`);
  await delay(400);
  await setSelect('app.display.folding', 'indent');
  const foldingValue = await evalIn(
    settings,
    `document.querySelector('[data-setting="app.display.folding"]')?.value ?? ''`,
  );
  check('U1b 折叠方式设置为 indent', foldingValue === 'indent', String(foldingValue));
  await closeSettings();

  step = 'U2 折叠保存与恢复';
  await openPath(main, fileFold);
  const marksReady = await waitForValue(
    async () => ((await evalIn(main, `document.querySelectorAll('.fold-mark').length`)) > 0 ? true : null),
    10000,
  );
  await evalIn(main, `(document.querySelector('.fold-mark')?.click(), true)`);
  const hidden = await waitForValue(async () => ((await foldApplied(main)) ? true : null), 6000);
  check('U2a 折叠后子行隐藏', marksReady === true && hidden === true, `texts=${JSON.stringify(await rowTexts(main))}`);
  await saveWait();
  await killApp();

  step = 'U2 重启恢复折叠';
  main = await launch(dataDir);
  const foldRestored = await waitForValue(async () => ((await foldApplied(main)) ? true : null), 15000);
  const markCount = await evalIn(main, `document.querySelectorAll('.fold-mark').length`);
  check('U2b 重启恢复折叠状态', foldRestored === true && markCount > 0, `marks=${markCount}`);

  // ---------- U3 折叠失效丢弃（文件变化导致区间长度不匹配） ----------
  step = 'U3 折叠失效丢弃';
  await killApp();
  writeFile(
    'fold.txt',
    ['root', '  child 1', '  child 2', '  extra child', 'root2', '  child 3'].join('\n') + '\n',
  );
  main = await launch(dataDir);
  const foldDropped = await waitForValue(async () => {
    const texts = await rowTexts(main);
    return Array.isArray(texts) && texts.includes('  child 1') && texts.includes('  extra child')
      ? true
      : null;
  }, 15000);
  check(
    'U3 区间不匹配时丢弃折叠锚点',
    foldDropped === true,
    `texts=${JSON.stringify(await rowTexts(main))}`,
  );

  // ---------- U4 光标保存与恢复 ----------
  step = 'U4 光标恢复';
  await openSettings();
  await setToggle('app.startup.restoreItems.folds', false);
  await setToggle('app.startup.restoreItems.caret', true);
  await closeSettings();
  await openPath(main, fileCaret);
  await evalIn(main, `(document.querySelector('[aria-label="切换编辑模式"]')?.click(), true)`);
  await waitForValue(
    async () => ((await evalIn(main, `!!document.querySelector('textarea.input-proxy')`)) ? true : null),
    10000,
  );
  await evalIn(main, `(document.querySelector('textarea.input-proxy')?.focus(), true)`);
  await key(main, 'Home', 'Home', 36, 2);
  await delay(200);
  for (let i = 0; i < 60; i += 1) {
    await key(main, 'ArrowDown', 'ArrowDown', 40);
    if (i % 20 === 19) await delay(120);
  }
  await key(main, 'Home', 'Home', 36);
  await key(main, 'ArrowRight', 'ArrowRight', 39);
  await key(main, 'ArrowRight', 'ArrowRight', 39);
  const placed = await waitForValue(
    async () => {
      const text = await statusText(main);
      return text.includes('61:3') ? text : null;
    },
    8000,
  );
  check('U4a 光标定位到 61:3', placed !== null, String(placed).slice(0, 80));
  await saveWait();
  await killApp();

  step = 'U4 重启恢复光标';
  main = await launch(dataDir);
  const caretRestored = await waitForValue(
    async () => {
      const text = await statusText(main);
      return text.includes('61:3') ? text : null;
    },
    15000,
  );
  check('U4b 重启恢复光标位置', caretRestored !== null, String(caretRestored).slice(0, 80));

  // ---------- U5 越界裁剪提示 ----------
  step = 'U5 越界裁剪';
  await killApp();
  writeFile('caret.txt', Array.from({ length: 30 }, (_, i) => `line ${i + 1}`).join('\n') + '\n');
  main = await launch(dataDir);
  let toastSeen = '';
  const toastWatch = (async () => {
    for (let i = 0; i < 60; i += 1) {
      const text = await evalIn(
        main,
        `[...document.querySelectorAll('.toast')].map((n) => n.textContent).join('|')`,
      );
      if (typeof text === 'string' && text.length > 0) {
        toastSeen = text;
        if (text.includes('超出文件长度')) break;
      }
      await delay(250);
    }
  })();
  const clipped = await waitForValue(
    async () => {
      const text = await statusText(main);
      // 行越界裁剪到末行、列保留（原 61:3 → 30:3）
      return text.includes('30:3') ? text : null;
    },
    15000,
  );
  await Promise.race([toastWatch, delay(6000)]);
  const finalStatus = clipped ?? (await statusText(main));
  check(
    'U5 越界裁剪到最后一行并提示',
    clipped !== null && toastSeen.includes('超出文件长度'),
    `status=${String(finalStatus).slice(0, 50)} toast=${toastSeen.slice(0, 50)}`,
  );

  // ---------- U6 滚动关闭 ----------
  step = 'U6 滚动开关';
  await openSettings();
  await setToggle('app.startup.restoreItems.caret', false);
  await setToggle('app.startup.restoreItems.scroll', false);
  await closeSettings();
  await openPath(main, fileScroll);
  await waitForValue(async () => ((await rowText(main, 0)) === 's 1' ? true : null), 10000);
  const scrolled = await evalIn(
    main,
    `(() => { const el = document.querySelector('.reader-scroll') ?? document.querySelector('.reader'); const row = document.querySelector('.row[data-row="40"]'); if (!el || !row) return -1; el.scrollTop = row.offsetTop; el.dispatchEvent(new Event('scroll')); return el.scrollTop; })()`,
  );
  await delay(500);
  await saveWait();
  await killApp();

  step = 'U6 滚动关闭重启';
  main = await launch(dataDir);
  await waitForValue(async () => ((await rowText(main, 0)) === 's 1' ? true : null), 15000);
  const topVisible = await evalIn(
    main,
    `(() => { const el = document.querySelector('.reader-scroll') ?? document.querySelector('.reader'); return el ? el.scrollTop : -1; })()`,
  );
  const secondRow = await rowText(main, 1);
  check(
    'U6 关闭滚动恢复后从顶部打开',
    topVisible === 0 && secondRow === 's 2',
    `scrollTop=${topVisible} row1=${secondRow} preScroll=${scrolled}`,
  );

  // ---------- U7 布局关闭合并单栏 ----------
  step = 'U7 布局关闭';
  await openSettings();
  await setToggle('app.startup.restoreItems.layout', false);
  await closeSettings();
  await openPath(main, fileScroll);
  // 记录拆分前首栏标签集合；期望值 = 首栏 ∪ {other.txt}
  const beforeSplit = await listTabs(main, 'main#1');
  const expectedMerged = [...beforeSplit.tabs.map((tab) => tab.name), 'other.txt'].sort();
  await evalIn(main, `(document.querySelector('[data-pane-split-right]')?.click(), true)`);
  await delay(600);
  await openPath(main, fileOther);
  await saveWait();
  await killApp();

  step = 'U7 合并单栏重启';
  main = await launch(dataDir);
  const singlePane = await waitForValue(
    async () => {
      const count = await evalIn(main, `document.querySelectorAll('[data-pane]').length`);
      return count === 1 ? count : null;
    },
    15000,
  );
  let lastMergedNames = null;
  const merged = await waitForValue(async () => {
    const view = await listTabs(main, 'main#1');
    lastMergedNames = view.tabs.map((tab) => tab.name).sort();
    return lastMergedNames.length === expectedMerged.length ? lastMergedNames : null;
  }, 12000);
  check(
    'U7 布局关闭后合并为单栏且标签保留',
    singlePane === 1 && Array.isArray(merged) && merged.join(',') === expectedMerged.join(','),
    `panes=${singlePane} tabs=${JSON.stringify(lastMergedNames)} expected=${JSON.stringify(expectedMerged)}`,
  );

  // ---------- U8 总开关关闭：窗口数量与几何仍恢复 ----------
  step = 'U8 总开关关闭准备';
  await openSettings();
  await setToggle('app.startup.restoreItems.layout', true);
  await setToggle('app.startup.restoreItems.scroll', true);
  await setToggle('app.startup.restoreItems.caret', true);
  await closeSettings();
  await evalIn(main, `(async () => { await window.__TAURI_INTERNALS__.invoke('new_window', {}); return true; })()`);
  await waitForValue(async () => {
    const list = (await targets()).filter(isMainTarget);
    return list.length === 2 ? list : null;
  }, 15000);
  const secondClient = await connectWindow('main-2');
  await openPath(secondClient, fileOther);
  await saveWait();
  await openSettings();
  await setToggle('app.startup.restoreSession', false);
  await closeSettings();
  await saveWait();
  await killApp();

  step = 'U8 总开关关闭重启';
  main = await launch(dataDir);
  const twoWindows = await waitForValue(async () => {
    const list = (await targets()).filter(isMainTarget);
    return list.length === 2 ? list : null;
  }, 20000);
  const mainTabs = await listTabs(main);
  let w2Empty = false;
  try {
    const w2 = await connectWindow('main-2');
    w2Empty = (await listTabs(w2)).tabs.length === 0;
  } catch {
    // 第二窗口未连上：w2Empty 保持 false，由断言报告
  }
  const emptyShown = await waitForValue(
    async () => ((await evalIn(main, `!!document.querySelector('.empty')`)) ? true : null),
    12000,
  );
  check(
    'U8 总开关关闭仍恢复两窗口且不重开标签',
    twoWindows !== null && mainTabs.tabs.length === 0 && w2Empty && emptyShown === true,
    `windows=${twoWindows?.length ?? 0} mainTabs=${mainTabs.tabs.length} w2Empty=${w2Empty}`,
  );

  // ---------- U9 布局恢复回归（开启后两栏与激活项） ----------
  step = 'U9 布局恢复';
  await openSettings();
  await setToggle('app.startup.restoreSession', true);
  await closeSettings();
  await openPath(main, fileScroll);
  await evalIn(main, `(document.querySelector('[data-pane-split-right]')?.click(), true)`);
  await delay(600);
  await openPath(main, fileOther);
  // 显式激活首栏：验证 focusedPane 恢复（分屏后激活栏为新栏，需切回再保存）
  await evalIn(
    main,
    `(document.querySelector('[data-pane="main#1"]')?.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })), true)`,
  );
  await delay(300);
  await saveWait();
  await killApp();

  step = 'U9 布局恢复重启';
  main = await launch(dataDir);
  const layoutRestored = await waitForValue(async () => {
    const count = await evalIn(main, `document.querySelectorAll('[data-pane]').length`);
    const active = await evalIn(
      main,
      `document.querySelector('[data-pane-active="true"]')?.getAttribute('data-pane') ?? ''`,
    );
    return count === 2 ? { count, active } : null;
  }, 15000);
  const pane2Tabs = await waitForValue(async () => {
    const view = await listTabs(main, 'main#2');
    return view.tabs.length === 1 ? view.tabs.map((tab) => tab.name) : null;
  }, 12000);
  check(
    'U9 布局与激活栏恢复',
    layoutRestored !== null && layoutRestored.active === 'main#1' && pane2Tabs?.[0] === 'other.txt',
    `panes=${layoutRestored?.count} active=${layoutRestored?.active} pane2=${JSON.stringify(pane2Tabs)}`,
  );

  // ---------- U10 截图（设置项可见） ----------
  step = 'U10 截图';
  await openSettings();
  await evalIn(
    settings,
    `(document.querySelector('[data-setting="app.startup.restoreItems.layout"]')?.scrollIntoView({ block: 'center' }), true)`,
  );
  await delay(400);
  const shot = await settings.send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(root, 'docs', 'screenshots', 'phase-p37-restore.png'), Buffer.from(shot.data, 'base64'));
  check('U10 截图已保存', true, 'docs/screenshots/phase-p37-restore.png');
  await closeSettings();
} finally {
  clearTimeout(watchdog);
  await killApp();
  await delay(800);
}

const passed = results.filter(Boolean).length;
console.log(`\n会话恢复冒烟结果：${passed}/${results.length} 通过`);
if (passed === results.length) {
  try {
    rmSync(work, { recursive: true, force: true });
  } catch {
    console.log(`工作目录占用中，保留（${work}）`);
  }
} else {
  console.log(`工作目录保留：${work}`);
  process.exitCode = 1;
}
