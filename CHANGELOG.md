# 变更日志

本文件遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 与 [语义化版本 2.0.0](https://semver.org/lang/zh-CN/)。

## [未发布]

### 新增

- 工程初始化：Tauri v2 + Svelte 5 脚手架、便携数据目录保护（`.gitignore`/pre-commit 钩子）、MIT 许可证、设计文档与实施计划
- 构建与验证工具链：编码自检（UTF-8 无 BOM / LF）、图标生成、CDP 冒烟与截图脚本

### 修复

- Windows GNU 工具链构建失败：PATH 中旧版 `libgcc_s_seh-1.dll`（Tesseract-OCR）遮蔽 MSYS2 运行库，导致 `cc1.exe` 启动失败（`0xC0000139 STATUS_ENTRYPOINT_NOT_FOUND`）而 `windres` 报 `preprocessing failed.`；修复方式见 README「Windows 构建环境注意」
