//! storage：便携数据目录与持久化基础设施。
//!
//! 模块划分（设计文档 §7）：
//! - `paths`：数据目录解析（程序目录/data 优先；`SRT_DATA_DIR` 覆盖）
//! - `data_dir`：目录保障与可写性探测（不可写时由上层引导用户选择目录）
//! - `atomic`（阶段 1 后续切片）：原子写入（临时文件 + rename）
//!
//! 约定：本模块不读取全局环境（除 `paths::resolve_data_dir` 这层薄封装外），
//! 便于单元测试注入参数、避免测试污染进程环境。

pub mod data_dir;
pub mod paths;
