// S-Read-TXT 系统集成 E2E（设置面板驱动）。
// 目的：验证「设置 → 系统集成」的真实交互与注册表效果（当前用户免管理员；全局路径仅验证确认与取消）。
// 流程：预清理（--integration-write user 0 0 0）→ 启动应用（隔离数据目录）
//   → 打开设置窗口 → 系统页 → 集成区块可见
//   → 勾选三项并「应用到当前用户」→ 断言 UI 状态徽标 + 注册表键（ProgID/打开方式/右键/默认值）
//   → 「从当前用户注销」→ 断言注册表归零与默认值复位
//   → 「应用到所有用户」→ 断言弹出管理员确认框 → 取消 → 无变更（不触发 UAC）
// 前置：已构建 debug 可执行文件（`npm run tauri build -- --debug --no-bundle`）。
// 用法：node scripts/smoke-integration.mjs

import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { argValue, createClient, delay, dismissOnboarding, findTarget } from './lib/smoke-cdp.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const exe = join(root, 'src-tauri', 'target', 'debug', 's-read-txt.exe');
const work = join(root, 'tmp', `e2e-integration-${Date.now()}`);
mkdirSync(work, { recursive: true });
const dataDir = join(work, 'data');
mkdirSync(dataDir, { recursive: true });
writeFileSync(join(dataDir, 'settings.json'), JSON.stringify({ schemaVersion: 17 }), 'utf8');
const port = Number(argValue('port', String(9900 + Math.floor(Math.random() * 80))));

let passed = 0;
let failed = 0;
const failures = [];
const check = (name, ok, extra = '') => {
  if (ok) {
    passed += 1;
    console.log(`PASS  ${name}${extra ? `  ← ${extra}` : ''}`);
  } else {
    failed += 1;
    failures.push(name);
    console.log(`FAIL  ${name}${extra ? `  ← ${extra}` : ''}`);
  }
};

const evalIn = async (client, expression) => {
  const result = await client.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(JSON.stringify(result.exceptionDetails).slice(0, 400));
  }
  return result.result.value;
};

const waitFor = async (client, expression, predicate, timeoutMs = 8000) => {
  const deadline = Date.now() + timeoutMs;
  let last;
  while (Date.now() < deadline) {
    last = await evalIn(client, expression);
    if (predicate(last)) return last;
    await delay(150);
  }
  return last;
};

/** 注册表项是否存在。 */
const regExists = (path) => spawnSync('reg', ['query', path], { stdio: 'ignore' }).status === 0;
/** 读取默认值（不存在或“数值未设置”返回 ''）。 */
const regDefault = (path) => {
  const result = spawnSync('reg', ['query', path, '/ve'], { encoding: 'utf8' });
  if (result.status !== 0) return '';
  const line = result.stdout.split(/\r?\n/).find((item) => item.includes('REG_SZ'));
  if (!line) return '';
  const value = line.split('REG_SZ')[1]?.trim() ?? '';
  return value.startsWith('(') ? '' : value;
};

// ---- 预清理：确保起点干净（用户级） ----
spawnSync(exe, ['--integration-write', 'user', '0', '0', '0'], { stdio: 'ignore' });
check('I0 预清理后无 ProgID 键', !regExists('HKCU\\Software\\Classes\\SReadTXT.txt'));

