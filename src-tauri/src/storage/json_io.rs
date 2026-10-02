//! JSON 文件读写（基于 `atomic` 的原子落盘 + 面向首启场景的容错读）。
//!
//! 约定：
//! - 读：文件不存在 → `Ok(None)`（首次运行/全新配置场景）；解析失败 → `InvalidData` 错误；
//! - 自愈载入：[`load_json_or_default`] 把「缺失回默认、损坏备份后回默认」封装为统一策略，
//!   各配置模块（settings/session 等）共用，避免各自重复实现；
//! - 写：pretty JSON（字段缩进 2 空格，便于用户手动查看/编辑与 diff），UTF-8 无 BOM；
//! - 不做 schema 版本迁移（各模型自带 `schemaVersion` 字段，迁移逻辑随模型演进）。

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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

/// 通用「自愈载入」：成功则归一；文件缺失取默认值；内容损坏则备份后取默认值（记日志）。
///
/// 参数：
/// - `path`：目标 JSON 文件；
/// - `normalize`：解析成功后的归一闭包（对齐版本、裁剪范围等）。
///
/// 返回：归一后的值；任何失败路径都退化为 `T::default()`（不阻塞调用方）。
pub fn load_json_or_default<T, F>(path: &Path, normalize: F) -> T
where
    T: DeserializeOwned + Default,
    F: FnOnce(&mut T),
{
    match read_json_opt::<T>(path) {
        Ok(Some(mut value)) => {
            normalize(&mut value);
            value
        }
        Ok(None) => T::default(),
        Err(err) => {
            log::warn!(
                "JSON 文件损坏，回退默认值并备份：{}（原因：{err}）",
                path.display()
            );
            backup_corrupt(path);
            T::default()
        }
    }
}

/// 将损坏文件重命名为 `<文件名>.corrupt-<纳秒>`；失败仅记日志（不阻塞启动）。
///
/// 返回：备份路径（成功）或 `None`（失败/文件名不可用）。
pub fn backup_corrupt(path: &Path) -> Option<PathBuf> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let file_name = path.file_name()?.to_string_lossy().into_owned();
    let backup = path.with_file_name(format!("{file_name}.corrupt-{nanos}"));
    match std::fs::rename(path, &backup) {
        Ok(()) => Some(backup),
        Err(err) => {
            log::warn!("备份损坏文件失败：{}（{err}）", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
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

    /// 自愈载入：缺失文件取默认值，归一闭包不执行。
    #[test]
    fn load_or_default_returns_default_when_missing() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let value: Payload = load_json_or_default(
            &tmp.path().join("none.json"),
            |_: &mut Payload| unreachable!(),
        );
        assert_eq!(value, Payload::default());
    }

    /// 自愈载入：成功时执行归一闭包。
    #[test]
    fn load_or_default_applies_normalize() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let path = tmp.path().join("p.json");
        std::fs::write(&path, br#"{"version":99,"name":"x"}"#).expect("写失败");
        let value: Payload =
            load_json_or_default(&path, |payload: &mut Payload| payload.version = 1);
        assert_eq!(value.version, 1);
        assert_eq!(value.name, "x");
    }

    /// 自愈载入：损坏时备份文件并取默认值。
    #[test]
    fn load_or_default_backs_up_corrupt() {
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let path = tmp.path().join("broken.json");
        std::fs::write(&path, b"{ broken").expect("写失败");
        let value: Payload = load_json_or_default(&path, |_: &mut Payload| {});
        assert_eq!(value, Payload::default());
        let backups: Vec<_> = std::fs::read_dir(tmp.path())
            .expect("列目录失败")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("broken.json.corrupt-"))
            .collect();
        assert_eq!(backups.len(), 1, "应生成备份：{backups:?}");
        assert!(!path.exists(), "损坏文件应已移走");
    }
}
