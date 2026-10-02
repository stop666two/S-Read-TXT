# S-Read-TXT 配置说明（逐字段）

> 本文件是配置的权威说明，随实现阶段同步更新（项目规则：文档同步义务）。
> 应用强制便携模式：**所有数据只写入程序目录**（`SRT_DATA_DIR` 覆盖除外，见 §1）。程序目录不可写时启动会引导选择可写目录（**仅本次运行有效**；运行时覆盖优先级最高，`get_app_info` / `data_dir_status` 的 `origin` 会返回 `runtimeOverride`）。

## 1. 环境变量

| 名称 | 作用 | 类型 | 可填值 | 必填 | 默认 | 示例 |
|---|---|---|---|---|---|---|
| `SRT_DATA_DIR` | 覆盖数据目录（测试/特殊部署用；运行时的用户选择目录优先级更高） | 路径字符串 | 绝对路径 | 否 | 未设置（程序目录/data） | `D:\srt-data` |
| （运行时）| 不可写引导中选择的目录：经命令 `set_data_dir` 设置，会话级生效（重启后重新探测）；设置/历史/会话/日志全部改路 | 路径字符串 | 绝对路径且可写 | 否 | 未设置 | `D:\srt-data` |
| `SRT_LOG_LEVEL` | 覆盖日志级别（优先于 settings.json 的 `logLevel`；空白/非法值忽略并回退） | 枚举字符串 | `error` / `warn` / `info` / `debug`（大小写不敏感） | 否 | 未设置（读取 settings.logLevel） | `debug` |
| `SRT_NO_ELEVATION` | 跳过启动时的提权初始化（存在即生效）。默认行为：数据目录不可写且当前非管理员时，首次启动弹一次 UAC，由助手模式（`--prepare-data-dir` / `--grant-sid`）创建目录并 `icacls` 授予当前用户修改权限后立即退出；应用本体始终以普通权限运行，此后零提示。本变量用于不希望任何 UAC 提示的用户与自动化测试 | 开关（存在即跳过） | 任意非空值（约定 `1`） | 否 | 未设置（允许一次性提权初始化） | `1` |

说明：
- 应用**不读取** `.env` 文件；环境变量由启动环境（终端、快捷方式）提供。
- 生产排错建议临时开启 `debug`，问题定位后恢复 `info`（日志级别开关见设计文档 §10）。
- 权限行为完整说明见 README「安装与权限行为」：便携/「仅为我」安装零权限提示；「所有用户」安装启动时请求管理员（取消则进入数据目录引导）。

## 2. 数据文件（程序目录 `data/`）

### 2.1 `settings.json`（主配置）

| 字段 | 类型 | 可填值 | 默认 | 说明 |
|---|---|---|---|---|
| `schemaVersion` | number | 固定 `1` | `1` | 配置格式版本；将来迁移依据 |
| `logLevel` | string | `error`/`warn`/`info`/`debug` | `info` | 日志详细级别；环境变量可覆盖 |
| `maxFileSizeMB` | number | 1–2048 整数 | `100` | 可打开文件大小上限；超限提示固定文案，可在设置调整 |
| `maxTabs` | number | 1–200 整数 | `20` | 标签数量上限；超限打开被拒绝并提示 |
| `history.maxEntries` | number | 100–1000000 整数 | `10000` | 历史保留条数上限（超出裁剪最旧） |
| `history.retentionDays` | number | 1–36500 整数 | `365` | 历史保留天数（过期裁剪） |
| `saveBackupEnabled` | boolean | `true`/`false` | `true` | 首次保存前是否生成 `.bak` 备份 |
| `showOnboarding` | boolean | `true`/`false` | `true` | 是否显示首启引导；用户选择「不再显示」后置 `false` |

### 2.2 `reader.json`（阅读排版子配置）

