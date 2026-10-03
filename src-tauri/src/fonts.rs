//! 自定义字体导入与管理（数据目录下的 `fonts/`）。
//!
//! 设计（维护者确认的扩充项「导入自定义字体」）：
//! - 只做「文件级」管理：扩展名白名单、大小上限、防目录穿越与非法字符；
//! - 不解析字体内部家族名（避免引入字体解析依赖）：显示名取原文件名主干，
//!   前端以 `custom:<存储文件名>` 约定值 + FontFace 动态加载；
//! - 所有数据位于数据目录内（便携模式），卸载清理钩子会连同 `data/` 一并删除。
//!
//! 资源纪律：`read_font_bytes` 只在被调用时读入单个字体文件（通常数百 KB～数十 MB），
//! 由前端在用完后交给 `FontFace`/浏览器管理；后端不缓存。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// 支持的字体扩展名（小写比较）。
const SUPPORTED_EXTENSIONS: [&str; 4] = ["ttf", "otf", "woff", "woff2"];
/// 单个字体文件大小上限（64MB）。
const MAX_FONT_BYTES: u64 = 64 * 1024 * 1024;
/// 存储文件名主干最大长度（字符数；防止超出路径长度限制）。
const MAX_STEM_CHARS: usize = 64;
/// 字体目录名（位于数据目录内）。
pub const FONTS_DIR_NAME: &str = "fonts";

/// 字体条目（IPC 返回体；camelCase 供前端直接消费）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontEntry {
    /// 存储文件名（`data/fonts/` 内；同时是前端 `custom:<file>` 的标识）
    pub file_name: String,
    /// 显示名（原文件名主干）
    pub label: String,
    /// 文件大小（字节）
    pub size_bytes: u64,
}

/// 字体管理错误。
#[derive(Debug, thiserror::Error)]
pub enum FontError {
    /// 不支持的扩展名（携带原文件名供提示）
    #[error("不支持的字体格式：{0}（仅支持 ttf / otf / woff / woff2）")]
    Unsupported(String),
    /// 超过大小上限
    #[error("字体文件过大：{size_bytes} 字节（上限 {limit_bytes} 字节）")]
    TooLarge {
        /// 实际大小（字节）
        size_bytes: u64,
        /// 上限（字节）
        limit_bytes: u64,
    },
    /// 文件不存在
    #[error("字体文件不存在")]
    NotFound,
    /// 非法文件名（含路径分隔符/穿越片段）
    #[error("字体文件名为非法值")]
    InvalidName,
    /// 底层 IO 失败
    #[error("字体操作失败：{0}")]
    Io(#[from] io::Error),
}

/// 字体目录路径（`<数据目录>/fonts`）。
pub fn fonts_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(FONTS_DIR_NAME)
}

/// 列出已导入的字体（按存储文件名排序；目录不存在时返回空表）。
pub fn list_fonts(data_dir: &Path) -> io::Result<Vec<FontEntry>> {
    let dir = fonts_dir(data_dir);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if extension_of(&file_name).is_none() {
            continue;
        }
        entries.push(FontEntry {
            label: label_of(&file_name),
            file_name,
            size_bytes: entry.metadata()?.len(),
        });
    }
    entries.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    Ok(entries)
}

/// 导入字体：校验扩展名与大小 → 复制到 `data/fonts/`（重名自动加序号）。
///
/// 返回：落库条目（含最终存储文件名）。
pub fn import_font(data_dir: &Path, source: &Path) -> Result<FontEntry, FontError> {
    import_font_with_limit(data_dir, source, MAX_FONT_BYTES)
}

/// 带自定义大小上限的导入实现（公共 API 固定 64MB；测试用小上限验证分支）。
fn import_font_with_limit(
    data_dir: &Path,
    source: &Path,
    limit_bytes: u64,
) -> Result<FontEntry, FontError> {
    let original_name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| FontError::Unsupported(source.display().to_string()))?;
    let extension = extension_of(&original_name)
        .ok_or_else(|| FontError::Unsupported(original_name.clone()))?;
    let metadata = fs::metadata(source).map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            FontError::NotFound
        } else {
            FontError::Io(err)
        }
    })?;
    if !metadata.is_file() {
        return Err(FontError::NotFound);
    }
    if metadata.len() > limit_bytes {
        return Err(FontError::TooLarge {
            size_bytes: metadata.len(),
            limit_bytes,
        });
    }
    let dir = fonts_dir(data_dir);
    fs::create_dir_all(&dir)?;
    let stem = sanitize_stem(&label_of(&original_name));
    let stored = unique_file_name(&dir, &stem, &extension);
    fs::copy(source, dir.join(&stored))?;
    Ok(FontEntry {
        file_name: stored,
        label: label_of(&original_name),
        size_bytes: metadata.len(),
    })
}

/// 删除已导入字体（文件名须通过安全性校验）。
pub fn remove_font(data_dir: &Path, file_name: &str) -> Result<(), FontError> {
    let path = validated_font_path(data_dir, file_name)?;
    fs::remove_file(path).map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            FontError::NotFound
        } else {
            FontError::Io(err)
        }
    })
}

/// 读取字体文件字节（文件名须通过安全性校验；由调用方决定生命周期）。
pub fn read_font_bytes(data_dir: &Path, file_name: &str) -> Result<Vec<u8>, FontError> {
    let path = validated_font_path(data_dir, file_name)?;
    fs::read(path).map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            FontError::NotFound
        } else {
            FontError::Io(err)
        }
    })
}

