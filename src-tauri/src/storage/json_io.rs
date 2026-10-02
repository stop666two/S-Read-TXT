//! JSON 文件读写（基于 `atomic` 的原子落盘 + 面向首启场景的容错读）。
//!
//! 约定：
//! - 读：文件不存在 → `Ok(None)`（首次运行/全新配置场景）；解析失败 → `InvalidData` 错误
//!       （调用方决定降级策略，例如回退默认值并备份损坏文件）；
//! - 写：pretty JSON（字段缩进 2 空格，便于用户手动查看/编辑与 diff），UTF-8 无 BOM；
//! - 不做 schema 版本迁移（阶段 1 模型自带 `version` 字段，迁移逻辑随模型演进）。

use std::io;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::storage::atomic;

/// 将 `value` 序列化为 pretty JSON 并原子写入 `path`。
///
/// 返回：`Ok(())` 成功；`Err(io::Error)`——序列化失败为 `InvalidData`，写盘失败为底层 IO 错误。
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    atomic::write_atomic_str(path, &text)
}

/// 读取 JSON 文件；文件不存在返回 `Ok(None)`。
///
/// 返回：`Ok(Some(T))` 解析成功；`Ok(None)` 文件不存在；`Err(InvalidData)` 内容损坏。
pub fn read_json_opt<T: DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Payload {
        version: u32,
        name: String,
    }

    /// 往返一致（含中文与多字段）。
    #[test]
    fn roundtrip_preserves_fields() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let path = tmp.path().join("p.json");
        let value = Payload {
            version: 1,
            name: "S-Read-TXT 测试".to_string(),
        };
        assert!(write_json_atomic(&path, &value).is_ok());
        let loaded: Option<Payload> = read_json_opt(&path).expect("读取失败");
        assert_eq!(loaded, Some(value));
    }

    /// 文件不存在 → None。
    #[test]
    fn missing_file_returns_none() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let loaded: Option<Payload> =
            read_json_opt(&tmp.path().join("none.json")).expect("不应报错");
        assert!(loaded.is_none());
    }

    /// 内容损坏 → InvalidData 错误。
    #[test]
    fn corrupted_content_reports_invalid_data() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let path = tmp.path().join("bad.json");
        std::fs::write(&path, b"{ not json").expect("写损坏文件失败");
        let err = read_json_opt::<Payload>(&path).expect_err("损坏内容应报错");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    /// 落盘文本为可读的 pretty JSON 且无 BOM。
    #[test]
    fn written_text_is_pretty_and_bom_free() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let path = tmp.path().join("pretty.json");
        let value = Payload {
            version: 2,
            name: "x".into(),
        };
        assert!(write_json_atomic(&path, &value).is_ok());
        let raw = std::fs::read(&path).expect("读取失败");
        assert!(!raw.starts_with(&[0xEF, 0xBB, 0xBF]), "不应有 BOM");
        let text = String::from_utf8(raw).expect("应为 UTF-8");
        assert!(text.contains('\n'), "应为 pretty 输出（含换行缩进）");
    }
}
