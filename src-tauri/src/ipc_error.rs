//! IPC 统一错误载荷（最小化 RFC 9457 精神：稳定错误码 + 中文消息）。
//!
//! 约定：
//! - 前端按 `code` 映射固定文案（例如 `FILE_TOO_LARGE` 使用需求给定的逐字提示
//!   「很抱歉，文件过大无法打开，可以在设置里面调整。」）；
//! - `message` 保留具体数值/原因，供日志与排障（不直接展示给用户时更细）；
//! - 错误码为稳定 ASCII 常量，不在前端硬编码字符串之外扩散。

use serde::Serialize;

use crate::app_state::AppStateError;
use crate::fonts::FontError;
use crate::textfile::editing::edit_doc::EditError;
use crate::textfile::editing::save::SaveError;
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
/// 配置导出失败（写入导出文件）
pub const CODE_SETTINGS_EXPORT: &str = "SETTINGS_EXPORT";
/// 配置导入失败（校验 / 读取 / 写入 / 回滚）
pub const CODE_SETTINGS_IMPORT: &str = "SETTINGS_IMPORT";
/// 设置重置失败（未知项 / 未知分组 / 保存失败）
pub const CODE_SETTINGS_RESET: &str = "SETTINGS_RESET";

/// 缓存清理范围未知（`clear_cache` 的 scope 非法）
pub const CODE_INVALID_SCOPE: &str = "INVALID_SCOPE";
/// 历史保存失败
pub const CODE_HISTORY_SAVE: &str = "HISTORY_SAVE";
/// 会话保存失败
pub const CODE_SESSION_SAVE: &str = "SESSION_SAVE";
/// 编辑位置无效/越界
pub const CODE_INVALID_POSITION: &str = "INVALID_POSITION";
/// 文件被外部修改（保存冲突）
pub const CODE_FILE_CONFLICT: &str = "FILE_CONFLICT";
/// 目标编码无法表示某字符
pub const CODE_ENCODING_UNREPRESENTABLE: &str = "ENCODING_UNREPRESENTABLE";
/// 标签未创建编辑文档
pub const CODE_NOT_EDITING: &str = "NOT_EDITING";
/// 存在未保存修改，操作被阻止
pub const CODE_EDIT_DIRTY: &str = "EDIT_DIRTY";
/// 文件超过只读阈值，不允许进入编辑模式
pub const CODE_FILE_READ_ONLY: &str = "FILE_READ_ONLY";
/// 查找/替换命中过多（全部替换保护上限）
pub const CODE_QUERY_TOO_BROAD: &str = "QUERY_TOO_BROAD";
/// 正则表达式无效（编译失败；message 为引擎说明）
pub const CODE_INVALID_REGEX: &str = "INVALID_REGEX";
/// 预览后文档发生变化（需重新查找/预览）
pub const CODE_SEARCH_STALE: &str = "SEARCH_STALE";
/// 字体格式不支持
pub const CODE_FONT_UNSUPPORTED: &str = "FONT_UNSUPPORTED";
/// 字体文件过大
pub const CODE_FONT_TOO_LARGE: &str = "FONT_TOO_LARGE";
/// 字体文件不存在
pub const CODE_FONT_NOT_FOUND: &str = "FONT_NOT_FOUND";
/// 字体文件名为非法值
pub const CODE_FONT_INVALID_NAME: &str = "FONT_INVALID_NAME";
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

impl From<EditError> for IpcError {
    fn from(err: EditError) -> Self {
        match err {
            EditError::File(err) => err.into(),
            EditError::RowOutOfRange { row } => {
                Self::new(CODE_INVALID_POSITION, format!("行号越界：{row}"))
            }
            EditError::Utf16OutOfRange { row, utf16 } => Self::new(
                CODE_INVALID_POSITION,
                format!("字符位置越界：行 {row} 偏移 {utf16}"),
            ),
            EditError::InvalidPosition => Self::new(CODE_INVALID_POSITION, "内部位置无效"),
            EditError::TooManyMatches { limit } => Self::new(
                CODE_QUERY_TOO_BROAD,
                format!("匹配过多（超过 {limit} 处），请使用更具体的查找内容"),
            ),
            EditError::InvalidRegex { message } => {
                Self::new(CODE_INVALID_REGEX, format!("正则表达式无效：{message}"))
            }
            EditError::StaleSearch => {
                Self::new(CODE_SEARCH_STALE, "文档已变化，请重新执行查找/替换")
            }
        }
    }
}

