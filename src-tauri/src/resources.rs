//! 资源占用统计与缓存清理：日志、备份、WebView 缓存与字体的占用统计与安全清理。
//!
//! - [`disk_usage`]：数据目录占用分项统计（固定 6 键：logs / webview / fonts / backups / files / others）；
//! - [`clear_scope`]：按范围清理（日志 / WebView 缓存 / 备份类文件），逐文件容错——被占用或删除失败
//!   的文件计入 `skipped` 而不中断整体清理（Windows 上运行中的应用会持有部分缓存文件句柄）。
//!
//! 安全边界：本模块只触碰数据目录内部；不递归删除数据目录之外的任何内容；
//! 目录缺失一律视为空（返回 0），不报错。

use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

/// 分项占用（`key` 与前端语言包 `settings.disk.<key>` 对应）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsageItem {
    /// 分项键：logs / webview / fonts / backups / files / others
    pub key: &'static str,
    /// 占用字节数
    pub bytes: u64,
    /// 文件数量
    pub files: u64,
}

/// 数据目录占用报告（IPC 返回体）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsageReport {
    /// 固定顺序的分项列表
    pub items: Vec<DiskUsageItem>,
    /// 总占用（各项之和）
    pub total_bytes: u64,
}

/// 清理结果（IPC 返回体）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearResult {
    /// 成功释放的字节数
    pub cleared_bytes: u64,
    /// 删除失败/被占用的文件数（不中断清理）
    pub skipped: u64,
}

/// 清理范围（与前端 `clearCache(scope)` 的字符串键对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearScope {
    /// 日志目录（`data/logs/`）
    Logs,
    /// WebView2 缓存（`data/webview/`；运行中部分文件必然被占用）
    Webview,
    /// 备份类文件（`*.bak` / `*.import-bak` / `*.corrupt-*`，数据目录根层）
    Backups,
}

impl ClearScope {
    /// 解析字符串键；未知键返回 `None`（由命令层报错误码）。
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "logs" => Some(Self::Logs),
            "webview" => Some(Self::Webview),
            "backups" => Some(Self::Backups),
            _ => None,
        }
    }
}

/// 备份类文件名判定（与 `json_io::backup_corrupt`、`bundle` 的命名约定一致）。
pub fn is_backup_name(name: &str) -> bool {
    name.ends_with(".bak") || name.ends_with(".import-bak") || name.contains(".corrupt-")
}

/// 递归统计目录占用：返回 `(字节数, 文件数)`；目录缺失/不可读视为空。
fn sum_recursive(path: &Path) -> (u64, u64) {
    let mut bytes = 0_u64;
    let mut files = 0_u64;
    let read = match fs::read_dir(path) {
        Ok(read) => read,
        Err(_) => return (0, 0),
    };
    for entry in read.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            let (child_bytes, child_files) = sum_recursive(&entry.path());
            bytes += child_bytes;
            files += child_files;
        } else if meta.is_file() {
            bytes += meta.len();
            files += 1;
        }
    }
    (bytes, files)
}

/// 统计数据目录占用（固定 6 项，顺序稳定供界面展示）。
pub fn disk_usage(dir: &Path) -> DiskUsageReport {
    let mut logs = (0_u64, 0_u64);
    let mut webview = (0_u64, 0_u64);
    let mut fonts = (0_u64, 0_u64);
    let mut backups = (0_u64, 0_u64);
    let mut files = (0_u64, 0_u64);
    let mut others = (0_u64, 0_u64);

    if let Ok(read) = fs::read_dir(dir) {
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                let (bytes, count) = sum_recursive(&entry.path());
                match name.as_str() {
                    "logs" => logs = (bytes, count),
                    "webview" => webview = (bytes, count),
                    "fonts" => fonts = (bytes, count),
                    _ => {
                        others.0 += bytes;
                        others.1 += count;
                    }
                }
            } else if meta.is_file() {
                if is_backup_name(&name) {
                    backups.0 += meta.len();
                    backups.1 += 1;
                } else {
                    files.0 += meta.len();
                    files.1 += 1;
                }
            }
        }
    }

    let items = vec![
        DiskUsageItem {
            key: "logs",
            bytes: logs.0,
            files: logs.1,
        },
        DiskUsageItem {
            key: "webview",
            bytes: webview.0,
            files: webview.1,
        },
        DiskUsageItem {
            key: "fonts",
            bytes: fonts.0,
            files: fonts.1,
        },
        DiskUsageItem {
            key: "backups",
            bytes: backups.0,
            files: backups.1,
        },
        DiskUsageItem {
            key: "files",
            bytes: files.0,
            files: files.1,
        },
        DiskUsageItem {
            key: "others",
            bytes: others.0,
            files: others.1,
        },
    ];
    let total_bytes = items.iter().map(|item| item.bytes).sum();
    DiskUsageReport { items, total_bytes }
}

/// 清空目录内容（保留目录本身）；逐文件容错。
fn clear_dir_contents(path: &Path, result: &mut ClearResult) {
    let read = match fs::read_dir(path) {
        Ok(read) => read,
        Err(_) => return,
    };
    for entry in read.flatten() {
        let child = entry.path();
        let Ok(meta) = entry.metadata() else {
            result.skipped += 1;
            continue;
        };
        if meta.is_dir() {
            clear_dir_contents(&child, result);
            if fs::remove_dir(&child).is_err() {
                result.skipped += 1;
            }
        } else if meta.is_file() {
            match fs::remove_file(&child) {
                Ok(()) => result.cleared_bytes += meta.len(),
                Err(_) => result.skipped += 1,
            }
        }
    }
}

