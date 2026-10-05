// 工具功能 E2E（P3-6 常驻套件）。
// 场景：
//   U1 首运异常提示：无快照时不提示；存在可恢复快照时提示
//   U2 拆分行数模式（后续任务补入）
//   U3 拆分预览后取消不写盘
//   U4 拆分标记模式
//   U5 拆分非法正则提示
//   U6 批量重命名扫描与冲突预览
//   U7 重命名执行与撤销
//   U8 工具菜单入口
// 依赖：debug 构建（npm run tauri build -- --debug --no-bundle）。
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
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
const work = join(root, 'tmp', `e2e-utility-${Date.now()}`);
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });

// 端口段避开 Windows 保留区间 10008–10107，并与既有套件错开
const port = 9450 + Math.floor(Math.random() * 80);

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

const killAll = () => {
  try {
    spawnSync('taskkill', ['/IM', 's-read-txt.exe', '/T', '/F'], { stdio: 'ignore' });
  } catch {
    /* 忽略 */
  }
};

const watchdog = setTimeout(() => {
  console.log(`FAIL  看门狗超时（240s，当前步骤：${step}）`);
  process.exitCode = 4;
  killAll();
}, 240_000);

killAll();
await delay(600);

const evalIn = async (client, expression) => {
  const result = await client.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result?.value;
};

const launch = async (dir) => {
  child = spawn(exe, [], {
    env: {
      ...process.env,
      SRT_DATA_DIR: dir,
      SRT_NO_ELEVATION: '1',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    },
    stdio: 'ignore',
  });
  await findTarget(port);
  const client = await createClient(await findTarget(port));
  await waitForValue(
    async () => ((await evalIn(client, '!!window.__srt?.openPath')) ? true : null),
    20000,
  );
  await dismissOnboarding((expr) => evalIn(client, expr));
  return client;
};

const stopApp = async () => {
  killAll();
  for (let i = 0; i < 40; i += 1) {
    const out = spawnSync('tasklist', ['/FI', 'IMAGENAME eq s-read-txt.exe', '/NH'], {
      encoding: 'utf8',
    });
    if (!String(out.stdout).includes('s-read-txt.exe')) return;
    await delay(250);
  }
};

/** 页面内全部 toast 文本 */
const toastText = (client) =>
  evalIn(
    client,
    `[...document.querySelectorAll('.toast .text')].map((n) => n.textContent).join('|')`,
  );

try {
  // ---- U1 首运异常提示 ----
  step = 'U1 首运异常提示';
  let client = await launch(dataDir);
  await delay(2200);
  const firstToasts = await toastText(client);
  check('U1a 全新数据目录首启不提示异常退出', !String(firstToasts).includes('异常退出'), `toasts=${firstToasts}`);
  await stopApp();

  // 制造「异常退出 + 存在快照」：写快照文件且不写 .clean-exit
  const snapDir = join(dataDir, 'snapshots', 'deadbeef');
  mkdirSync(snapDir, { recursive: true });
  writeFileSync(join(snapDir, '1700000000000-0001.snap'), 'crash-recover\n');

  client = await launch(dataDir);
  const recovered = await waitForValue(async () => {
    const text = await toastText(client);
    return String(text).includes('异常退出') ? text : null;
  }, 6000);
  check('U1b 存在可恢复快照时提示异常退出', recovered !== null, `toasts=${recovered ?? ''}`);
  await stopApp();
} catch (error) {
  failed += 1;
  console.log(`FAIL  未预期异常：${error?.message ?? error}（当前步骤：${step}）`);
} finally {
  clearTimeout(watchdog);
  killAll();
}

console.log(`\n工具功能冒烟：${passed}/${passed + failed} 通过`);
if (failed > 0) {
  console.log(`工作目录保留：${work}`);
  process.exitCode = 1;
} else {
  try {
    rmSync(work, { recursive: true, force: true });
  } catch {
    console.log(`工作目录占用中，保留（${work}）`);
  }
}
