// 限制可调端到端冒烟（P4）：逐项修改设置并验证真实行为变化（受限拒绝 ↔ 放宽放行），
// 覆盖分屏上限（含语言切换提示）、导出/打印/比较/拆分/工作区/大纲/折叠/剪贴板/
// 批注/设置导入上限，以及启动窗口数上限（两次重启验证）。
//
// 依赖：已构建的 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 数据隔离：SRT_DATA_DIR 指向项目 tmp 工作目录；通过后清理，失败保留现场。

import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget } from './lib/smoke-cdp.mjs';
import { removeWithRetryAsync } from './lib/system.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-limits-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
writeFileSync(join(dataDir, 'settings.json'), JSON.stringify({ schemaVersion: 17 }), 'utf8');
const basePort = Number(argValue('--port', String(8930 + Math.floor(Math.random() * 60))));

const results = [];
const check = (name, ok, extra = '') => {
  results.push({ name, ok });
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${extra ? `  → ${extra}` : ''}`);
};

let child = null;
let port = 0;
let main = null;

const watchdog = setTimeout(() => {
  console.log('FAIL  看门狗超时（720s）');
  process.exitCode = 4;
  if (child) spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
}, 720_000);
process.on('exit', () => spawnSync('taskkill', ['/IM', 's-read-txt.exe', '/T', '/F'], { stdio: 'ignore' }));

const evalIn = async (client, expression) => {
  const result = await client.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(`eval 异常: ${result.exceptionDetails.text}`);
  return result.result.value;
};

const waitFor = async (client, expression, predicate, timeoutMs = 8000) => {
  const deadline = Date.now() + timeoutMs;
  let last;
  for (;;) {
    last = await evalIn(client, expression);
    if (predicate(last)) return last;
    if (Date.now() > deadline) return last;
    await delay(150);
  }
};

/** 启动应用并等待主窗就绪（含残留目标防护与重试）。 */
async function launch() {
  port = basePort + Math.floor(Math.random() * 200);
  child = spawn(exe, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dataDir,
      SRT_NO_ELEVATION: '1',
      SRT_PRINT_NO_AUTO: '1',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });
  let ws = null;
  for (let attempt = 1; attempt <= 3 && !ws; attempt += 1) {
    try {
      ws = await findTarget(port, 'tauri.localhost');
    } catch (error) {
      if (attempt === 3) throw error;
      await delay(2500);
    }
  }
  main = await createClient(ws);
  await main.send('Page.enable');
  await dismissOnboarding((expression) => evalIn(main, expression));
  await waitFor(main, `typeof window.__TAURI_INTERNALS__ === 'object'`, (v) => v === true, 15000);
}

async function stop() {
  if (child) {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    child = null;
  }
  for (let i = 0; i < 30; i += 1) {
    const out = spawnSync('tasklist', ['/FI', 'IMAGENAME eq s-read-txt.exe', '/NH'], { encoding: 'utf8' });
    if (!String(out.stdout).includes('s-read-txt.exe')) break;
    await delay(200);
  }
  await delay(900);
  main = null;
}

const invoke = (cmd, args = {}) =>
  evalIn(main, `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);

const invokeSafe = (cmd, args = {}) =>
  evalIn(
    main,
    `(async () => { try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}) }; } catch (e) { return { ok: false, error: e }; } })()`,
  );

/** 修改并保存设置（assignments 为 app 对象上的赋值语句串）。 */
const setApp = async (assignments) => {
  const expr = `(async () => {
    const s = await window.__TAURI_INTERNALS__.invoke('get_settings');
    const app = s.app;
    ${assignments}
    await window.__TAURI_INTERNALS__.invoke('save_settings', { request: { app, reader: s.reader, shortcuts: s.shortcuts.bindings } });
    return true;
  })()`;
  await evalIn(main, expr);
  await delay(250);
};

const writeText = (name, text) => {
  const file = join(work, name);
  writeFileSync(file, text, 'utf8');
  return file;
};