const child = spawn(exe, [], {
  env: {
    ...process.env,
    SRT_DATA_DIR: dataDir,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: 'ignore',
});

try {
  const mainWs = await findTarget(port, 'tauri.localhost');
  const main = await createClient(mainWs);
  await dismissOnboarding((expression) => evalIn(main, expression));
  await delay(400);

  // 打开设置窗口 → 系统页
  await waitFor(main, `!!document.querySelector('button[title="设置"]')`, (v) => v === true);
  await evalIn(main, `(() => { document.querySelector('button[title="设置"]').click(); return true; })()`);
  const settingsWs = await findTarget(port, 'settings.html');
  const settings = await createClient(settingsWs);
  await settings.send('Page.enable');
  await waitFor(
    settings,
    `[...document.querySelectorAll('.tabs [role="tab"]')].some((b) => b.textContent.trim() === '系统')`,
    (v) => v === true,
  );
  await evalIn(
    settings,
    `(() => { [...document.querySelectorAll('.tabs [role="tab"]')].find((b) => b.textContent.trim() === '系统').click(); return true; })()`,
  );

  const sectionReady = await waitFor(
    settings,
    `!!document.querySelector('[data-integration-txt]')`,
    (v) => v === true,
    10_000,
  );
  check('I1 设置窗口显示集成区块', sectionReady === true);

  const chipOn = async (chip) =>
    (await evalIn(
      settings,
      `document.querySelector('[data-integration-chip="${chip}"]')?.getAttribute('data-on')`,
    )) === 'true';

  // 勾选三项 → 应用到当前用户
  await evalIn(
    settings,
    `(() => {
      for (const id of ['txt', 'log', 'menu']) {
        const box = document.querySelector('[data-integration-' + id + ']');
        if (box && !box.checked) box.click();
      }
      return true;
    })()`,
  );
  await evalIn(settings, `(() => { document.querySelector('[data-integration-apply-user]').click(); return true; })()`);
  await waitFor(
    settings,
    `document.querySelector('[data-integration-chip="user-txt"]')?.getAttribute('data-on')`,
    (v) => v === 'true',
  );
  check(
    'I2 应用后用户级状态=已登记',
    (await chipOn('user-txt')) && (await chipOn('user-log')) && (await chipOn('user-menu')),
  );
  check('I3 注册表：ProgID 已创建', regExists('HKCU\\Software\\Classes\\SReadTXT.txt\\shell\\open\\command'));
  check(
    'I4 注册表：打开方式条目已创建',
    regExists('HKCU\\Software\\Classes\\Applications\\s-read-txt.exe\\shell\\open\\command'),
  );
  check(
    'I5 注册表：右键菜单已创建',
    regExists('HKCU\\Software\\Classes\\SystemFileAssociations\\.txt\\shell\\S-Read-TXT\\command') &&
      regExists('HKCU\\Software\\Classes\\SystemFileAssociations\\.log\\shell\\S-Read-TXT\\command'),
  );
  check('I6 注册表：.txt 默认值登记为本应用', regDefault('HKCU\\Software\\Classes\\.txt') === 'SReadTXT.txt');

  // 注销（当前用户）
  await evalIn(settings, `(() => { document.querySelector('[data-integration-remove-user]').click(); return true; })()`);
  await waitFor(
    settings,
    `document.querySelector('[data-integration-chip="user-txt"]')?.getAttribute('data-on')`,
    (v) => v === 'false',
  );
  check(
    'I7 注销后用户级状态=未登记',
    !(await chipOn('user-txt')) && !(await chipOn('user-log')) && !(await chipOn('user-menu')),
  );
  check('I8 注册表：ProgID 已删除', !regExists('HKCU\\Software\\Classes\\SReadTXT.txt'));
  check('I9 注册表：打开方式条目已删除', !regExists('HKCU\\Software\\Classes\\Applications\\s-read-txt.exe'));
  check(
    'I10 注册表：右键菜单已删除',
    !regExists('HKCU\\Software\\Classes\\SystemFileAssociations\\.txt\\shell\\S-Read-TXT') &&
      !regExists('HKCU\\Software\\Classes\\SystemFileAssociations\\.log\\shell\\S-Read-TXT'),
  );
  check('I11 注册表：.txt 默认值已复位', regDefault('HKCU\\Software\\Classes\\.txt') === '');

  // 全局路径：点击后弹确认框 → 取消（不触发 UAC）
  await evalIn(settings, `(() => { document.querySelector('[data-integration-apply-machine]').click(); return true; })()`);
  const dialog = await waitFor(
    settings,
    `document.querySelector('.dialog[role="alertdialog"]')?.textContent ?? ''`,
    (v) => typeof v === 'string' && v.includes('管理员'),
  );
  check(
    'I12 全局注册弹出管理员确认框',
    typeof dialog === 'string' && dialog.includes('管理员'),
    String(dialog).slice(0, 40),
  );
  await evalIn(
    settings,
    `(() => {
      const dialog = document.querySelector('.dialog[role="alertdialog"]');
      const cancel = [...dialog.querySelectorAll('button')].find((b) => /取消/.test(b.textContent));
      cancel.click();
      return true;
    })()`,
  );
  await delay(300);
  const dialogGone = await evalIn(settings, `!document.querySelector('.dialog[role="alertdialog"]')`);
  check(
    'I13 取消后确认框关闭且无变更',
    dialogGone === true && !(await chipOn('machine-txt')) && !regExists('HKLM\\Software\\Classes\\SReadTXT.txt'),
  );

  console.log(`\n系统集成冒烟：${passed}/${passed + failed} 通过`);
  if (failed > 0) {
    console.log(`失败项：${failures.join('；')}`);
    process.exitCode = 1;
  }
} catch (error) {
  console.error(`未捕获错误：${error.message}`);
  process.exitCode = 1;
} finally {
  try {
    if (child.pid) {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' });
    }
  } catch {
    // 忽略清理失败
  }
  // 兜底清理（避免探针中断残留）
  spawnSync(exe, ['--integration-write', 'user', '0', '0', '0'], { stdio: 'ignore' });
}

process.exit(process.exitCode ?? 0);
