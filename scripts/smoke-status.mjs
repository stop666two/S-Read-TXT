// 状态栏 v2 物理验证与演示（P2-1）：真实启动应用、真实操作界面、产出截图。
//
// 用法：node scripts/smoke-status.mjs [--exe path] [--keep-open]
//   --keep-open  验证后不关闭应用（维护者人工查看/操作）
// 前置：npm run tauri build -- --debug --no-bundle
//
// 步骤：
//   S1 启动（隔离数据目录）→ 打开演示文件 → 阅读态状态栏（行列/字数/进度/大小/编码/换行）
//   S2 进入编辑态 → Home + Shift+→×8 选区 → 行列 + 选中统计
//   S3 输入字符 → 修改标记 ●
//   S4 设置窗：阅读排版 → 状态栏分组（items/countMode/tabWidth/…）截图
//   S5 设置 countMode=byte → 主窗计数单位变为「字节」
//   S6 状态栏点击 LF → 菜单选 CRLF → 转换成功（状态更新 + Toast；Ctrl+Z 可撤销）
// 产物：docs/screenshots/p2-status-{read,edit,settings,byte,crlf}.png

import { spawn } from 'node:child_process';
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
} from './lib/smoke-cdp.mjs';

const root = resolve(process.cwd());
const exe = argValue('--exe', join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe'));
const keepOpen = process.argv.includes('--keep-open');
const shotDir = join(root, 'docs', 'screenshots');
mkdirSync(shotDir, { recursive: true });

const work = join(tmpdir(), `srt-status-${Date.now()}`);
mkdirSync(work, { recursive: true });
const dataDir = join(work, 'data');
const port = 9600 + Math.floor(Math.random() * 250);

const DEMO = [
  '第一章 春夜',
  '',
  '这是一段用于演示状态栏的示例文本。',
  '轻量、离线、数据随身。',
  '',
  'Lorem ipsum dolor sit amet.',
  '状态栏会显示：行列、字数、词数、进度、大小、编码、换行与修改标记。',
  '',
].join('\n');
const demoPath = join(work, '演示文本.txt');
writeFileSync(demoPath, DEMO, 'utf8');

const checks = [];
function check(name, ok, detail = '') {
  checks.push({ name, ok: !!ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`);
}

function shot(client, name) {
  return client.send('Page.captureScreenshot', { format: 'png' }).then((res) => {
    writeFileSync(join(shotDir, name), Buffer.from(res.data, 'base64'));
    console.log(`SHOT ${name}`);
  });
}

const child = spawn(exe, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: 'ignore',
  detached: true,
});
// 分离运行：--keep-open 时不阻塞 Node 退出（应用独立存活）；
// 其余情况保持引用，避免事件循环提前清空导致 unsettled await。
if (keepOpen) child.unref();

let killed = false;
function killTree() {
  if (killed || child.pid === undefined) return;
  killed = true;
  try {
    spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
  } catch {
    /* 忽略 */
  }
}

try {
  const target = await findTarget(port);
  const client = await createClient(target);
  const evalJs = async (expression) => {
    const res = await client.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (res.exceptionDetails) throw new Error(res.exceptionDetails.text ?? 'eval 异常');
    return res.result?.value;
  };
  const waitUntil = async (expression, timeoutMs = 10_000) => {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        if (await evalJs(expression)) return true;
      } catch {
        /* 重试 */
      }
      await delay(150);
    }
    return false;
  };
  const key = async (keyName, code, vk, modifiers = 0) => {
    await client.send('Input.dispatchKeyEvent', {
      type: 'keyDown',
      key: keyName,
      code,
      modifiers,
      windowsVirtualKeyCode: vk,
    });
    await client.send('Input.dispatchKeyEvent', { type: 'keyUp', key: keyName, code, modifiers });
  };

  // S0 就绪 + 首启引导关闭
  await waitUntil('!!window.__srt && !!document.querySelector("#app")', 20_000);
  await dismissOnboarding(evalJs);
  await evalJs(openPathDone(demoPath));
  await waitUntil('document.querySelectorAll(".row").length > 3', 10_000);
  await waitUntil('!!document.querySelector(".status-bar")?.innerText?.includes("字")', 10_000);

  // S1 阅读态状态栏
  const readText = await evalJs('document.querySelector(".status-bar").innerText');
  check('S1a 阅读态含行列（第 N 行）', /第\s*1\s*行/.test(readText), readText);
  check('S1b 阅读态含字数单位', readText.includes('字'), readText);
  check('S1c 阅读态含进度与编码与换行', /阅读|%/.test(readText) && readText.includes('UTF-8') && readText.includes('LF'));
  await shot(client, 'p2-status-read.png');

  // S2 进入编辑态 + 选区统计
  await waitUntil(
    '(() => { const b = document.querySelector(\'[aria-label="切换编辑模式"]\'); return !!b && !b.disabled; })()',
    8000,
  );
  await evalJs('document.querySelector(\'[aria-label="切换编辑模式"]\').click()');
  await waitUntil('!!document.querySelector("textarea.input-proxy")', 8_000);
  await evalJs('document.querySelector("textarea.input-proxy").focus()');
  await delay(200);
  await key('Home', 'Home', 36);
  for (let i = 0; i < 4; i += 1) await key('ArrowRight', 'ArrowRight', 39, 8);
  await delay(600);
  const editText = await evalJs('document.querySelector(".status-bar").innerText');
  check('S2a 编辑态含行列（r:c）', /1:5/.test(editText), editText);
  check('S2b 编辑态含选中统计', editText.includes('选中'), editText);
  await shot(client, 'p2-status-edit.png');

  // S3 输入 → 修改标记（insertText 走真实编辑管线：beforeinput/input）
  await client.send('Input.insertText', { text: 'x' });
  await delay(500);
  const dirtyText = await evalJs('document.querySelector(".status-bar").innerText');
  check('S3a 修改标记出现', dirtyText.includes('●'), dirtyText);
  await shot(client, 'p2-status-dirty.png');

  // S4 设置窗状态栏分组
  await evalJs('window.__TAURI_INTERNALS__.invoke("open_settings", { tab: "typography" })');
  const settingsTarget = await findTarget(port, 'settings.html');
  const settings = await createClient(settingsTarget);
  const evalSet = async (expression) => {
    const res = await settings.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (res.exceptionDetails) throw new Error(res.exceptionDetails.text ?? 'eval 异常');
    return res.result?.value;
  };
  const waitSet = async (expression, timeoutMs = 10_000) => {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      try {
        if (await evalSet(expression)) return true;
      } catch {
        /* 重试 */
      }
      await delay(150);
    }
    return false;
  };
  await waitSet('!!document.querySelector(\'[data-setting-group="app.status"]\')', 10_000);
  await evalSet(
    'document.querySelector(\'[data-setting-group="app.status"]\')?.scrollIntoView({ block: "center" })',
  );
  await delay(300);
  const groupOk = await evalSet(
    'document.querySelectorAll(\'[data-setting^="app.status.items."]\').length >= 7',
  );
  check('S4a 设置窗状态栏分组可见（items 控件）', groupOk === true);
  await shot(settings, 'p2-status-settings.png');

  // S5 重新选区（计数显示依赖选区）→ 切换 countMode=byte → 主窗单位变化
  await evalJs('document.querySelector("textarea.input-proxy").focus()');
  await key('Home', 'Home', 36);
  for (let i = 0; i < 4; i += 1) await key('ArrowRight', 'ArrowRight', 39, 8);
  await delay(600);
  await evalSet(`(() => {
    const sel = document.querySelector('[data-setting="app.status.countMode"]');
    sel.value = 'byte';
    sel.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  })()`);
  const byteOk = await waitUntil(
    'document.querySelector(".status-bar").innerText.includes("字节")',
    8_000,
  );
  check('S5a 计数单位切换为字节', byteOk === true);
  await shot(client, 'p2-status-byte.png');

  // S6 换行符转换 LF → CRLF
  await evalJs(
    '[...document.querySelectorAll(".status-bar button")].find((b) => b.innerText.trim() === "LF")?.click()',
  );
  await waitUntil('!!document.querySelector(".eol-item")', 5000);
  await evalJs('[...document.querySelectorAll(".eol-item")].find((b) => b.innerText.includes("CRLF"))?.click()');
  const crlfOk = await waitUntil(
    'document.querySelector(".status-bar").innerText.includes("CRLF")',
    8_000,
  );
  check('S6a 换行符转换为 CRLF', crlfOk === true);
  await shot(client, 'p2-status-crlf.png');

  const failed = checks.filter((entry) => !entry.ok);
  console.log(`\n结果：${checks.length - failed.length}/${checks.length} 通过`);
  if (failed.length > 0) process.exitCode = 2;

  if (keepOpen) {
    console.log('应用保持打开（--keep-open）：请人工查看主窗与设置窗。');
    console.log(`EXE_PID=${child.pid}`);
    process.exit(failed.length > 0 ? 2 : 0);
  } else {
    killTree();
    await delay(1000);
    rmSync(work, { recursive: true, force: true });
  }
} catch (error) {
  console.error('物理验证失败：', error);
  process.exitCode = 1;
  if (!keepOpen) {
    killTree();
    await delay(800);
    rmSync(work, { recursive: true, force: true });
  }
}