| 字段 | 类型 | 可填值 | 默认 | 说明 |
|---|---|---|---|---|
| `schemaVersion` | number | 固定 `1` | `1` | 格式版本 |
| `theme` | string | `light`/`dark`/`eye`/`system` | `system` | 主题；`system` 跟随系统明暗解析 |
| `typography.fontFamily` | string | 系统已安装字体名 | `Microsoft YaHei` | 正文西文+中文主字体 |
| `typography.fontSize` | number | 8–72（px） | `16` | 正文字号 |
| `typography.lineHeight` | number | 1.0–3.2 | `1.8` | 行高倍数 |
| `typography.contentWidth` | number | 320–2400（px） | `720` | 正文限宽（约 40 汉字/行） |
| `typography.pagePadding` | number | 0–240（px） | `48` | 阅读区左右页边距 |
| `typography.pagePaddingY` | number | 0–240（px） | `48` | 阅读区上下留白（旧版配置缺此字段时取默认，向后兼容） |

### 2.3 `shortcuts.json`（快捷键绑定子配置）

| 字段 | 类型 | 可填值 | 默认 | 说明 |
|---|---|---|---|---|
| `schemaVersion` | number | 固定 `1` | `1` | 格式版本 |
| `bindings` | object | 动作 id → 组合键字符串 | 见下表 | 仅存**被修改过**的绑定；缺失动作使用默认值；恢复默认 = 清空覆盖项 |

组合键字符串格式：修饰键 `Ctrl`/`Shift`/`Alt`（`+` 连接）+ 主键（如 `Ctrl+Shift+H`、`F11`、`PgDn`）。
不可绑定项（固定键）：编辑标准键（`Ctrl+Z/Y/X/C/V/A`、方向键、编辑态 `Home/End`）、阅读态备用键（空格/Shift+空格翻页）、标签跳转（`Ctrl+1`~`9`）。

默认绑定表（动作 id 为稳定 ASCII 标识，随配置持久化；未列出的动作 id 视为未知并忽略）：

| 动作 id | 默认组合键 | 动作 id | 默认组合键 |
|---|---|---|---|
| `openFile` | `Ctrl+O` | `save` | `Ctrl+S` |
| `saveAs` | `Ctrl+Shift+S` | `toggleEdit` | `Ctrl+E` |
| `closeTab` | `Ctrl+W` | `nextTab` | `Ctrl+Tab` |
| `prevTab` | `Ctrl+Shift+Tab` | `pageDown` | `PgDn` |
| `pageUp` | `PgUp` | `firstLine` | `Home` |
| `lastLine` | `End` | `fullscreen` | `F11` |
| `find` | `Ctrl+F` | `replace` | `Ctrl+H` |
| `historyPanel` | `Ctrl+Shift+H` | | |

### 2.4 `session.json`（会话；退出时写入，启动时读取）

| 字段 | 类型 | 可填值 | 默认 | 说明 |
|---|---|---|---|---|
| `schemaVersion` | number | 固定 `1` | `1` | 格式版本 |
| `window.x` / `window.y` | number \| null | 屏幕坐标或 `null` | `null`（居中） | 窗口位置；`null` 或越界（按当前显示器判定）时居中 |
| `window.width` / `window.height` | number | 720–16384 | `1100×760` | 窗口尺寸；低于最小值/高于上限时回退默认（防手改配置导致窗口不可用） |
| `window.maximized` | boolean | `true`/`false` | `false` | 是否最大化启动 |
| `activeTabIndex` | number | ≥0 整数 | `0` | 活动标签下标；载入时收敛到 `tabs` 有效范围 |
| `tabs[]` | array | 见下 | `[]` | 上次打开的标签（惰性恢复：仅激活标签建索引） |
| `tabs[].path` | string | 文件绝对路径 | — | 文件不存在时启动跳过并 Toast 提示 |
| `tabs[].encoding` | string \| null | 编码名或 `null` | `null` | `null` = 自动检测；手动切换过则记录 |
| `tabs[].scrollRow` | number | ≥0 | `0` | 恢复的滚动锚点（显示行号） |
| `tabs[].editMode` | boolean | `true`/`false` | `false` | 上次是否处于编辑态（未保存内容不持久化） |

