//! IPC 统一错误载荷（最小化 RFC 9457 精神：稳定错误码 + 中文消息）。
//!
//! 约定：
//! - 前端按 `code` 映射固定文案（例如 `FILE_TOO_LARGE` 使用需求给定的逐字提示
//!   「很抱歉，文件过大无法打开，可以在设置里面调整。」）；
//! - `message` 保留具体数值/原因，供日志与排障（不直接展示给用户时更细）；
//! - 错误码为稳定 ASCII 常量，不在前端硬编码字符串之外扩散。

use serde::Serialize;

use crate::app_state::AppStateError;
use crate::textfile::session::TextFileError;

/// 文件不存在或不可访问
pub const CODE_FILE_NOT_FOUND: &str = "FILE_NOT_FOUND";
/// 文件超过可打开大小上限
pub const CODE_FILE_TOO_LARGE: &str = "FILE_TOO_LARGE";
/// 标签数量已达上限
pub const CODE_MAX_TABS: &str = "MAX_TABS";
/// 标签不存在
pub const CODE_TAB_NOT_FOUND: &str = "TAB_NOT_FOUND";
/// 未知编码名
pub const CODE_INVALID_ENCODING: &str = "INVALID_ENCODING";
/// 底层 IO 失败
pub const CODE_IO: &str = "IO";
/// 配置保存失败
pub const CODE_CONFIG_SAVE: &str = "CONFIG_SAVE";
/// 历史保存失败
pub const CODE_HISTORY_SAVE: &str = "HISTORY_SAVE";
/// 会话保存失败
pub const CODE_SESSION_SAVE: &str = "SESSION_SAVE";
/// 内部错误（锁中毒等）
pub const CODE_INTERNAL: &str = "INTERNAL";

/// IPC 错误载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpcError {
    /// 稳定错误码（见本模块常量）
    pub code: &'static str,
    /// 中文错误消息（含数值/原因）
    pub message: String,
}

impl IpcError {
    /// 构造错误载荷。
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// 内部错误快捷构造。
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(CODE_INTERNAL, message)
    }
}

impl From<TextFileError> for IpcError {
    fn from(err: TextFileError) -> Self {
        match err {
            TextFileError::NotFound(path) => Self::new(
                CODE_FILE_NOT_FOUND,
                format!("文件不存在或不可访问：{}", path.display()),
            ),
            TextFileError::TooLarge {
                size_bytes,
                limit_mb,
            } => {
                let size_mb = size_bytes as f64 / (1024.0 * 1024.0);
                Self::new(
                    CODE_FILE_TOO_LARGE,
                    format!("文件过大：{size_mb:.1} MB，超过上限 {limit_mb} MB"),
                )
            }
            TextFileError::Io(err) => Self::new(CODE_IO, format!("读取文件失败：{err}")),
        }
    }
}

impl From<AppStateError> for IpcError {
    fn from(err: AppStateError) -> Self {
        match err {
            AppStateError::TabNotFound(tab_id) => {
                Self::new(CODE_TAB_NOT_FOUND, format!("标签不存在：{tab_id}"))
            }
            AppStateError::MaxTabs { limit } => Self::new(
                CODE_MAX_TABS,
                format!("标签数量已达上限（{limit} 个），请先关闭部分标签"),
            ),
            AppStateError::TextFile(err) => err.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 文件不存在 → 稳定错误码。
    #[test]
    fn not_found_maps_to_code() {
        let err: IpcError = TextFileError::NotFound(PathBuf::from("D:/x.txt")).into();
        assert_eq!(err.code, CODE_FILE_NOT_FOUND);
        assert!(err.message.contains("D:/x.txt"));
    }

    /// 文件过大 → 稳定错误码 + 数值消息。
    #[test]
    fn too_large_maps_with_numbers() {
        let err: IpcError = TextFileError::TooLarge {
            size_bytes: 10 * 1024 * 1024,
            limit_mb: 5,
        }
        .into();
        assert_eq!(err.code, CODE_FILE_TOO_LARGE);
        assert!(err.message.contains("10.0 MB"));
        assert!(err.message.contains("5 MB"));
    }

    /// 状态错误映射（标签不存在 / 超上限 / 透传文本错误）。
    #[test]
    fn app_state_errors_map() {
        let not_found: IpcError = AppStateError::TabNotFound(7).into();
        assert_eq!(not_found.code, CODE_TAB_NOT_FOUND);
        let max_tabs: IpcError = AppStateError::MaxTabs { limit: 20 }.into();
        assert_eq!(max_tabs.code, CODE_MAX_TABS);
        let passthrough: IpcError =
            AppStateError::TextFile(TextFileError::NotFound(PathBuf::from("D:/y.txt"))).into();
        assert_eq!(passthrough.code, CODE_FILE_NOT_FOUND);
    }
}