/// 标准 Base64 编码（RFC 4648 §4；为省一个依赖而内置，附 RFC 测试向量）。
pub fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(*chunk.get(1).unwrap_or(&0));
        let b2 = u32::from(*chunk.get(2).unwrap_or(&0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// 提取小写扩展名（供白名单与列表过滤共用）。
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

/// 清洗存储名主干：去控制字符与文件系统非法字符，限长，空则回退 `font`。
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
        "font".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 生成唯一存储文件名（`<stem>.<ext>`；冲突时 `<stem> (2).<ext>`，依次递增）。
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

/// 校验存储文件名并返回绝对路径（拒绝分隔符/穿越/非法扩展名）。
fn validated_font_path(data_dir: &Path, file_name: &str) -> Result<PathBuf, FontError> {
    if file_name.is_empty()
        || file_name.contains(['/', '\\'])
        || file_name.contains("..")
        || extension_of(file_name).is_none()
    {
        return Err(FontError::InvalidName);
    }
    Ok(fonts_dir(data_dir).join(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("创建临时目录失败")
    }

    /// Base64 内置实现与 RFC 4648 测试向量一致。
    #[test]
    fn base64_matches_rfc4648_vectors() {
        let cases: [(&[u8], &str); 7] = [
            (b"", ""),
            (b"f", "Zg=="),
            (b"fo", "Zm8="),
            (b"foo", "Zm9v"),
            (b"foob", "Zm9vYg=="),
            (b"fooba", "Zm9vYmE="),
            (b"foobar", "Zm9vYmFy"),
        ];
        for (input, expected) in cases {
            assert_eq!(base64_encode(input), expected, "输入：{input:?}");
        }
    }

    /// 导入 → 列表 → 读取 → 删除 全链路（含中文文件名与唯一化）。
    #[test]
    fn import_list_read_remove_roundtrip() {
        let dir = work_dir();
        let source = dir.path().join("测试字体.ttf");
        fs::write(&source, b"fake-font-bytes").expect("写源文件失败");
        let entry = import_font(dir.path(), &source).expect("导入失败");
        assert_eq!(entry.label, "测试字体");
        assert_eq!(entry.file_name, "测试字体.ttf");
        assert_eq!(entry.size_bytes, 15);

        // 再次导入同名 → 自动唯一化
        let again = import_font(dir.path(), &source).expect("再次导入失败");
        assert_eq!(again.file_name, "测试字体 (2).ttf");

        let list = list_fonts(dir.path()).expect("列目录失败");
        assert_eq!(list.len(), 2);

        let bytes = read_font_bytes(dir.path(), &entry.file_name).expect("读取失败");
        assert_eq!(bytes, b"fake-font-bytes");

        remove_font(dir.path(), &entry.file_name).expect("删除失败");
        assert_eq!(list_fonts(dir.path()).expect("列目录失败").len(), 1);
    }

    /// 不支持的扩展名拒绝。
    #[test]
    fn unsupported_extension_is_rejected() {
        let dir = work_dir();
        let source = dir.path().join("note.txt");
        fs::write(&source, b"x").expect("写文件失败");
        assert!(matches!(
            import_font(dir.path(), &source),
            Err(FontError::Unsupported(_))
        ));
    }

    /// 超过大小上限拒绝（用小上限验证分支）。
    #[test]
    fn oversized_font_is_rejected() {
        let dir = work_dir();
        let source = dir.path().join("big.ttf");
        fs::write(&source, vec![0u8; 32]).expect("写文件失败");
        match import_font_with_limit(dir.path(), &source, 16) {
            Err(FontError::TooLarge {
                size_bytes,
                limit_bytes,
            }) => {
                assert_eq!((size_bytes, limit_bytes), (32, 16));
            }
            other => panic!("应拒绝过大字体：{other:?}"),
        }
    }

    /// 源文件不存在。
    #[test]
    fn missing_source_reports_not_found() {
        let dir = work_dir();
        assert!(matches!(
            import_font(dir.path(), &dir.path().join("nope.ttf")),
            Err(FontError::NotFound)
        ));
    }

    /// 非法文件名（穿越/分隔符/无扩展名）在读取与删除时均被拒绝。
    #[test]
    fn invalid_file_names_are_rejected() {
        let dir = work_dir();
        for bad in ["../escape.ttf", "sub\\font.ttf", "font", "a..ttf"] {
            assert!(matches!(
                read_font_bytes(dir.path(), bad),
                Err(FontError::InvalidName)
            ));
            assert!(matches!(
                remove_font(dir.path(), bad),
                Err(FontError::InvalidName)
            ));
        }
    }

    /// 空目录列表为空；不相关扩展名文件被忽略。
    #[test]
    fn list_ignores_unrelated_files_and_missing_dir() {
        let dir = work_dir();
        assert!(list_fonts(dir.path()).expect("列表失败").is_empty());
        let fonts = fonts_dir(dir.path());
        fs::create_dir_all(&fonts).expect("建目录失败");
        fs::write(fonts.join("readme.txt"), b"x").expect("写文件失败");
        fs::write(fonts.join("ok.otf"), b"y").expect("写文件失败");
        let list = list_fonts(dir.path()).expect("列表失败");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].file_name, "ok.otf");
    }
}