### 2.5 `history.jsonl`（历史；每行一条 JSON）

| 字段 | 类型 | 说明 |
|---|---|---|
| `path` | string | 文件绝对路径（去重键：同路径仅保留最新一条） |
| `name` | string | 文件名（展示用，避免 UI 再解析路径） |
| `size` | number | 文件大小（字节） |
| `encoding` | string | 最近一次打开的编码 |
| `openedAt` | string | 最近打开时间（RFC 3339 UTC） |
| `lastRow` | number | 上次阅读的显示行号（续读用） |
| `lastPercent` | number | 上次阅读进度（0–100，展示用） |

剪枝时机：启动时（条数 + 天数）；文件损坏行跳过并记录日志。

### 2.6 `logs/`（日志目录）

- `app.log`：JSON 行格式；字段：`time`（RFC 3339 带时区）、`level`（error/warn/info/debug，RFC 5424 命名）、`module`、`message`、`tab`/`req`（链路标识，可选）。
- 轮转：单文件 5MB，保留 3 份（`app.log`、`app.log.1`、`app.log.2`）。

## 3. `src-tauri/tauri.conf.json` 字段说明

| 字段 | 值 | 说明 |
|---|---|---|
| `productName` | `S-Read-TXT` | 安装后名称/产物名 |
| `version` | `0.0.1-beta` | 与 Cargo.toml、package.json 同步 |
| `identifier` | `com.sreadtxt.desktop` | 应用标识（ASCII 反向域名；不以 `.app` 结尾以避免 macOS 打包警告） |
| `build.beforeDevCommand` | `npm run dev` | 开发时先起 Vite |
| `build.devUrl` | `http://localhost:1420` | 与 vite.config.ts 端口一致（strictPort） |
| `build.beforeBuildCommand` | `npm run build` | 正式构建前先打包前端 |
| `build.frontendDist` | `../dist` | 前端产物目录 |
| `app.windows[0].label` | `main` | 主窗口标签（capability 匹配用） |
| `app.windows[0].width/height` | `1100×760` | 默认窗口尺寸 |
| `app.windows[0].minWidth/minHeight` | `720×480` | 最小尺寸（防布局塌陷） |
| `app.windows[0].center` | `true` | 首次居中 |
| `app.windows[0].dragDropEnabled` | `true` | 启用原生文件拖放（拖入打开 TXT） |
| `app.security.csp` | 见文件 | 仅允许自身资源；样式允许内联（动态 CSS 变量）；IPC 走 `ipc:`/`ipc.localhost` |
| `bundle.targets` | `["nsis"]` | 仅产 NSIS 安装器（体积最小，不产 MSI） |
| `bundle.windows.webviewInstallMode.type` | `skip` | 不捆绑、不下载 WebView2（完全离线；系统缺失时由系统提示） |
| `bundle.windows.nsis.languages` | `["SimpChinese"]` | 安装器中文界面 |
| `bundle.icon` | 4 项 | 由 `npx tauri icon` 生成（源图 `assets/icon-source.png`） |

## 4. `uno.config.ts` 颜色令牌

| 令牌（类名示例） | CSS 变量 | 语义 |
|---|---|---|
| `bg-base` | `--c-bg` | 页面背景 |
| `bg-surface` | `--c-surface` | 面板/表面背景 |
| `text-ink` | `--c-text` | 正文文字 |
| `text-muted` | `--c-muted` | 次要文字 |
| `border-line` | `--c-border` | 边框/分隔线 |
| `text-accent` / `bg-accent` | `--c-accent` | 强调色（交互/信息） |

主题切换由 `<html data-theme="light|dark|eye">` 驱动（CSS 变量替换），类名不重建。色值见设计文档 §8。

## 5. `package.json` / `Cargo.toml` 版本策略

- 全部依赖锁定精确版本：`package.json` 无 `^`/`~`（`.npmrc` 已设 `save-exact=true`）；`Cargo.toml` 使用 `=` 前缀。
- 锁文件（`package-lock.json`、`Cargo.lock`）必须提交且不手动修改。