try {
  await launch();

  // ---------- L1 分屏上限 ----------
  await setApp('app.maxPanes = 1;');
  await evalIn(main, `(document.querySelector('[data-pane="main#1"] [data-pane-split-right]')?.click(), true)`);
  await delay(400);
  const panes1 = await evalIn(main, `document.querySelectorAll('[data-pane]').length`);
  check('L1a maxPanes=1 时拆分被拒（仍单栏）', panes1 === 1, `panes=${panes1}`);
  const toastZh = await evalIn(main, `[...document.querySelectorAll('.toast')].map((n) => n.textContent).join('|')`);
  check('L1b 拒绝提示按设置值（含 1）', toastZh.includes('1'), `toast=${toastZh}`);

  await setApp("app.locale = 'en';");
  await delay(600);
  await evalIn(main, `(document.querySelector('[data-pane="main#1"] [data-pane-split-right]')?.click(), true)`);
  await delay(400);
  const toastEn = await evalIn(main, `[...document.querySelectorAll('.toast')].map((n) => n.textContent).join('|')`);
  check('L1c 切换英文后同提示为英文', /Up to/i.test(toastEn), `toast=${toastEn}`);
  await setApp("app.locale = 'zh-CN';");
  await delay(400);

  await setApp('app.maxPanes = 2;');
  await evalIn(main, `(document.querySelector('[data-pane="main#1"] [data-pane-split-right]')?.click(), true)`);
  const panes2 = await waitFor(main, `document.querySelectorAll('[data-pane]').length`, (v) => v === 2);
  check('L1d maxPanes=2 时拆分放行（两栏）', panes2 === 2, `panes=${panes2}`);

  // ---------- L2/L3 导出与打印上限 ----------
  const bigFile = writeText('big.txt', 'x'.repeat(2 * 1024 * 1024));
  const tabInfo = await invoke('open_file', { path: bigFile });
  const tabId = tabInfo.tabId;
  await setApp('app.file.exportMaxMB = 1;');
  const exportDenied = await invokeSafe('export_text', { tabId, path: join(work, 'out1.txt') });
  check(
    'L2a 导出超上限被拒（EXPORT_TOO_LARGE）',
    exportDenied.ok === false && exportDenied.error?.code === 'EXPORT_TOO_LARGE',
    JSON.stringify(exportDenied.error),
  );
  await setApp('app.file.exportMaxMB = 10;');
  const exportOk = await invokeSafe('export_text', { tabId, path: join(work, 'out1.txt') });
  check('L2b 提高上限后导出成功', exportOk.ok === true && exportOk.value > 0, `bytes=${exportOk.value}`);

  await setApp('app.file.printMaxMB = 1;');
  const printDenied = await invokeSafe('print_document', { tabId });
  check(
    'L3 打印超上限被拒（PRINT_TOO_LARGE）',
    printDenied.ok === false && printDenied.error?.code === 'PRINT_TOO_LARGE',
    JSON.stringify(printDenied.error),
  );
  await setApp('app.file.printMaxMB = 16;');

  // ---------- L4 比较上限 ----------
  const cmpLine = 'y'.repeat(99);
  const cmpLines = Array.from({ length: 180_000 }, () => cmpLine);
  const bigCompare = writeText('cmp-big.txt', cmpLines.join('\n'));
  const changed = [...cmpLines];
  changed[0] = `Y${cmpLine.slice(1)}`;
  const smallCompare = writeText('cmp-small.txt', changed.join('\n'));
  await setApp('app.tools.compareMaxMB = 16;');
  const diffDenied = await invokeSafe('diff_docs', { left: bigCompare, right: smallCompare });
  check(
    'L4a 比较超上限被拒（FILE_TOO_LARGE）',
    diffDenied.ok === false && diffDenied.error?.code === 'FILE_TOO_LARGE',
    JSON.stringify(diffDenied.error),
  );
  await setApp('app.tools.compareMaxMB = 64;');
  const diffOk = await invokeSafe('diff_docs', { left: bigCompare, right: smallCompare });
  check('L4b 提高上限后比较成功', diffOk.ok === true && Array.isArray(diffOk.value?.hunks), JSON.stringify(diffOk).slice(0, 220));

  // ---------- L5/L6 拆分上限与预览份数 ----------
  const splitFile = writeText('split.txt', Array.from({ length: 10 }, (_, i) => `l${i}\n`).join(''));
  await setApp('app.tools.splitMaxParts = 2;');
  const splitDenied = await invokeSafe('preview_split', { path: splitFile, mode: { kind: 'lines', linesPerFile: 1 } });
  check(
    'L5a 拆分超份数上限被拒（SPLIT_INVALID）',
    splitDenied.ok === false && splitDenied.error?.code === 'SPLIT_INVALID',
    JSON.stringify(splitDenied.error),
  );
  await setApp('app.tools.splitMaxParts = 10; app.tools.splitPreviewParts = 3;');
  const preview = await invokeSafe('preview_split', { path: splitFile, mode: { kind: 'lines', linesPerFile: 1 } });
  check(
    'L5b/L6 提高上限后放行且预览按设置截断',
    preview.ok === true && preview.value?.totalParts === 10 && preview.value?.parts?.length === 3,
    `total=${preview.value?.totalParts} parts=${preview.value?.parts?.length}`,
  );

  // ---------- L7 工作区搜索匹配上限 ----------
  const matchA = writeText('ws-a.txt', Array.from({ length: 15 }, () => 'needle line').join('\n'));
  const matchB = writeText('ws-b.txt', Array.from({ length: 15 }, () => 'needle line').join('\n'));
  await invoke('open_file', { path: matchA });
  await invoke('open_file', { path: matchB });
  await setApp('app.tools.workspaceMatchCap = 10;');
  const search = await invokeSafe('search_workspace', { query: 'needle', caseSensitive: false });
  const perFile = search.value?.files?.map((f) => f.matches.length) ?? [];
  check(
    'L7 工作区搜索逐文件截断到上限且标记 truncated',
    search.ok === true && perFile.length >= 2 && perFile.every((n) => n <= 10) && search.value?.truncated === true,
    `perFile=${JSON.stringify(perFile)} total=${search.value?.totalMatches}`,
  );

  // ---------- L8 大纲条目上限 ----------
  const outlineFile = writeText('outline.txt', Array.from({ length: 120 }, (_, i) => `# 标题${i}\n正文`).join('\n'));
  const outlineTab = await invoke('open_file', { path: outlineFile });
  await setApp("app.display.outlinePatterns = ['^# ']; app.display.outlineMaxItems = 100;");
  const outline = await invokeSafe('outline_items', { tabId: outlineTab.tabId });
  check('L8 大纲条目按设置截断（120→100）', outline.ok === true && outline.value?.length === 100, `items=${outline.value?.length}`);

  // ---------- L9 折叠区域上限 ----------
  const foldFile = writeText('fold.txt', Array.from({ length: 150 }, () => 'a\n  b').join('\n'));
  const foldTab = await invoke('open_file', { path: foldFile });
  await setApp("app.display.folding = 'indent'; app.display.foldMaxRegions = 100;");
  const folds = await invokeSafe('fold_regions', { tabId: foldTab.tabId });
  check('L9 折叠区域按设置截断（150+→100）', folds.ok === true && folds.value?.length === 100, `regions=${folds.value?.length}`);

  // ---------- L10 剪贴板单条上限 ----------
  await setApp('app.editor.clipboard.entryMaxChars = 100; app.editor.clipboard.historyLimit = 50;');
  const clip = await invokeSafe('add_clipboard_entry', { text: 'c'.repeat(200) });
  check(
    'L10 剪贴板单条按设置截断（200→100）',
    clip.ok === true && clip.value?.[0]?.text?.length === 100,
    `len=${clip.value?.[0]?.text?.length}`,
  );

  // ---------- L11 批注上限 ----------
  const annTab = await invoke('open_file', { path: matchA });
  await setApp('app.annotations.noteMaxChars = 100; app.annotations.labelMaxChars = 10;');
  const note = await invokeSafe('add_note', { tabId: annTab.tabId, row: 0, utf16: 0, text: 'n'.repeat(150), kind: 'note' });
  check(
    'L11a 注释文本按设置截断（150→100）',
    note.ok === true && note.value?.notes?.[0]?.text?.length === 100,
    `len=${note.value?.notes?.[0]?.text?.length}`,
  );
  const bookmark = await invokeSafe('add_bookmark', { tabId: annTab.tabId, row: 1, utf16: 0, label: 'L'.repeat(20) });
  check(
    'L11b 书签标签按设置截断（20→10）',
    bookmark.ok === true && bookmark.value?.bookmarks?.[0]?.label?.length === 10,
    `len=${bookmark.value?.bookmarks?.[0]?.label?.length}`,
  );

  // ---------- L12 设置导入上限 ----------
  const bundlePath = join(work, 'bundle.json');
  await invoke('export_settings', { path: bundlePath });
  const bundleRaw = readFileSync(bundlePath, 'utf8');
  writeFileSync(bundlePath, bundleRaw + ' '.repeat(2 * 1024 * 1024), 'utf8');
  await setApp('app.maxImportMB = 1;');
  const importDenied = await invokeSafe('import_settings', { path: bundlePath });
  check(
    'L12a 导入超上限被拒（SETTINGS_IMPORT）',
    importDenied.ok === false && importDenied.error?.code === 'SETTINGS_IMPORT',
    JSON.stringify(importDenied.error),
  );
  await setApp('app.maxImportMB = 8;');
  const importOk = await invokeSafe('import_settings', { path: bundlePath });
  check('L12b 提高上限后导入成功', importOk.ok === true);

  // ---------- L13/L14 启动窗口上限（两次重启） ----------
  const fileA = writeText('win-a.txt', 'A\n');
  const fileB = writeText('win-b.txt', 'B\n');
  const session = {
    schemaVersion: 4,
    focusedLabel: 'main',
    windows: [
      {
        label: 'main',
        window: { width: 1100, height: 760, maximized: false },
        panes: [{ pane: 'main#1', activeTabIndex: 0, tabs: [{ path: fileA, scrollRow: 0 }] }],
        layout: { type: 'leaf', pane: 'main#1' },
      },
      {
        label: 'main-2',
        window: { width: 1100, height: 760, maximized: false },
        panes: [{ pane: 'main-2#1', activeTabIndex: 0, tabs: [{ path: fileB, scrollRow: 0 }] }],
        layout: { type: 'leaf', pane: 'main-2#1' },
      },
    ],
  };
  writeFileSync(join(dataDir, 'session.json'), JSON.stringify(session), 'utf8');
  await setApp('app.startup.maxWindows = 1; app.startup.restoreSession = true;');
  await stop();
  await launch();
  const count1 = await waitFor(main, `window.__TAURI_INTERNALS__.invoke('main_window_count')`, (v) => v === 1, 15000);
  check('L13 maxWindows=1 时只恢复 1 个窗口', count1 === 1, `count=${count1}`);

  writeFileSync(join(dataDir, 'session.json'), JSON.stringify(session), 'utf8');
  await setApp('app.startup.maxWindows = 16;');
  await stop();
  await launch();
  const count2 = await waitFor(main, `window.__TAURI_INTERNALS__.invoke('main_window_count')`, (v) => v === 2, 20000);
  check('L14 maxWindows=16 时恢复 2 个窗口', count2 === 2, `count=${count2}`);

  const failed = results.filter((r) => !r.ok);
  console.log(`结果：${results.length - failed.length}/${results.length} 通过`);
  process.exitCode = failed.length === 0 ? 0 : 1;
} catch (error) {
  console.error(`套件异常：${error?.stack ?? error}`);
  process.exitCode = 1;
} finally {
  clearTimeout(watchdog);
  await stop();
  if (process.exitCode === 0) {
    await removeWithRetryAsync(work);
  }
}
