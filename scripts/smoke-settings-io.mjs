// 设置导入/导出/重置/迁移 E2E（覆盖矩阵补测：G1–G3）。
// 场景：
//   M1–M5 旧版（v1）配置启动自动迁移：版本升级 + `.v1.bak` 备份 + 用户值保留 + 新字段补默认
//   E1–E10 命令链（页面内真实 invoke）：
//     E1 导出落盘结构校验；E2–E4 三类篡改拒绝（范围 / 未知字段 / 类型）；
//     E5 合法导入生效 + `*.import-bak`；E6–E8 重置单项 / 分组 / 全部；
//     E9 注册表内容断言；E10 未知项拒绝；
//     E11–E13 快捷键独立导入/导出（P0-9）：导出结构 / 导入生效 / 未知动作拒绝。
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  argValue,
  createClient,
  delay,
  dismissOnboarding,
  findTarget,
  waitForValue,
} from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = resolve(argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe')));
const work = join(tmpdir(), `srt-settings-io-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
const port = 9900 + Math.floor(Math.random() * 90);

/** 预置 v1 配置：触发启动迁移，并保留可验证的用户值。 */
function seedV1Files() {
  writeFileSync(
    join(dataDir, 'settings.json'),
    JSON.stringify(
      {
        schemaVersion: 1,
        logLevel: 'info',
        maxFileSizeMB: 100,
        maxTabs: 33,
        history: { maxEntries: 3000, retentionDays: 100 },
        saveBackupEnabled: true,
        showOnboarding: true,
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(dataDir, 'reader.json'),
    JSON.stringify(
      {
        schemaVersion: 1,
        theme: 'dark',
        typography: {
          fontFamily: 'Microsoft YaHei',
          fontSize: 20,
          lineHeight: 1.8,
          contentWidth: 720,
          pagePadding: 48,
          pagePaddingY: 48,
          paragraphSpacing: 4,
          firstLineIndent: 2,
          textAlign: 'left',
          smoothScroll: true,
        },
        statusBar: { showFileName: true, showPercent: true, showSize: true, showEncoding: true },
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(dataDir, 'shortcuts.json'),
    JSON.stringify({ schemaVersion: 1, bindings: { openFile: 'Ctrl+Shift+O' } }, null, 2),
  );
}
seedV1Files();

let child = spawn(exe, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: 'ignore',
});

const evalIn = async (client, expression) => {
  const result = await client.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result?.value;
};

let passed = 0;
let failed = 0;
const ok = (name, extra = '') => {
  passed += 1;
  console.log(`PASS  ${name}${extra ? `  ← ${extra}` : ''}`);
};
const bad = (name, extra = '') => {
  failed += 1;
  console.log(`FAIL  ${name}${extra ? `  ← ${extra}` : ''}`);
};
const chk = (name, condition, extra = '') => (condition ? ok(name, extra) : bad(name, extra));

const readJson = (path) => JSON.parse(readFileSync(path, 'utf8'));
const codeOf = (result) => result?.error?.code ?? '';
const msgOf = (result) => String(result?.error?.message ?? result?.error ?? '');

try {
  const main = await createClient(await findTarget(port));
  const evalMain = (expression) => evalIn(main, expression);
  await waitForValue(
    async () => ((await evalMain('(() => !!(window.__srt && window.__srt.openPath))()')) ? true : null),
    30000,
  );
  await dismissOnboarding(evalMain);
  await delay(300);

  // 页面内统一调用助手：捕获 IpcError（code/message）而不抛出。
  await evalMain(
    `(() => { window.__sio = async (cmd, args) => {
        try { return { ok: true, value: await window.__TAURI_INTERNALS__.invoke(cmd, args) }; }
        catch (error) { return { ok: false, error }; } };
      return true; })()`,
  );
  const call = (cmd, args = {}) => evalMain(`window.__sio(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);

  // ---- M1–M5：启动迁移 ----
  const appAfter = readJson(join(dataDir, 'settings.json'));
  chk('M1 settings.json 迁移到 v3', appAfter.schemaVersion === 3, `schemaVersion=${appAfter.schemaVersion}`);
  const bakPath = join(dataDir, 'settings.json.v1.bak');
  chk(
    'M2 迁移前备份 .v1.bak（内容为 v1）',
    existsSync(bakPath) && readJson(bakPath).schemaVersion === 1,
  );
  const snapshot = await call('get_settings');
  chk(
    'M3 用户值保留 + 新字段补默认',
    snapshot.ok &&
      snapshot.value.app.maxTabs === 33 &&
      snapshot.value.app.hardLimitMB === 2048 &&
      snapshot.value.app.locale === 'zh-CN',
    JSON.stringify(snapshot.value?.app?.maxTabs),
  );
  const readerAfter = readJson(join(dataDir, 'reader.json'));
  const shortcutsAfter = readJson(join(dataDir, 'shortcuts.json'));
  chk(
    'M4 三文件版本升级 + 阅读/快捷键值保留',
    readerAfter.schemaVersion === 3 &&
      readerAfter.typography.fontSize === 20 &&
      shortcutsAfter.schemaVersion === 3 &&
      snapshot.value.shortcuts.bindings.openFile === 'Ctrl+Shift+O',
  );
  chk(
    'M5 三个文件均生成 .v1.bak',
    readdirSync(dataDir).filter((name) => name.endsWith('.v1.bak')).length === 3,
  );

  // ---- E1：导出落盘 ----
  const exportPath = join(work, 'export.json');
  const exported = await call('export_settings', { path: exportPath });
  const bundle = exported.ok && existsSync(exportPath) ? readJson(exportPath) : null;
  chk(
    'E1 导出落盘且结构完整',
    exported.ok &&
      bundle?.bundleVersion === 1 &&
      bundle?.schemaVersion === 3 &&
      bundle?.app?.maxTabs === 33 &&
      bundle?.reader &&
      bundle?.shortcuts,
  );

  // ---- E2–E4：三类篡改拒绝 ----
  const mutate = (apply) => {
    const value = JSON.parse(JSON.stringify(bundle));
    apply(value);
    const path = join(work, `bad-${Math.random().toString(36).slice(2, 8)}.json`);
    writeFileSync(path, JSON.stringify(value));
    return path;
  };

  const rangePath = mutate((value) => {
    value.app.maxTabs = 9999;
  });
  const rangeResult = await call('import_settings', { path: rangePath });
  chk(
    'E2 范围篡改拒绝（含字段路径）',
    !rangeResult.ok &&
      codeOf(rangeResult) === 'SETTINGS_IMPORT' &&
      msgOf(rangeResult).includes('app.maxTabs'),
    msgOf(rangeResult).slice(0, 80),
  );

  const unknownPath = mutate((value) => {
    value.app.bogus = 1;
  });
  const unknownResult = await call('import_settings', { path: unknownPath });
  chk(
    'E3 未知字段拒绝（含字段名）',
    !unknownResult.ok && msgOf(unknownResult).includes('bogus'),
    msgOf(unknownResult).slice(0, 80),
  );

  const typePath = mutate((value) => {
    value.reader.typography.fontSize = 'big';
  });
  const typeResult = await call('import_settings', { path: typePath });
  chk(
    'E4 类型篡改拒绝（含字段路径）',
    !typeResult.ok && msgOf(typeResult).includes('reader.typography.fontSize'),
    msgOf(typeResult).slice(0, 80),
  );

  // ---- E5：合法导入生效 + import-bak ----
  const goodPath = mutate((value) => {
    value.app.maxTabs = 44;
  });
  const goodResult = await call('import_settings', { path: goodPath });
  chk(
    'E5 合法导入生效 + *.import-bak',
    goodResult.ok &&
      goodResult.value.app.maxTabs === 44 &&
      readJson(join(dataDir, 'settings.json')).maxTabs === 44 &&
      existsSync(join(dataDir, 'settings.json.import-bak')),
  );

  // ---- E6：重置单项 ----
  const resetField = await call('reset_settings', { scope: { kind: 'field', id: 'app.maxTabs' } });
  chk(
    'E6 重置单项（app.maxTabs → 20）',
    resetField.ok &&
      resetField.value.app.maxTabs === 20 &&
      readJson(join(dataDir, 'settings.json')).maxTabs === 20,
  );

  // ---- E7：重置分组（阅读排版） ----
  await evalMain(
    `(async () => { const s = await window.__TAURI_INTERNALS__.invoke('get_settings');
        s.reader.typography.fontSize = 30;
        s.reader.typography.paragraphSpacing = 12;
        await window.__TAURI_INTERNALS__.invoke('save_settings', { request: { app: s.app, reader: s.reader, shortcuts: s.shortcuts.bindings } });
        return true; })()`,
  );
  const resetGroup = await call('reset_settings', { scope: { kind: 'group', name: 'reader.typography' } });
  chk(
    'E7 重置分组（字号 16 / 段间距 0）',
    resetGroup.ok &&
      resetGroup.value.reader.typography.fontSize === 16 &&
      resetGroup.value.reader.typography.paragraphSpacing === 0,
  );

  // ---- E8：重置全部（含快捷键覆盖清除） ----
  await evalMain(
    `(async () => { const s = await window.__TAURI_INTERNALS__.invoke('get_settings');
        s.shortcuts.bindings.openFile = 'Ctrl+Shift+U';
        await window.__TAURI_INTERNALS__.invoke('save_settings', { request: { app: s.app, reader: s.reader, shortcuts: s.shortcuts.bindings } });
        return true; })()`,
  );
  const resetAll = await call('reset_settings', { scope: { kind: 'all' } });
  const shortcutsRaw = readJson(join(dataDir, 'shortcuts.json'));
  chk(
    'E8 重置全部（快捷键覆盖清空、恢复默认）',
    resetAll.ok &&
      resetAll.value.shortcuts.bindings.openFile === 'Ctrl+O' &&
      Object.keys(shortcutsRaw.bindings).length === 0,
  );

  // ---- E9：注册表内容断言 ----
  const registry = await call('get_settings_registry');
  const specs = registry.ok && Array.isArray(registry.value) ? registry.value : [];
  const ids = new Set(specs.map((spec) => spec.id));
  const locale = specs.find((spec) => spec.id === 'app.locale');
  chk(
    'E9 注册表完整（≥27 项、含 app.locale 枚举、id 唯一）',
    specs.length >= 27 &&
      ids.size === specs.length &&
      locale?.kind?.type === 'enum' &&
      locale.kind.values.includes('zh-CN'),
    `count=${specs.length}`,
  );

  // ---- E10：未知项拒绝 ----
  const badReset = await call('reset_settings', { scope: { kind: 'field', id: 'app.noSuchField' } });
  chk(
    'E10 未知设置项拒绝',
    !badReset.ok && codeOf(badReset) === 'SETTINGS_RESET' && msgOf(badReset).includes('未知'),
    msgOf(badReset).slice(0, 80),
  );

  // ---- L1–L4：语言切换（持久化 locale=en → 重启后英文 UI） ----
  const savedEn = await evalMain(
    `(async () => { const s = await window.__TAURI_INTERNALS__.invoke('get_settings');
        s.app.locale = 'en';
        await window.__TAURI_INTERNALS__.invoke('save_settings', { request: { app: s.app, reader: s.reader, shortcuts: s.shortcuts.bindings } });
        return true; })()`,
  );
  spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  await delay(900);
  const port2 = port + 1;
  child = spawn(exe, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port2}`,
    },
    stdio: 'ignore',
  });
  const main2 = await createClient(await findTarget(port2));
  const evalMain2 = (expression) => evalIn(main2, expression);
  const langReady = await waitForValue(async () => {
    const lang = await evalMain2('document.documentElement.lang');
    return lang === 'en' ? true : null;
  }, 20000);
  chk('L1 重启后 <html lang> 随语言设置更新（en）', savedEn === true && langReady === true);
  const toolbarTitle = await evalMain2(
    `document.querySelector('.toolbar button[aria-label="Open file"]')?.title ?? null`,
  );
  chk(
    'L2 英文界面（工具栏提示 Open file）',
    typeof toolbarTitle === 'string' && toolbarTitle.includes('Open file'),
    String(toolbarTitle),
  );
  chk(
    'L3 英文状态栏（No file open）',
    String(await evalMain2(`document.querySelector('.status-bar')?.textContent ?? ''`)).includes(
      'No file open',
    ),
  );
  chk(
    'L4 英文标题栏（Close）',
    (await evalMain2(`document.querySelector('.title-bar .ctl.close')?.getAttribute('aria-label')`)) ===
      'Close',
  );

  // L5：菜单文案（点开「文件」菜单 → 条目为英文）
  await evalMain2(
    `(() => { document.querySelector('nav.menu-bar > button')?.click(); return true; })()`,
  );
  await delay(300);
  const menuText = String(
    await evalMain2(`document.querySelector('.menu-bar')?.textContent ?? ''`),
  );
  chk(
    'L5 英文菜单（含 Open，且无「打开」）',
    menuText.includes('Open') && !menuText.includes('打开'),
    menuText.replace(/\s+/g, ' ').slice(0, 80),
  );
  await evalMain2(
    `(() => { document.querySelector('nav.menu-bar > button')?.click(); return true; })()`,
  );
  await delay(200);
  // L6：空状态按钮（en）
  chk(
    'L6 英文空状态按钮（Open file）',
    String(await evalMain2(`document.querySelector('.empty .open-btn')?.textContent ?? ''`))
      .trim()
      .includes('Open'),
  );

  // E11–E13：快捷键独立导入/导出（P0-9）
  const shortcutExportPath = join(work, 'shortcuts-export.json');
  const shortcutExportBytes = await evalMain2(
    `(async () => await window.__TAURI_INTERNALS__.invoke('export_shortcuts', { path: ${JSON.stringify(shortcutExportPath)} }))()`,
  );
  let shortcutBundle = null;
  try {
    shortcutBundle = JSON.parse(readFileSync(shortcutExportPath, 'utf8'));
  } catch {
    shortcutBundle = null;
  }
  chk(
    'E11 快捷键导出（格式字段 + 15 项）',
    typeof shortcutExportBytes === 'number' &&
      shortcutBundle?.bundleVersion === 1 &&
      Object.keys(shortcutBundle?.bindings ?? {}).length === 15,
    `bytes=${shortcutExportBytes} keys=${shortcutBundle ? Object.keys(shortcutBundle.bindings).length : 'null'}`,
  );

  if (shortcutBundle) {
    shortcutBundle.bindings.openFile = 'Ctrl+Shift+O';
    writeFileSync(shortcutExportPath, JSON.stringify(shortcutBundle), 'utf8');
  }
  const shortcutImportResult = await evalMain2(
    `(async () => { try { await window.__TAURI_INTERNALS__.invoke('import_shortcuts', { path: ${JSON.stringify(shortcutExportPath)} }); return 'OK'; } catch (error) { return 'ERR:' + JSON.stringify(error); } })()`,
  );
  const shortcutBindingAfter = await evalMain2(
    `(async () => { const s = await window.__TAURI_INTERNALS__.invoke('get_settings'); return s.shortcuts.bindings.openFile; })()`,
  );
  chk(
    'E12 快捷键导入生效（openFile→Ctrl+Shift+O）',
    String(shortcutImportResult) === 'OK' && shortcutBindingAfter === 'Ctrl+Shift+O',
    `result=${shortcutImportResult} openFile=${shortcutBindingAfter}`,
  );

  const badShortcutPath = join(work, 'shortcuts-bad.json');
  writeFileSync(badShortcutPath, JSON.stringify({ bindings: { bogus: 'Ctrl+B' } }), 'utf8');
  const shortcutBadImport = await evalMain2(
    `(async () => { try { await window.__TAURI_INTERNALS__.invoke('import_shortcuts', { path: ${JSON.stringify(badShortcutPath)} }); return 'OK'; } catch (error) { return 'ERR:' + JSON.stringify(error); } })()`,
  );
  chk(
    'E13 未知动作拒绝且提示动作名',
    String(shortcutBadImport).includes('bogus'),
    String(shortcutBadImport).slice(0, 90),
  );

  console.log(`\n设置导入/导出/重置/迁移套件：通过 ${passed}/${passed + failed}`);
  process.exitCode = failed === 0 ? 0 : 1;
} catch (error) {
  console.error('套件异常：', error instanceof Error ? error.message : error);
  process.exitCode = 1;
} finally {
  try {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } catch {
    // 忽略
  }
  await delay(500);
  for (let i = 0; i < 10; i += 1) {
    try {
      rmSync(work, { recursive: true, force: true });
    } catch {
      // 忽略
    }
    await delay(250);
  }
  process.exit(process.exitCode ?? 0);
}
