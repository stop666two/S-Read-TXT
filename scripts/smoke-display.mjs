// 显示选项 E2E（P2-2 V 组，常驻套件）：行号/相对行号/当前行高亮/标尺/缩进参考线/
// 不可见字符（空格·制表→行尾空白·换行）/自动换行关闭；编辑态光标行高亮。
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
// 说明：设置经 get_settings → save_settings 真实写入（触发 srt://settings-changed 广播，
// 与用户操作路径一致）；行数等断言均基于真实 DOM。
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

import {
  argValue,
  createClient,
  delay,
  dismissOnboarding,
  findTarget,
  openPathDone,
  waitForValue,
} from './lib/smoke-cdp.mjs';

const exePath = argValue(
  '--exe',
  resolve(process.cwd(), 'src-tauri', 'target', 'debug', 's-read-txt.exe'),
);
const workDir = join(tmpdir(), `srt-smoke-display-${Date.now()}`);
const dataDir = join(workDir, 'data');
const sample = join(workDir, 'display.txt');
mkdirSync(dataDir, { recursive: true });
// 行 0：空格/制表/行尾空白；行 1：缩进；行 2：普通
writeFileSync(sample, 'a b\tc  \n      indented\nplain\n', 'utf8');
const port = 9600 + Math.floor(Math.random() * 250);

const child = spawn(exePath, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: 'ignore',
});

let failed = 0;
function check(name, ok, detail = '') {
  const mark = ok ? 'PASS' : 'FAIL';
  if (!ok) failed += 1;
  console.log(`[${mark}] ${name}${detail ? ` — ${detail}` : ''}`);
}

const wsUrl = await findTarget(port);
const client = await createClient(wsUrl);
const evalJs = async (expression) => {
  const result = await client.send('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  return result?.result?.value;
};

async function setDisplay(patch) {
  return evalJs(`(async () => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    const snapshot = await invoke('get_settings');
    snapshot.app.display = Object.assign({}, snapshot.app.display, ${JSON.stringify(patch)});
    await invoke('save_settings', { request: {
      app: snapshot.app,
      reader: snapshot.reader,
      shortcuts: snapshot.shortcuts.bindings,
    } });
    return true;
  })()`);
}

async function shoot(name) {
  const shot = await client.send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(process.cwd(), 'docs', 'screenshots', name), Buffer.from(shot.data, 'base64'));
}

async function key(code, keyName) {
  await client.send('Input.dispatchKeyEvent', {
    type: 'rawKeyDown',
    windowsVirtualKeyCode: code,
    code: keyName,
    key: keyName,
  });
  await client.send('Input.dispatchKeyEvent', {
    type: 'keyUp',
    windowsVirtualKeyCode: code,
    code: keyName,
    key: keyName,
  });
}

