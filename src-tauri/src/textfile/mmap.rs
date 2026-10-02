//! 只读内存映射（超大文件按需分页，禁止整读文件）。
//!
//! 说明：
//! - 空文件无法映射（Windows 限制），以 `None` 包装并对外表现为空切片；
//! - 映射期间若外部进程截断文件可能触发访问异常（Windows 上为进程级异常），
//!   缓解策略：外部修改由上层按修改时间检测并提示重新加载（阶段 2 接线），
//!   本模块不主动承担该职责。

use std::fs::File;
use std::path::Path;

use memmap2::Mmap;

/// 打开后的只读文件映射。
pub struct MappedFile {
    /// 映射本体（空文件为 None）
    mmap: Option<Mmap>,
    /// 文件字节长度
    len: u64,
}

impl MappedFile {
    /// 只读映射文件。
    ///
    /// 返回：`Ok(MappedFile)`；`Err(io::Error)`（文件不存在/无权限/映射失败）。
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        if len == 0 {
            return Ok(Self { mmap: None, len: 0 });
        }
        // SAFETY: 只读映射；不通过映射写入。外部截断导致的内存访问异常风险
        // 由上层的外部修改检测策略缓解（见模块说明）。
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self {
            mmap: Some(mmap),
            len,
        })
    }

    /// 文件字节长度。
    pub fn len(&self) -> u64 {
        self.len
    }

    /// 是否为空文件。
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 只读字节切片（空文件返回 `&[]`）。
    pub fn bytes(&self) -> &[u8] {
        self.mmap.as_deref().map(|m| &m[..]).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 映射内容与磁盘一致。
    #[test]
    fn maps_file_content() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "内容 abc\n第二行").expect("写文件失败");
        let mapped = MappedFile::open(&path).expect("映射失败");
        assert_eq!(mapped.len(), "内容 abc\n第二行".len() as u64);
        assert_eq!(mapped.bytes(), "内容 abc\n第二行".as_bytes());
        assert!(!mapped.is_empty());
    }

    /// 空文件可打开且表现为空切片。
    #[test]
    fn empty_file_is_supported() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("empty.txt");
        File::create(&path)
            .expect("创建失败")
            .flush()
            .expect("flush 失败");
        let mapped = MappedFile::open(&path).expect("映射失败");
        assert!(mapped.is_empty());
        assert_eq!(mapped.bytes(), b"");
    }

    /// 文件不存在返回错误。
    #[test]
    fn missing_file_errors() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        assert!(MappedFile::open(&dir.path().join("none.txt")).is_err());
    }
}
