# S-Read-TXT 配置说明（逐字段）

> 本文件是配置的权威说明，随实现阶段同步更新（项目规则：文档同步义务）。
> 应用强制便携模式：**所有数据只写入程序目录**（`SRT_DATA_DIR` 覆盖与数据目录迁移除外，见 §1 与 §2.8）。程序目录不可写时启动会引导选择可写目录（**仅本次运行有效**）；也可通过「设置 → 常规 → 数据位置 → 迁移」把数据迁移到自定义目录（写入程序目录 `config.json` 指针，重启生效）。目录解析优先级：运行时覆盖 > `SRT_DATA_DIR` > 指针文件 > 程序目录/data；`get_app_info` / `data_dir_status` 的 `origin` 返回 `runtimeOverride` / `envOverride` / `persisted` / `portable`。

## 1. 环境变量

| 名称 | 作用 | 类型 | 可填值 | 必填 | 默认 | 示例 |
|---|---|---|---|---|---|---|
| `SRT_DATA_DIR` | 覆盖数据目录（测试/特殊部署用；运行时选择 > 本变量 > 迁移指针 > 便携目录） | 路径字符串 | 绝对路径 | 否 | 未设置（程序目录/data） | `D:\srt-data` |
| （运行时）| 不可写引导中选择的目录：经命令 `set_data_dir` 设置，会话级生效（重启后重新探测）；设置/历史/会话/日志全部改路 | 路径字符串 | 绝对路径且可写 | 否 | 未设置 | `D:\srt-data` |
| `SRT_LOG_LEVEL` | 覆盖日志级别（优先于 settings.json 的 `logLevel`；空白/非法值忽略并回退） | 枚举字符串 | `error` / `warn` / `info` / `debug`（大小写不敏感） | 否 | 未设置（读取 settings.logLevel） | `debug` |
| `SRT_NO_ELEVATION` | 跳过启动时的提权初始化（存在即生效）。默认行为：数据目录不可写且当前非管理员时，首次启动弹一次 UAC，由助手模式（`--prepare-data-dir` / `--grant-sid`）创建目录并 `icacls` 授予当前用户修改权限后立即退出；应用本体始终以普通权限运行，此后零提示。本变量用于不希望任何 UAC 提示的用户与自动化测试 | 开关（存在即跳过） | 任意非空值（约定 `1`） | 否 | 未设置（允许一次性提权初始化） | `1` |
| `SRT_MSYS_BIN` | 仅自检脚本（`scripts/verify-all.mjs`）使用：MSYS2 工具链目录（防旧 DLL 遮蔽）；未设置时自动从 PATH 探测以 `ucrt64\bin` 结尾的条目（CI 无需） | 路径字符串 | 绝对路径 | 否 | 未设置（自动探测） | 按实际安装位置 |

说明：
- 应用**不读取** `.env` 文件；环境变量由启动环境（终端、快捷方式）提供。
- 生产排错建议临时开启 `debug`，问题定位后恢复 `info`（日志级别开关见设计文档 §10）。
- 权限行为完整说明见 README「安装与权限行为」：便携/「仅为我」安装零权限提示；「所有用户」安装启动时请求管理员（取消则进入数据目录引导）。

## 2. 数据文件（程序目录 `data/`）

### 2.1 `settings.json`（主配置）