try {
  await waitForValue(async () => (await evalJs('!!window.__srt && !!window.__srt.openPath')) || null, 20000);
  await dismissOnboarding(evalJs);
  await evalJs(openPathDone(sample));
  const rowsReady = await waitForValue(
    async () => (await evalJs(`document.querySelectorAll('.row[data-row]').length >= 3`)) || null,
    15000,
  );
  check('D0 样本打开且行已渲染', rowsReady === true);

  // ---- 行号 / 相对行号 ----
  await setDisplay({ lineNumbers: true, relativeLineNumbers: false });
  const ln1 = await waitForValue(
    async () => (await evalJs(`document.querySelector('[data-row="0"] .ln')?.textContent ?? ''`)) || null,
    8000,
  );
  check('D1 行号显示（首行 1）', ln1 === '1', String(ln1));
  const lnCount = await evalJs(
    `(() => { const rows = document.querySelectorAll('.row[data-row]').length; const lns = document.querySelectorAll('.row[data-row] .ln').length; return rows === lns && lns >= 3; })()`,
  );
  check('D2 每个渲染行都有行号', lnCount === true);

  await setDisplay({ relativeLineNumbers: true });
  const relLns = await waitForValue(async () => {
    const value = await evalJs(
      `[...document.querySelectorAll('.row[data-row] .ln')].slice(0, 3).map((el) => el.textContent).join(',')`,
    );
    return value === '1,1,2' ? value : null;
  }, 8000);
  check('D3 相对行号（阅读态参照首行：1,1,2）', relLns === '1,1,2', String(relLns));

  // ---- 当前行高亮 ----
  await setDisplay({ highlightCurrentLine: true });
  const current0 = await waitForValue(
    async () => (await evalJs(`!!document.querySelector('.row.current[data-row="0"]')`)) || null,
    8000,
  );
  check('D4 阅读态当前行高亮（首行）', current0 === true);

  // ---- 标尺 ----
  await setDisplay({ ruler: true, rulerPosition: 200 });
  const ruler = await waitForValue(async () => {
    const value = await evalJs(
      `(() => { const el = document.querySelector('[data-ruler]'); return el ? el.getAttribute('style') ?? '' : ''; })()`,
    );
    return value.includes('200px') ? value : null;
  }, 8000);
  check('D5 标尺出现且位置 200px', typeof ruler === 'string' && ruler.includes('200px'), String(ruler));

  // ---- 缩进参考线 ----
  await setDisplay({ indentGuides: true });
  const guides = await waitForValue(async () => {
    const value = await evalJs(`document.querySelectorAll('[data-row="1"] .guide').length`);
    return value === 1 ? value : null;
  }, 8000);
  check('D6 缩进参考线（6 空格 → 1 条 @4 列）', guides === 1, String(guides));

  // ---- 不可见字符 ----
  await setDisplay({ invisible: ['space', 'tab'] });
  const marked = await waitForValue(async () => {
    const value = await evalJs(
      `(() => { const el = document.querySelector('[data-row="0"] .txt'); return el ? el.textContent : ''; })()`,
    );
    return value.includes('·') && value.includes('→') ? value : null;
  }, 8000);
  check('D7 空格·/制表→ 标记（长度不变）', typeof marked === 'string' && marked.length === 7, JSON.stringify(marked));

  await setDisplay({ invisible: ['space', 'tab', 'trailingSpace'] });
  const trailing = await waitForValue(async () => {
    const value = await evalJs(`document.querySelector('[data-row="0"] .ts')?.textContent ?? ''`);
    return value.length > 0 ? value : null;
  }, 8000);
  check('D8 行尾空白单独着色片段', trailing !== null, JSON.stringify(trailing));

  await setDisplay({ invisible: ['newline'] });
  const nlCount = await waitForValue(async () => {
    const value = await evalJs(
      `(() => { const rows = document.querySelectorAll('.row[data-row]').length; const marks = document.querySelectorAll('.nl').length; return rows === marks && marks >= 3; })()`,
    );
    return value === true ? true : null;
  }, 8000);
  check('D9 换行标记 ¶（每个渲染行）', nlCount === true);

  // 全开截图（阅读态）
  await setDisplay({ invisible: ['space', 'tab', 'trailingSpace', 'newline'] });
  await delay(400);
  await shoot('p2-display-on.png');

  // ---- 自动换行关闭 ----
  await setDisplay({ wordWrap: false });
  const nowrapOn = await waitForValue(async () => {
    const value = await evalJs(
      `(() => { const page = document.querySelector('.page.nowrap'); if (!page) return ''; const row = document.querySelector('.row[data-row]'); return getComputedStyle(row).whiteSpace; })()`,
    );
    return value === 'pre' ? value : null;
  }, 8000);
  check('D10 关闭换行（white-space: pre）', nowrapOn === 'pre', String(nowrapOn));
  await delay(300);
  await shoot('p2-display-nowrap.png');
  await setDisplay({ wordWrap: true });

  // ---- 编辑态：光标行高亮 + 相对行号参照光标 ----
  await evalJs(
    `(() => { const btn = document.querySelector('[aria-label="切换编辑模式"]'); btn?.click(); return !!btn; })()`,
  );
  const editing = await waitForValue(
    async () => (await evalJs(`!!document.querySelector('textarea.input-proxy')`)) || null,
    8000,
  );
  check('D11 进入编辑模式', editing === true);
  await client.send('Runtime.evaluate', {
    expression: `document.querySelector('textarea.input-proxy')?.focus()`,
  });
  await key(40, 'ArrowDown');
  const editCurrent = await waitForValue(
    async () => (await evalJs(`document.querySelector('.row.current')?.dataset.row ?? ''`)) || null,
    8000,
  );
  check('D12 编辑态当前行跟随光标（行 1）', editCurrent === '1', String(editCurrent));
  const relEdit = await waitForValue(async () => {
    const value = await evalJs(
      `[...document.querySelectorAll('.row[data-row] .ln')].slice(0, 3).map((el) => el.textContent).join(',')`,
    );
    return value === '1,2,1' ? value : null;
  }, 8000);
  check('D13 相对行号参照光标（1,2,1）', relEdit === '1,2,1', String(relEdit));
  await delay(300);
  await shoot('p2-display-edit.png');

  // ---- 关闭全部（清理路径） ----
  await setDisplay({
    lineNumbers: false,
    relativeLineNumbers: false,
    highlightCurrentLine: false,
    ruler: false,
    indentGuides: false,
    invisible: [],
    wordWrap: true,
  });
  const cleared = await waitForValue(async () => {
    const value = await evalJs(
      `(() => { return !document.querySelector('.ln') && !document.querySelector('[data-ruler]') && !document.querySelector('.guide') && !document.querySelector('.page.nowrap'); })()`,
    );
    return value === true ? true : null;
  }, 8000);
  check('D14 全部关闭后清理干净', cleared === true);
} finally {
  try {
    client.close();
  } catch {
    // 忽略关闭异常
  }
  spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  await delay(400);
  rmSync(workDir, { recursive: true, force: true });
}

console.log(failed === 0 ? '\n显示选项套件：全部通过' : `\n显示选项套件：${failed} 项失败`);
process.exit(failed === 0 ? 0 : 1);
