// P3-2 E2E（新建 / 导出 / 打印）：真实应用 + CDP。
// 用法：node scripts/smoke-p32.mjs [--exe path]
// 前置：npm run tauri build -- --debug --no-bundle（或 node scripts/dev.mjs build）
// 说明：SRT_PRINT_NO_AUTO=1 禁用打印页自动弹窗（自动化专用）；数据目录位于项目内 tmp/（已 gitignore）。
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

import { argValue, createClient, delay, dismissOnboarding, findTarget, waitForValue } from './lib/smoke-cdp.mjs';
import { createDialogOps } from './lib/dialog.mjs';

const root = resolve(process.cwd());
const exe = argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe'));
const work = join(root, 'tmp', `e2e-p32-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
const savedPath = join(work, 'untitled-saved.txt');
const exportHtml = join(work, 'export.html');
const exportJson = join(work, 'export.json');
const shot = join(root, 'docs', 'screenshots', 'p3-p32.png');
const port = 10600 + Math.floor(Math.random() * 200);

let checks = 0;
let failed = 0;
function check(name, ok, detail = '') {
  checks += 1;
  if (ok) {
    console.log(`  ok ${name}`);
  } else {
    failed += 1;
    console.log(`  FAIL ${name}${detail ? ` :: ${String(detail).slice(0, 300)}` : ''}`);
  }
}

const child = spawn(exe, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
    SRT_NO_ELEVATION: '1',
    SRT_PRINT_NO_AUTO: '1',
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: 'ignore',
});
let exitCode = 1;

function killTree() {
  spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
}

async function main() {
  const wsUrl = await waitForValue(() => findTarget(port, null), 30000);
  const client = await createClient(wsUrl);
  const evalJs = async (expression) => {
    const result = await client.send('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    return result?.result?.value;
  };
  await waitForValue(async () => (await evalJs('!!(window.__srt && document.querySelector(".menu-bar"))')) === true, 30000);
  await dismissOnboarding(evalJs);

  const invoke = async (cmd, args = {}) =>
    evalJs(
      `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)}).then((value) => JSON.stringify(value ?? null)).catch((error) => 'ERR:' + JSON.stringify(error))`,
    );
  const invokeValue = async (cmd, args = {}) => {
    const raw = await invoke(cmd, args);
    if (typeof raw === 'string' && raw.startsWith('ERR:')) throw new Error(raw);
    return raw === null ? null : JSON.parse(raw);
  };

  // N1 新建文件（真实菜单路径：文件 → 新建文件）
  await evalJs(
    `(() => { const b=[...document.querySelectorAll('.menu-bar button')].find((x)=>x.textContent.includes('文件')); b?.click(); return true; })()`,
  );
  await delay(200);
  await evalJs(
    `(() => { const b=[...document.querySelectorAll('.dropdown button')].find((x)=>x.textContent.includes('新建文件')); b?.click(); return true; })()`,
  );
  await delay(500);
  const tabsAfterNew = await invokeValue('list_tabs');
  const tab = tabsAfterNew.tabs[tabsAfterNew.tabs.length - 1];
  const tabNameShown = await waitForValue(async () => {
    const text = await evalJs(`(document.querySelector('.tab-bar .name')?.textContent ?? '')`);
    return String(text).includes('未命名') ? text : null;
  }, 6000);
  check('N1a 标签显示「未命名」', typeof tabNameShown === 'string', tabNameShown);
  check('N1b 新建即编辑态', tab.editing === true);
  check('N1c 未命名标记存在', typeof tab.untitled === 'number', tab.untitled);

  // N1d 截图（未命名标签 + 编辑区）
  await client.send('Page.bringToFront');
  await delay(400);
  const shotData = await client.send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(shot, Buffer.from(shotData.data, 'base64'));
  check('N1d 截图已保存', existsSync(shot), shot);

  // N2 输入 → 变脏（等待隐藏输入框获得焦点后 CDP 物理输入）
  await evalJs(`(document.querySelector('textarea.input-proxy')?.focus(), true)`);
  await waitForValue(
    async () => (await evalJs(`document.activeElement?.classList.contains('input-proxy') ?? false`)) || null,
    6000,
  );
  await client.send('Input.insertText', { text: 'hello p32' });
  const dirtyOk = await waitForValue(async () => {
    const view = await invokeValue('list_tabs');
    return view.tabs.find((item) => item.tabId === tab.tabId)?.dirty === true ? true : null;
  }, 7000);
  check('N2 输入后变脏', dirtyOk === true);

  // N3 Ctrl+S → 重定向到另存为原生对话框（取消）
  const dlg = createDialogOps(child.pid);
  await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 's', code: 'KeyS', windowsVirtualKeyCode: 83, modifiers: 2 });
  await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 's', code: 'KeyS', windowsVirtualKeyCode: 83, modifiers: 2 });
  let saveDialogOpened = await dlg.waitDialog(true, 10000);
  if (!saveDialogOpened) {
    // 首轮可能因输入法/时序未到达全局监听：再发一次
    await client.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 's', code: 'KeyS', windowsVirtualKeyCode: 83, modifiers: 2 });
    await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key: 's', code: 'KeyS', windowsVirtualKeyCode: 83, modifiers: 2 });
    saveDialogOpened = await dlg.waitDialog(true, 12000);
  }
  if (saveDialogOpened) dlg.dialogOp('close');
  check('N3a Ctrl+S 打开原生另存为对话框', saveDialogOpened === true);
  await delay(400);

  // N4 直接另存为（绕过原生对话框）→ 名称切换、未命名标记清除、磁盘可见
  await invokeValue('save_tab_as', { tabId: tab.tabId, newPath: savedPath, encoding: null, makeBackup: false });
  const tabsAfterSave = await invokeValue('list_tabs');
  const saved = tabsAfterSave.tabs.find((item) => item.tabId === tab.tabId);
  check('N4a 标签名变为文件名', saved?.name === 'untitled-saved.txt', saved?.name);
  check('N4b 未命名标记清除', saved?.untitled == null);
  check('N4c 磁盘文件生成且内容正确', existsSync(savedPath) && readFileSync(savedPath, 'utf8').includes('hello p32'));

  // N5 导出 html / json
  const htmlBytes = await invokeValue('export_text', { tabId: tab.tabId, path: exportHtml });
  check('N5a 导出 HTML 字节数 > 0', Number(htmlBytes) > 0, htmlBytes);
  const htmlText = readFileSync(exportHtml, 'utf8');
  check('N5b HTML 含转义行内容', htmlText.includes('<pre>') && htmlText.includes('hello p32'));
  await invokeValue('export_text', { tabId: tab.tabId, path: exportJson });
  const jsonParsed = JSON.parse(readFileSync(exportJson, 'utf8'));
  check('N5c JSON 为行数组且含内容', Array.isArray(jsonParsed) && jsonParsed.join('\n').includes('hello p32'));

  // N6 打印窗口（打印页自动弹窗已由 SRT_PRINT_NO_AUTO 关闭）
  await invokeValue('print_document', { tabId: tab.tabId });
  const printTarget = await findTarget(port, 'data:text/html', 10000).catch(() => null);
  check('N6 打印预览窗口出现', Boolean(printTarget));

  // N7 未命名文件的临时文件清理（另存为后即清理/无残留）
  const untitledDir = join(dataDir, 'untitled');
  const leftovers = existsSync(untitledDir) ? readdirSync(untitledDir) : [];
  check('N7 未命名临时文件无残留', leftovers.length === 0, JSON.stringify(leftovers));

  console.log(`\nsmoke-p32 完成：${checks - failed}/${checks} 通过`);
  exitCode = failed === 0 ? 0 : 1;
}

try {
  await main();
} catch (error) {
  console.error('smoke-p32 未捕获错误：', error);
  exitCode = 1;
} finally {
  killTree();
  await delay(600);
  try {
    rmSync(work, { recursive: true, force: true });
  } catch {
    // 清理失败留待手工处理（tmp/ 已忽略）
  }
  process.exitCode = exitCode;
}
