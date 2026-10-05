//! storage：便携数据目录与持久化基础设施。
//!
//! 模块划分（设计文档 §7）：
//! - `paths`：数据目录解析（运行时 > 环境变量 > 指针文件 > 程序目录/data）
//! - `data_dir`：目录保障与可写性探测（不可写时由上层引导用户选择目录）
//! - `migrate_dir`：数据目录迁移（复制校验 → 写指针 → 清理原目录）
//! - `atomic`：原子写入（临时文件 + fsync + rename）
//! - `json_io`：JSON 原子读写（缺失返回 None；损坏返回错误）
//!
//! 约定：本模块不读取全局环境（除 `paths::resolve_data_dir` 这层薄封装外），
//! 便于单元测试注入参数、避免测试污染进程环境。

pub mod atomic;
pub mod data_dir;
pub mod json_io;
pub mod migrate_dir;
pub mod paths;