impl From<FontError> for IpcError {
    fn from(err: FontError) -> Self {
        match err {
            FontError::Unsupported(name) => Self::new(
                CODE_FONT_UNSUPPORTED,
                format!("不支持的字体格式：{name}（仅支持 ttf / otf / woff / woff2）"),
            ),
            FontError::TooLarge {
                size_bytes,
                limit_bytes,
            } => Self::new(
                CODE_FONT_TOO_LARGE,
                format!(
                    "字体文件过大：{:.1} MB（上限 {:.1} MB）",
                    size_bytes as f64 / 1_048_576.0,
                    limit_bytes as f64 / 1_048_576.0
                ),
            ),
            FontError::NotFound => Self::new(CODE_FONT_NOT_FOUND, "字体文件不存在"),
            FontError::InvalidName => Self::new(CODE_FONT_INVALID_NAME, "字体文件名为非法值"),
            FontError::Io(err) => Self::new(CODE_IO, format!("字体操作失败：{err}")),
        }
    }
}

impl From<SaveError> for IpcError {
    fn from(err: SaveError) -> Self {
        match err {
            SaveError::Conflict => {
                Self::new(CODE_FILE_CONFLICT, "文件已在外部被修改，请选择覆盖或另存为")
            }
            SaveError::Unrepresentable { ch } => Self::new(
                CODE_ENCODING_UNREPRESENTABLE,
                format!("当前编码无法表示字符「{ch}」，可改用 UTF-8 保存"),
            ),
            SaveError::Io(err) => Self::new(CODE_IO, format!("保存文件失败：{err}")),
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
            AppStateError::Edit(err) => err.into(),
            AppStateError::Save(err) => err.into(),
            AppStateError::Io(err) => Self::new(CODE_IO, format!("文件操作失败：{err}")),
            AppStateError::NotEditing(tab_id) => Self::new(
                CODE_NOT_EDITING,
                format!("标签 {tab_id} 没有可用的编辑文档"),
            ),
            AppStateError::DirtyEdit(tab_id) => Self::new(
                CODE_EDIT_DIRTY,
                format!("标签 {tab_id} 有未保存的修改，请先保存或放弃修改"),
            ),
            AppStateError::EditTooLarge {
                size_bytes,
                limit_mb,
            } => Self::new(
                CODE_FILE_READ_ONLY,
                format!(
                    "文件大小 {:.1} MB 超过只读阈值 {limit_mb} MB，已以只读模式打开（可在设置中调整）",
                    size_bytes as f64 / (1024.0 * 1024.0)
                ),
            ),
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

    /// 编辑类错误映射（位置越界 / 脏态阻止）。
    #[test]
    fn edit_errors_map() {
        let position: IpcError = EditError::RowOutOfRange { row: 99 }.into();
        assert_eq!(position.code, CODE_INVALID_POSITION);
        let dirty: IpcError = AppStateError::DirtyEdit(3).into();
        assert_eq!(dirty.code, CODE_EDIT_DIRTY);
        let too_many: IpcError = EditError::TooManyMatches { limit: 200_000 }.into();
        assert_eq!(too_many.code, CODE_QUERY_TOO_BROAD);
    }

    /// 保存类错误映射（冲突 / 不可表示字符）。
    #[test]
    fn save_errors_map() {
        let conflict: IpcError = SaveError::Conflict.into();
        assert_eq!(conflict.code, CODE_FILE_CONFLICT);
        let unmappable: IpcError = SaveError::Unrepresentable { ch: '𠀀' }.into();
        assert_eq!(unmappable.code, CODE_ENCODING_UNREPRESENTABLE);
        assert!(unmappable.message.contains('𠀀'));
    }
}
