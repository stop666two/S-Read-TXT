//! 背景图文件管理（数据目录下的 `backgrounds/`）。
//!
//! 设计（设置规格 T-04～T-08，维护者确认的「背景图」扩充项）：
//! - 文件级管理：扩展名白名单（png/jpg/jpeg/webp）、大小上限 10MB、防目录穿越；
//! - **单文件驻留**：目录内只保留当前背景图（设置成功后清理旧文件，避免磁盘累积）；
//! - 前端以设置项 `reader.background.file` 记录存储文件名，经 `read_background_image`
//!   取回 base64 + MIME 渲染为图层；后端不缓存。
//!
//! 许可与来源：由用户本地选择；仅复制到数据目录（便携模式），卸载清理钩子会随 `data/` 一并删除。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// 支持的图片扩展名（小写比较）。
const SUPPORTED_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];
/// 单张背景图大小上限（10MB；与设置规格一致）。
const MAX_BACKGROUND_BYTES: u64 = 10 * 1024 * 1024;
/// 存储文件名主干最大长度（字符数；防止超出路径长度限制）。
const MAX_STEM_CHARS: usize = 64;
/// 背景图目录名（位于数据目录内）。
pub const BACKGROUNDS_DIR_NAME: &str = "backgrounds";

/// 背景图条目（IPC 返回体；camelCase）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundEntry {
    /// 存储文件名（`data/backgrounds/` 内；设置项 `reader.background.file` 存此值）
    pub file_name: String,
    /// 显示名（原文件名主干）
    pub label: String,
    /// 文件大小（字节）
    pub size_bytes: u64,
}

/// 背景图管理错误。
#[derive(Debug, thiserror::Error)]
pub enum BackgroundError {
    /// 不支持的扩展名
    #[error("不支持的图片格式：{0}（仅支持 png / jpg / jpeg / webp）")]
    Unsupported(String),
    /// 超过大小上限
    #[error("图片文件过大：{size_bytes} 字节（上限 {limit_bytes} 字节）")]
    TooLarge {
        /// 实际大小（字节）
        size_bytes: u64,
        /// 上限（字节）
        limit_bytes: u64,
    },
    /// 文件不存在
    #[error("图片文件不存在")]
    NotFound,
    /// 非法文件名（含路径分隔符/穿越片段）
    #[error("图片文件名为非法值")]
    InvalidName,
    /// 底层 IO 失败
    #[error("背景图操作失败：{0}")]
    Io(#[from] io::Error),
}

/// 背景图目录路径（`<数据目录>/backgrounds`）。
pub fn backgrounds_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKGROUNDS_DIR_NAME)
}

/// 导入背景图：校验扩展名与大小 → 复制到 `data/backgrounds/` → 清理旧文件（单文件驻留）。
///
/// 返回：落库条目（含最终存储文件名）。
pub fn set_background(data_dir: &Path, source: &Path) -> Result<BackgroundEntry, BackgroundError> {
    set_background_with_limit(data_dir, source, MAX_BACKGROUND_BYTES)
}

/// 带自定义大小上限的实现（公共 API 固定 10MB；测试用小上限验证分支）。
fn set_background_with_limit(
    data_dir: &Path,
    source: &Path,
    limit_bytes: u64,
) -> Result<BackgroundEntry, BackgroundError> {
    let original_name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| BackgroundError::Unsupported(source.display().to_string()))?;
    let extension = extension_of(&original_name)
        .ok_or_else(|| BackgroundError::Unsupported(original_name.clone()))?;
    let metadata = fs::metadata(source).map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            BackgroundError::NotFound
        } else {
            BackgroundError::Io(err)
        }
    })?;
    if !metadata.is_file() {
        return Err(BackgroundError::NotFound);
    }
    if metadata.len() > limit_bytes {
        return Err(BackgroundError::TooLarge {
            size_bytes: metadata.len(),
            limit_bytes,
        });
    }
    let dir = backgrounds_dir(data_dir);
    fs::create_dir_all(&dir)?;
    let stem = sanitize_stem(&label_of(&original_name));
    let stored = unique_file_name(&dir, &stem, &extension);
    fs::copy(source, dir.join(&stored))?;
    remove_other_files(&dir, &stored);
    Ok(BackgroundEntry {
        file_name: stored,
        label: label_of(&original_name),
        size_bytes: metadata.len(),
    })
}

/// 删除背景图（不存在视为成功——幂等清理；非法文件名仍拒绝）。
pub fn remove_background(data_dir: &Path, file_name: &str) -> Result<(), BackgroundError> {
    let path = validated_background_path(data_dir, file_name)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(BackgroundError::Io(err)),
    }
}

/// 读取背景图字节与 MIME（文件名须通过安全性校验）。
pub fn read_background(
    data_dir: &Path,
    file_name: &str,
) -> Result<(Vec<u8>, &'static str), BackgroundError> {
    let path = validated_background_path(data_dir, file_name)?;
    let mime = mime_of(file_name).ok_or_else(|| BackgroundError::InvalidName)?;
    let bytes = fs::read(path).map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            BackgroundError::NotFound
        } else {
            BackgroundError::Io(err)
        }
    })?;
    Ok((bytes, mime))
}

/// 扩展名 → MIME（白名单外返回 `None`）。
pub fn mime_of(file_name: &str) -> Option<&'static str> {
    match extension_of(file_name)?.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// 提取小写扩展名（白名单内）。
