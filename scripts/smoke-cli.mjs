// P3-3 命令行/单实例/粘贴打开 E2E（常驻套件）。
// 场景：
//   C1 启动携带文件参数 → 自动打开标签
//   C2 二次启动（带另一文件）→ 进程退出且首个实例新增标签（单实例转发）
//   C3 「打开剪贴板中的路径」→ 打开剪贴板文件
//   C4 截图
// 前置：npm run tauri build -- --debug --no-bundle（或 node scripts/dev.mjs build）
// 说明：工作目录位于项目内 tmp/（已 gitignore）；SRT_NO_ELEVATION=1 仅供自动化稳定。
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createClient, delay, dismissOnboarding, findTarget, waitForValue } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = resolve(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-cli-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(work, { recursive: true });
mkdirSync(dataDir, { recursive: true });
const fileA = join(work, 'cli-a.txt');
const fileB = join(work, 'cli-b.txt');
const fileC = join(work, 'cli-c.txt');
writeFileSync(fileA, 'alpha\nbeta\ngamma\n', 'utf8');
writeFileSync(fileB, 'bravo\n', 'utf8');
writeFileSync(fileC, 'charlie\n', 'utf8');
const port = 9600 + Math.floor(Math.random() * 300);

const env = {
  ...process.env,
  SRT_DATA_DIR: dataDir,
  SRT_NO_ELEVATION: '1',
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
};

const checks = [];
function check(name, ok, detail) {
  checks.push({ name, ok, detail });
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail === undefined ? '' : `  [${detail}]`}`);
}

const child = spawn(exe, [fileA], { env, stdio: 'ignore', detached: false });
let client = null;

const evalJs = async (expression) => {
  const result = await client.send('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) {
    throw new Error(`page eval 失败：${result.exceptionDetails.text}`);
  }
  return result.result?.value;
};

const invoke = (name, args) =>
  evalJs(
    `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)}, ${JSON.stringify(args ?? {})})`,
  );

const tabsNow = () => invoke('list_tabs');

async function clickAt(x, y) {
  await client.send('Input.dispatchMouseEvent', { type: 'mouseMoved', x, y });
  await client.send('Input.dispatchMouseEvent', {
    type: 'mousePressed',
    x,
    y,
    button: 'left',
    clickCount: 1,
  });
  await client.send('Input.dispatchMouseEvent', {
    type: 'mouseReleased',
    x,
    y,
    button: 'left',
    clickCount: 1,
  });
  await delay(120);
}

async function centerOf(expression) {
  return evalJs(`(() => {
    const el = ${expression};
    if (!el) return null;
    const rect = el.getBoundingClientRect();
    return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  })()`);
}

function killTree() {
  if (child.pid) spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
}

try {
  const wsUrl = await waitForValue(() => findTarget(port, null), 30_000, 400);
  client = await createClient(wsUrl);
  await waitForValue(
    () => evalJs('Boolean(window.__srt && document.querySelector(\'.menu-bar\'))'),
    30_000,
    400,
  );
  // 全新数据目录首启会出现使用向导模态：真实鼠标点击菜单前必须关闭（否则点击被遮挡）。
  await dismissOnboarding(evalJs);

  // C1 启动参数自动打开
  const tabs1 = await waitForValue(async () => {
    const view = await tabsNow();
    return view.tabs.some((tab) => tab.name === 'cli-a.txt') ? view : false;
  }, 20_000, 300);
  check('C1a 参数文件自动打开', tabs1.tabs.some((tab) => tab.name === 'cli-a.txt'), tabs1.tabs.length);
  let c3 = true;
  try {
    const pending = await invoke('take_cli_files');
    c3 = Array.isArray(pending) && pending.length === 0;
  } catch {
    c3 = false;
  }
  check('C1b 启动队列已排空', c3);

  // C2 二次启动转发
  const second = spawnSync(exe, [fileB], { env, timeout: 25_000 });
  check('C2a 二次启动进程退出', second.status === 0, `status=${second.status}`);
  const tabs2 = await waitForValue(async () => {
    const view = await tabsNow();
    return view.tabs.some((tab) => tab.name === 'cli-b.txt') ? view : false;
  }, 20_000, 300);
  check('C2b 首个实例新增转发标签', tabs2.tabs.some((tab) => tab.name === 'cli-b.txt'), tabs2.tabs.length);

  // C3 粘贴路径打开
  await invoke('plugin:clipboard-manager|write_text', { text: `${fileC}\r\n` });
  const menuRect = await centerOf(
    `[...document.querySelectorAll('.menu-bar button')].find((b) => b.textContent?.trim() === '文件')`,
  );
  if (!menuRect) throw new Error('未找到「文件」菜单');
  // 真实鼠标点击偶发未展开下拉（转发标签刷新与点击的时序竞争）：
  // 重试至多 3 次，每次等待下拉出现；仍失败才判 FAIL。
  let itemRect = null;
  for (let attempt = 0; attempt < 3 && !itemRect; attempt += 1) {
    await delay(200);
    await clickAt(menuRect.x, menuRect.y);
    itemRect = await waitForValue(
      () =>
        centerOf(
          `[...document.querySelectorAll('.menu-bar .item')].find((b) => b.textContent?.includes('打开剪贴板中的路径'))`,
        ),
      1500,
      200,
    );
  }
  check('C3a 菜单项存在', Boolean(itemRect));
  if (itemRect) {
    await clickAt(itemRect.x, itemRect.y);
    const tabs3 = await waitForValue(async () => {
      const view = await tabsNow();
      return view.tabs.some((tab) => tab.name === 'cli-c.txt') ? view : false;
    }, 20_000, 300);
    check('C3b 剪贴板路径打开为标签', tabs3.tabs.some((tab) => tab.name === 'cli-c.txt'), tabs3.tabs.length);
  } else {
    check('C3b 剪贴板路径打开为标签', false, '菜单项缺失');
  }

  // C4 截图
  const shot = await client.send('Page.captureScreenshot', { format: 'png' });
  if (shot?.data) {
    writeFileSync(
      resolve(root, 'docs', 'screenshots', 'phase-p33-cli.png'),
      Buffer.from(shot.data, 'base64'),
    );
  }
  check('C4 截图产出', Boolean(shot?.data));

  const failed = checks.filter((entry) => !entry.ok);
  console.log(`\n结果：${checks.length - failed.length}/${checks.length}`);
  if (failed.length > 0) {
    process.exitCode = 1;
  }
} catch (error) {
  console.error('未捕获错误：', error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
} finally {
  try {
    client?.close?.();
  } catch {
    // 忽略关闭失败
  }
  killTree();
  // WebView2 子进程随进程树回收需要时间：轮询删除，避免 EPERM 致套件在收尾崩溃
  for (let attempt = 0; attempt < 40 && existsSync(work); attempt += 1) {
    try {
      rmSync(work, { recursive: true, force: true });
    } catch {
      // 仍被占用：下轮重试
    }
    if (existsSync(work)) await delay(250);
  }
}
