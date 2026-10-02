//! textfile：超大文本文件读取引擎（内存映射 + 编码检测 + 稀疏行索引 + 按需解码）。
//!
//! 设计要点（设计文档 §4）：
//! - **禁止整读文件**：mmap 只读映射 + 仅解码可视窗口；
//! - **稀疏索引**：每 512 行一个字节检查点（内存 ≈ 行数/512 × 8B）；
//! - **显示行 ≤ 8KB**：无换行超长行分块（UTF-8/UTF-16/传统多字节各自字符边界安全）；
//! - **换行族**：`\n` / `\r\n` / `\r` 统一语义（UTF-16 按双字节对扫描）。
//!
//! 模块划分：
//! - `encoding`：编码枚举、BOM/采样检测（WHATWG 名映射）、窗口解码
//! - `mmap`：只读内存映射（空文件安全包装）
//! - `line_index`：稀疏行索引（行号 ↔ 字节偏移）
//! - `window`：按行取文本窗口
//! - `session`：单文件会话（打开/取窗/百分比/切换编码）
//! - `editing`：编辑引擎（片表 + 撤销重做 + 保存链，设计 §5 / D17–D19）
//! - `source`：文档源契约（扩展点：未来解析器/格式实现该 trait 即可接入）

pub mod editing;
pub mod encoding;
pub mod line_index;
pub mod mmap;
pub mod session;
pub mod source;
pub mod window;