| 字段 | 类型 | 可填值 | 默认 | 说明 |
|---|---|---|---|---|
| `schemaVersion` | number | 固定 `3` | `3` | 配置格式版本（当前 v3；启动自动迁移旧版，见 §2.7） |
| `logLevel` | string | `error`/`warn`/`info`/`debug` | `info` | 日志详细级别；环境变量可覆盖 |
| `maxFileSizeMB` | number | 1–2048 整数 | `100` | 只读阈值：超过此大小以只读模式打开（可浏览、不可编辑；状态栏显示「只读」、编辑入口禁用并提示） |
| `hardLimitMB` | number | 100–16384 整数 | `2048` | 硬上限：超过此大小直接拒绝打开（沿用逐字提示「很抱歉，文件过大无法打开，可以在设置里面调整。」）；低于只读阈值时自动修正为只读阈值 |
| `maxTabs` | number | 1–200 整数 | `20` | 标签数量上限；超限打开被拒绝并提示 |
| `history.maxEntries` | number | 100–1000000 整数 | `10000` | 历史保留条数上限（超出裁剪最旧） |
| `history.retentionDays` | number | 1–36500 整数 | `365` | 历史保留天数（过期裁剪） |
| `saveBackupEnabled` | boolean | `true`/`false` | `true` | 首次保存前是否生成 `.bak` 备份 |
| `showOnboarding` | boolean | `true`/`false` | `true` | 是否显示首启引导；用户选择「不再显示」后置 `false` |
| `locale` | string | `zh-CN` / `en` | `zh-CN` | 界面语言（BCP 47 标签；未知值归一为默认；即时切换） |
| `startup.restoreSession` | boolean | `true`/`false` | `true` | 启动时恢复上次会话（标签与阅读位置） |
| `startup.restoreWindow` | boolean | `true`/`false` | `true` | 启动时恢复窗口位置与大小；关闭后使用默认几何（居中 1100×760） |

### 2.2 `reader.json`（阅读排版子配置）

| 字段 | 类型 | 可填值 | 默认 | 说明 |
|---|---|---|---|---|
| `schemaVersion` | number | 固定 `3` | `3` | 配置格式版本（当前 v3；启动自动迁移旧版，见 §2.7） |
| `theme` | string | `system` / `light` / `dark` / `eye-green` / `paper-cream` / `high-contrast` / `minimal-gray` / 用户主题 id | `system` | 主题 id；`system` 跟随系统明暗解析；用户主题来自 `data/themes/<id>.json`（导入生成） |
| `themeAnimEnabled` | boolean | `true`/`false` | `true` | 主题切换过渡动画（尊重系统「减少动态效果」） |
| `themeAnimMs` | number | 0–1000 整数 | `200` | 主题过渡时长（ms；0 = 无过渡） |
| `typography.fontFamily` | string | 系统字体名 或 `custom:<文件名>` | `Microsoft YaHei` | 正文主字体；`custom:` 前缀指向 `data/fonts/` 中导入的自定义字体 |
| `typography.fontSize` | number | 8–72（px） | `16` | 正文字号 |
| `typography.lineHeight` | number | 1.0–3.2 | `1.8` | 行高倍数 |
| `typography.contentWidth` | number | 320–2400（px） | `720` | 正文限宽（约 40 汉字/行） |
| `typography.pagePadding` | number | 0–240（px） | `48` | 阅读区左右页边距 |
| `typography.pagePaddingY` | number | 0–240（px） | `48` | 阅读区上下留白（旧版配置缺此字段时取默认，向后兼容） |
| `typography.paragraphSpacing` | number | 0–64（px） | `0` | 段间距（段落间额外留白） |
| `typography.firstLineIndent` | number | 0–8（字） | `0` | 首行缩进字符数（按字号换算实际像素） |
| `typography.textAlign` | string | `left`/`justify` | `left` | 文字对齐；未知值载入时归一为 `left` |
| `typography.smoothScroll` | boolean | `true`/`false` | `true` | PgUp/PgDn 翻页平滑动画（首尾跳转始终瞬时） |
| `statusBar.showFileName` | boolean | `true`/`false` | `true` | 状态栏显示文件名与进度 |
| `statusBar.showPercent` | boolean | `true`/`false` | `true` | 状态栏显示阅读百分比 |
| `statusBar.showSize` | boolean | `true`/`false` | `true` | 状态栏显示文件大小 |
| `statusBar.showEncoding` | boolean | `true`/`false` | `true` | 状态栏显示编码切换按钮 |

> 自定义字体文件存放于数据目录 `data/fonts/`（导入时复制，支持 ttf / otf / woff / woff2，单文件 ≤64MB；重名自动加序号；删除前确认）。

### 2.3 `shortcuts.json`（快捷键绑定子配置）

| 字段 | 类型 | 可填值 | 默认 | 说明 |
|---|---|---|---|---|
| `schemaVersion` | number | 固定 `3` | `3` | 配置格式版本（当前 v3；启动自动迁移旧版，见 §2.7） |
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
| `schemaVersion` | number | 固定 `1` | `1` | 会话格式版本（独立于设置 schema：会话结构变更时递增，迁移在会话模块内提供） |
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