/// 按范围清理；目录缺失视为无事发生（返回全 0）。
pub fn clear_scope(dir: &Path, scope: ClearScope) -> ClearResult {
    let mut result = ClearResult {
        cleared_bytes: 0,
        skipped: 0,
    };
    match scope {
        ClearScope::Logs => clear_dir_contents(&dir.join("logs"), &mut result),
        ClearScope::Webview => clear_dir_contents(&dir.join("webview"), &mut result),
        ClearScope::Backups => {
            if let Ok(read) = fs::read_dir(dir) {
                for entry in read.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let Ok(meta) = entry.metadata() else {
                        result.skipped += 1;
                        continue;
                    };
                    if !meta.is_file() || !is_backup_name(&name) {
                        continue;
                    }
                    match fs::remove_file(entry.path()) {
                        Ok(()) => result.cleared_bytes += meta.len(),
                        Err(_) => result.skipped += 1,
                    }
                }
            }
        }
    }
    result
}

/// 便捷：解析并清理（命令层使用）；未知范围返回 `Err(io::Error)`。
pub fn clear_scope_by_key(dir: &Path, key: &str) -> io::Result<ClearResult> {
    let scope = ClearScope::from_key(key).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, format!("未知清理范围：{key}"))
    })?;
    Ok(clear_scope(dir, scope))
}

/// 便捷：统计（命令层使用；当前实现不会失败，保留 `io::Result` 以便未来扩展）。
pub fn disk_usage_result(dir: &Path) -> io::Result<DiskUsageReport> {
    Ok(disk_usage(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一个指定大小的文件（内容为零字节，速度稳定）。
    fn seed(path: &Path, bytes: usize) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("建目录失败");
        }
        fs::write(path, vec![0_u8; bytes]).expect("写文件失败");
    }

    /// 分项统计正确、total = 各项之和。
    #[test]
    fn disk_usage_breaks_down_entries() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let dir = tmp.path();
        seed(&dir.join("logs").join("app.log"), 1024);
        seed(&dir.join("logs").join("app.log.1"), 512);
        seed(&dir.join("webview").join("Cache").join("data.bin"), 4096);
        seed(&dir.join("fonts").join("demo.ttf"), 2048);
        seed(&dir.join("settings.json"), 300);
        seed(&dir.join("history.jsonl"), 700);
        seed(&dir.join("settings.json.bak"), 128);
        seed(&dir.join("settings.json.import-bak"), 64);
        seed(&dir.join("reader.json.corrupt-123"), 32);
        seed(&dir.join("themes").join("light.json"), 256);

        let report = disk_usage(dir);
        let by_key = |key: &str| {
            report
                .items
                .iter()
                .find(|item| item.key == key)
                .expect("分项存在")
                .clone()
        };
        assert_eq!(by_key("logs").bytes, 1536);
        assert_eq!(by_key("logs").files, 2);
        assert_eq!(by_key("webview").bytes, 4096);
        assert_eq!(by_key("fonts").bytes, 2048);
        assert_eq!(by_key("backups").bytes, 224);
        assert_eq!(by_key("backups").files, 3);
        assert_eq!(by_key("files").bytes, 1000);
        assert_eq!(by_key("files").files, 2);
        assert_eq!(by_key("others").bytes, 256);
        let sum: u64 = report.items.iter().map(|item| item.bytes).sum();
        assert_eq!(report.total_bytes, sum);
    }

    /// 目录缺失：不报错、全 0。
    #[test]
    fn disk_usage_missing_dir_is_empty() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let report = disk_usage(&tmp.path().join("nope"));
        assert_eq!(report.total_bytes, 0);
        assert!(report
            .items
            .iter()
            .all(|item| item.bytes == 0 && item.files == 0));
    }

    /// 清理日志：删 logs 内容、保留目录与用户文件。
    #[test]
    fn clear_logs_keeps_user_files() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let dir = tmp.path();
        seed(&dir.join("logs").join("nested").join("app.log"), 800);
        seed(&dir.join("settings.json"), 120);
        let result = clear_scope(dir, ClearScope::Logs);
        assert_eq!(result.cleared_bytes, 800);
        assert_eq!(result.skipped, 0);
        assert!(dir.join("logs").is_dir(), "logs 目录本身保留");
        assert!(!dir.join("logs").join("nested").exists());
        assert!(dir.join("settings.json").exists(), "用户文件不受影响");
    }

    /// 清理备份：只删备份类文件。
    #[test]
    fn clear_backups_only_matches_backup_names() {
        let tmp = tempfile::tempdir().expect("临时目录");
        let dir = tmp.path();
        seed(&dir.join("settings.json"), 100);
        seed(&dir.join("settings.json.bak"), 200);
        seed(&dir.join("settings.json.import-bak"), 300);
        seed(&dir.join("shortcuts.json.corrupt-9"), 400);
        seed(&dir.join("notbak.txt"), 500);
        let result = clear_scope(dir, ClearScope::Backups);
        assert_eq!(result.cleared_bytes, 900);
        assert!(dir.join("settings.json").exists());
        assert!(dir.join("notbak.txt").exists());
        assert!(!dir.join("settings.json.bak").exists());
        assert!(!dir.join("settings.json.import-bak").exists());
        assert!(!dir.join("shortcuts.json.corrupt-9").exists());
    }

    /// 范围键解析与未知键处理。
    #[test]
    fn scope_key_parsing() {
        assert_eq!(ClearScope::from_key("logs"), Some(ClearScope::Logs));
        assert_eq!(ClearScope::from_key("webview"), Some(ClearScope::Webview));
        assert_eq!(ClearScope::from_key("backups"), Some(ClearScope::Backups));
        assert_eq!(ClearScope::from_key("nope"), None);
        let tmp = tempfile::tempdir().expect("临时目录");
        assert!(clear_scope_by_key(tmp.path(), "nope").is_err());
    }
}