fn extension_of(file_name: &str) -> Option<String> {
    let ext = Path::new(file_name)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    SUPPORTED_EXTENSIONS.contains(&ext.as_str()).then_some(ext)
}

/// 显示名 = 文件名主干（去扩展名）。
fn label_of(file_name: &str) -> String {
    Path::new(file_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_name.to_string())
}

/// 清洗存储名主干：去控制字符与文件系统非法字符，限长，空则回退 `background`。
fn sanitize_stem(stem: &str) -> String {
    let cleaned: String = stem
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        .take(MAX_STEM_CHARS)
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "background".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 生成唯一存储文件名（`<stem>.<ext>`；冲突时追加序号）。
fn unique_file_name(dir: &Path, stem: &str, extension: &str) -> String {
    let first = format!("{stem}.{extension}");
    if !dir.join(&first).exists() {
        return first;
    }
    for index in 2..1000 {
        let candidate = format!("{stem} ({index}).{extension}");
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{stem}-{}.{extension}", std::process::id())
}

/// 单文件驻留：删除目录内除 `keep` 之外的图片文件（失败不阻断——清理属尽力而为）。
fn remove_other_files(dir: &Path, keep: &str) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == keep {
            continue;
        }
        if extension_of(&name).is_some() {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// 校验存储文件名并返回绝对路径（拒绝分隔符/穿越/白名单外扩展名）。
fn validated_background_path(data_dir: &Path, file_name: &str) -> Result<PathBuf, BackgroundError> {
    if file_name.is_empty()
        || file_name.contains(['/', '\\'])
        || file_name.contains("..")
        || extension_of(file_name).is_none()
    {
        return Err(BackgroundError::InvalidName);
    }
    Ok(backgrounds_dir(data_dir).join(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("创建临时目录失败")
    }

    /// 设置 → 读取 → 删除 全链路；再次设置同源 → 单文件驻留（旧文件被清理）。
    #[test]
    fn set_read_remove_and_single_file_invariant() {
        let dir = work_dir();
        let source = dir.path().join("壁纸.png");
        fs::write(&source, b"fake-png-bytes").expect("写源文件失败");
        let entry = set_background(dir.path(), &source).expect("设置失败");
        assert_eq!(entry.label, "壁纸");
        assert_eq!(entry.file_name, "壁纸.png");
        assert_eq!(entry.size_bytes, 14);

        let (bytes, mime) = read_background(dir.path(), &entry.file_name).expect("读取失败");
        assert_eq!(bytes, b"fake-png-bytes");
        assert_eq!(mime, "image/png");

        // 再次设置（源仍在原位置）→ 新唯一名，旧文件被清理
        let again = set_background(dir.path(), &source).expect("再次设置失败");
        assert_ne!(again.file_name, entry.file_name);
        let names: Vec<String> = fs::read_dir(backgrounds_dir(dir.path()))
            .expect("列目录失败")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 1, "单文件驻留：{names:?}");
        assert_eq!(names[0], again.file_name);

        remove_background(dir.path(), &again.file_name).expect("删除失败");
        // 幂等：再次删除不报错
        remove_background(dir.path(), &again.file_name).expect("幂等删除失败");
    }

    /// 不支持的扩展名拒绝。
    #[test]
    fn unsupported_extension_is_rejected() {
        let dir = work_dir();
        let source = dir.path().join("note.txt");
        fs::write(&source, b"x").expect("写文件失败");
        assert!(matches!(
            set_background(dir.path(), &source),
            Err(BackgroundError::Unsupported(_))
        ));
    }

    /// 超过大小上限拒绝（用小上限验证分支）。
    #[test]
    fn oversized_background_is_rejected() {
        let dir = work_dir();
        let source = dir.path().join("big.jpg");
        fs::write(&source, vec![0u8; 32]).expect("写文件失败");
        match set_background_with_limit(dir.path(), &source, 16) {
            Err(BackgroundError::TooLarge {
                size_bytes,
                limit_bytes,
            }) => assert_eq!((size_bytes, limit_bytes), (32, 16)),
            other => panic!("应拒绝过大图片：{other:?}"),
        }
    }

    /// 源文件不存在。
    #[test]
    fn missing_source_reports_not_found() {
        let dir = work_dir();
        assert!(matches!(
            set_background(dir.path(), &dir.path().join("nope.webp")),
            Err(BackgroundError::NotFound)
        ));
    }

    /// 非法文件名（穿越/分隔符/无扩展名）在读取与删除时均被拒绝。
    #[test]
    fn invalid_file_names_are_rejected() {
        let dir = work_dir();
        for bad in ["../escape.png", "sub\\bg.png", "bg", "a..png"] {
            assert!(matches!(
                read_background(dir.path(), bad),
                Err(BackgroundError::InvalidName)
            ));
            assert!(matches!(
                remove_background(dir.path(), bad),
                Err(BackgroundError::InvalidName)
            ));
        }
    }

    /// MIME 映射白名单。
    #[test]
    fn mime_mapping_matches_whitelist() {
        assert_eq!(mime_of("a.png"), Some("image/png"));
        assert_eq!(mime_of("a.JPG"), Some("image/jpeg"));
        assert_eq!(mime_of("a.jpeg"), Some("image/jpeg"));
        assert_eq!(mime_of("a.webp"), Some("image/webp"));
        assert_eq!(mime_of("a.gif"), None);
        assert_eq!(mime_of("a"), None);
    }
}
