# S-Read-TXT

极简 Windows 桌面 TXT 阅读器（可切换编辑）。基于 Tauri v2 + Rust + Svelte 5，完全离线、便携存储、专注长文阅读。

- 版本：0.0.1-beta
- 平台：Windows 10/11（x64）
- 许可证：MIT
- 仓库地址：待补充（占位）

## 功能

- 阅读：内存映射 + 按需分页读取（禁止整读文件），支持超大文件（默认上限 100MB，可设置）；自动检测编码（BOM/chardetng）并可手动切换；兼容 LF/CRLF/CR 与无换行超长行
- 多标签：紧凑标签栏（中键关闭、右键菜单、拖拽排序、上限可设），切换保持各标签阅读状态
- 历史记录：独立面板 + 菜单「最近打开」，重开恢复阅读进度；条数与保留期可设，支持清理
- 快捷键：应用内全局、默认方案 + 自定义（录制、冲突检测、恢复默认）
- 编辑（可选切换）：分块编辑引擎（piece table），任意大小文件、内存随编辑量增长；查找替换、另存为、首存 .bak、保存编码询问
- 主题：浅色 / 深色 / 护眼 / 跟随系统；排版（字号/行距/字体/限宽/边距）可调

## 硬性指标（验收红线）

| 指标 | 要求 |
|---|---|
| 内存 | 10 标签只读态 <100MB（120MB 兜底，报告说明） |
| 冷启动 | 到可阅读 <1s |
| 安装包 | NSIS 产物 <10MB（不捆绑 WebView2） |

## 技术栈与依赖

- 后端：Rust（Tauri 2.12.1）、memmap2、encoding_rs、chardetng、memchr、serde、time、log
- 前端：Svelte 5.57.1、Vite 8.3.2、UnoCSS 66.10.5、TypeScript 5.9.3
- 测试：cargo test、Vitest、CDP 驱动 E2E、Node 采样脚本
- 依赖精确锁定（`Cargo.toml` 使用 `=` 版本；`package.json` 无范围符）

## 开发环境要求

- Windows 10/11 x64，WebView2 运行时（系统自带；安装包不捆绑）
- Node.js ≥ 20（开发使用 v26）与 npm
- Rust 工具链（开发使用 stable-x86_64-pc-windows-gnu；MSVC 工具链亦可）

### Windows 构建环境注意

- 首次 Rust 编译约需 10~15 分钟（依赖全量编译），之后增量构建为秒级。
- 本项目在 Windows 上以 **GNU 工具链**实测构建通过（Tauri 官方仅支持 MSVC；如需切换：安装 VS Build Tools 后执行 `rustup default stable-x86_64-pc-windows-msvc`）。
- 若构建报 `windres: preprocessing failed.` 或 `cc1.exe` 静默失败（退出码 `0xC0000139` / `STATUS_ENTRYPOINT_NOT_FOUND`）：原因是 PATH 中其他目录（如 Tesseract-OCR）携带的**旧版 `libgcc_s_seh-1.dll`** 抢先覆盖了 MSYS2 的运行库。修复：确保 MSYS2 的 `ucrt64\bin`（例如 `D:\msys64\ucrt64\bin`）在 PATH 中排在该目录之前，或从 PATH 移除该目录后重新构建。

## 构建与运行

```powershell
# 安装依赖
npm install

# 开发运行（前端热更 + Tauri 窗口）
npm run tauri dev

# 前端构建 / 类型检查 / 单元测试
npm run build
npm run check
npm run test

# 打包（NSIS 安装器输出于 src-tauri/target/release/bundle/nsis/）
npm run tauri build
```

## 数据与隐私

- 强制便携模式：所有数据存放于**程序目录** `data/`（`settings.json`、`reader.json`、`shortcuts.json`、`session.json`、`history.jsonl`、`logs/`），不写入系统其他位置
- 如果程序目录不可写：启动时弹窗引导选择可写目录（仅本次会话生效）
- 完全离线：无遥测、无错误上报、无更新检测、无任何网络请求
- 数据不加密；日志不含敏感内容

## 环境变量（可选）

| 名称 | 用途 | 类型 | 必填 | 示例 |
|---|---|---|---|---|
| `SRT_DATA_DIR` | 覆盖数据目录（便于测试与特殊部署） | 路径字符串 | 否 | `D:\srt-data` |
| `SRT_LOG_LEVEL` | 覆盖日志级别（error/warn/info/debug） | 枚举字符串 | 否 | `debug` |

说明：应用不读取 `.env` 文件，环境变量由启动环境（终端/快捷方式）提供。

## 目录结构

```
docs/            设计文档、实施计划、配置说明、测试报告、截图
scripts/         Node 脚本（图标生成、编码自检、内存采样、git 钩子）
src/             Svelte 前端
src-tauri/       Rust 后端（Tauri）
```

## 文档

- 设计定稿：`docs/design/2026-10-02-s-read-txt-design.md`
- 实施计划：`docs/plan/implementation-plan.md`
- 配置逐字段说明：`docs/configuration.md`（随阶段补充）
- 测试报告：`docs/test-report.md`（阶段 9 交付）