### 2.7 配置迁移与导入/导出（P0-2）

**schema 版本（当前 v3）**：`settings.json` / `reader.json` / `shortcuts.json` 共用 `schemaVersion`（定义于 `settings::defaults::SCHEMA_VERSION`；`session.json` 版本独立）。应用启动时自动迁移旧版文件（v1→v2 字段补齐；v2→v3 主题值 `eye` 映射为 `paper-cream`）：

| 情况 | 行为 |
|---|---|
| 版本低于当前 | 依次应用迁移链 → 写回前备份为 `<文件名>.v<旧版本>.bak`（同目录） |
| 版本等于当前 | 不写文件 |
| 版本高于当前（未来版本） | 保持原文件不动（加载层回退默认；换回新版应用即恢复） |
| JSON 无法解析 | 保持原文件不动（加载层另行备份 `.corrupt-<纳秒>` 并回退默认） |
| 迁移失败 | 保持原文件不动（原因写日志） |

**命令**：

| 命令 | 作用 |
|---|---|
| `export_settings(path)` | 导出全部配置为 JSON 包到指定路径（原子写；返回写入字节数） |
| `import_settings(path)` | 导入 JSON 包：强校验 → 备份现有配置（`*.import-bak`）→ 依次写入；任一步失败自动回滚已写文件 |
| `reset_settings(scope)` | 重置设置，`scope` 三态：`{"kind":"all"}` / `{"kind":"group","name":"reader.typography"}` / `{"kind":"field","id":"app.maxTabs"}` |
| `get_settings_registry()` | 设置项注册表（id / group / label / kind），设置界面动态生成与搜索的唯一元数据源 |

导入包结构（`bundleVersion = 1`）：

```json
{
  "bundleVersion": 1,
  "schemaVersion": 2,
  "exportedAt": "2026-10-03T00:00:00.000Z",
  "app": { "…": "settings.json 原文" },
  "reader": { "…": "reader.json 原文" },
  "shortcuts": { "…": "shortcuts.json 原文（仅覆盖项）" }
}
```

导入校验规则：顶层与配置段内的未知字段一律拒绝并给出字段路径；数值 / 枚举 / 类型按注册表范围强校验（**越界拒绝**而非裁剪）；快捷键动作必须在白名单内；文件大小上限 8MB；旧版本包先走迁移链；缺失字段按默认值补齐。

### 2.8 `config.json`（数据目录迁移指针；P0-10）

位于**程序目录**（与 exe 同级；不在 `data/` 内）。仅在使用「设置 → 常规 → 数据位置 → 迁移」后生成；缺失 / 损坏 / 版本不符 / 空值会被忽略并回退便携目录。

| 字段 | 类型 | 可填值 | 默认 | 说明 |
|---|---|---|---|---|
| `schemaVersion` | number | 固定 `1` | `1` | 指针格式版本（独立于设置 schema） |
| `dataDir` | string | 绝对路径 | 无 | 自定义数据目录；迁移写入，重启后生效 |

迁移行为：复制 → 校验（文件数/字节数）→ 写指针 → 清理原目录。被占用文件跳过（典型为 WebView2 缓存）；原目录清理失败时写入新目录 `.cleanup.json` 标记，下次启动自动重试（删除成功即移除标记，损坏标记仅清除自身）。任何失败都会保留原目录并返回 `MIGRATE_FAILED`。

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

主题令牌由 `src/lib/theme.ts` 在挂载前写入 `<html>`（CSS 变量 + `data-theme-base`；后端 `get_theme` 解析，启动时经 `main.ts` 预载以防闪烁）；内置主题 `src-tauri/resources/themes/*.json`（编译期嵌入），用户主题 `data/themes/<id>.json`（导入生成，单主题驻留）。色值见设计文档 §8 与主题清单。

## 5. `package.json` / `Cargo.toml` 版本策略

- 全部依赖锁定精确版本：`package.json` 无 `^`/`~`（`.npmrc` 已设 `save-exact=true`）；`Cargo.toml` 使用 `=` 前缀。
- 锁文件（`package-lock.json`、`Cargo.lock`）必须提交且不手动修改。
