//! `settings.json` · 文件与快照设置。
//!
//! 与 `docs/configuration.md` §2.1 的 `file.*` 字段保持同步。
//! 新增项以独立字段追加（`#[serde(default)]` 保证旧配置兼容）。

use serde::{Deserialize, Serialize};

use crate::settings::defaults;

/// 新建文件的换行风格（D-02）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NewEol {
    /// LF（`\n`，默认）
    #[serde(rename = "lf")]
    Lf,
    /// CRLF（`\r\n`）
    #[serde(rename = "crlf")]
    CrLf,
    /// CR（`\r`）
    #[serde(rename = "cr")]
    Cr,
    /// 未知取值（向前兼容；载入时归一为默认）
    #[serde(rename = "unknown")]
    Unknown,
}

/// 手写反序列化：宽容常见写法（大小写、`\n`/`\r\n`/`\r` 字面量）。
impl<'de> Deserialize<'de> for NewEol {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.trim().to_ascii_lowercase().as_str() {
            "lf" | "\\n" => Self::Lf,
            "crlf" | "\\r\\n" => Self::CrLf,
            "cr" | "\\r" => Self::Cr,
            _ => Self::Unknown,
        })
    }
}

impl Default for NewEol {
    fn default() -> Self {
        defaults::DEFAULT_FILE_NEW_EOL
    }
}

impl NewEol {
    /// 归一：`Unknown` 回退默认，其余原样。
    pub fn normalized(self) -> Self {
        if self == Self::Unknown {
            defaults::DEFAULT_FILE_NEW_EOL
        } else {
            self
        }
    }

    /// 换行字节序列（新建/导出使用）。
    pub fn sequence(self) -> &'static str {
        match self {
            Self::CrLf => "\r\n",
            Self::Cr => "\r",
            _ => "\n",
        }
    }
}

/// 文件与快照设置（`settings.json` 的嵌套对象 `file`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FileSettings {
    /// 新建文件默认编码（D-01；取值见 `docs/configuration.md`，非法回退 UTF-8）
    pub new_encoding: String,
    /// 新建文件默认换行（D-02）
    pub new_eol: NewEol,
    /// 自动保存快照间隔（秒，D-05；快照式，不写回用户文件）
    pub autosave_interval_sec: u32,
    /// 自动保存是否写回用户文件（D-06；默认关闭，仅显式开启后启用）
    pub autosave_write_back: bool,
    /// 快照保留份数上限（D-07；超限删最旧）
    pub snapshot_keep: u32,
    /// 快照总容量上限（MB，D-08；超限删最旧）
    #[serde(rename = "snapshotMaxMB")]
    pub snapshot_max_mb: u32,
    /// 版本历史开关（D-13；关闭后不保留历史视图）
    pub version_history: bool,
    /// 文件关联扩展名列表（D-16；含点小写，用户自定义）
    pub associations: Vec<String>,
    /// 「最近打开」显示条数（D-18；0 = 不显示）
    pub recent_limit: u32,
}

impl Default for FileSettings {
    fn default() -> Self {
        Self {
            new_encoding: defaults::DEFAULT_FILE_NEW_ENCODING.to_string(),
            new_eol: defaults::DEFAULT_FILE_NEW_EOL,
            autosave_interval_sec: defaults::DEFAULT_FILE_AUTOSAVE_INTERVAL_SEC,
            autosave_write_back: defaults::DEFAULT_FILE_AUTOSAVE_WRITE_BACK,
            snapshot_keep: defaults::DEFAULT_FILE_SNAPSHOT_KEEP,
            snapshot_max_mb: defaults::DEFAULT_FILE_SNAPSHOT_MAX_MB,
            version_history: defaults::DEFAULT_FILE_VERSION_HISTORY,
            associations: defaults::DEFAULT_FILE_ASSOCIATIONS
                .iter()
                .map(|ext| (*ext).to_string())
                .collect(),
            recent_limit: defaults::DEFAULT_FILE_RECENT_LIMIT,
        }
    }
}
