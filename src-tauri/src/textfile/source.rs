//! 文档源抽象（扩展点预留，维护者要求）。
//!
//! 背景：S-Read-TXT 当前以「纯文本读取引擎」（mmap + 编码 + 稀疏索引 + 编辑片表）
//! 为首个文档源实现。未来可能扩展**解析器与格式**（例如：Markdown 结构化渲染、
//! 压缩包内文本、其它文档容器的文本抽取）。
//!
//! 预留策略（避免将来动大手术）：
//! - 本 trait 是「虚拟滚动 ↔ 后端取行」之间最小且稳定的读契约；AppState 取行与
//!   前端渲染只依赖该契约；
//! - 新格式实现本 trait，并在打开流程（`AppState::open_file`）按格式检测分派即可
//!   接入，IPC 与界面无需改动；
//! - 编辑能力**不在**本契约内：解析类文档天然只读；如未来需要可编辑的新格式，
//!   另行定义编辑契约（当前编辑契约仅绑定文本引擎）。
//!
//! 当前实现：`FileSession`（只读磁盘视图）与 `EditDoc`（含未保存修改的编辑视图）。

use std::path::Path;

use crate::textfile::editing::edit_doc::EditDoc;
use crate::textfile::encoding::FileEncoding;
use crate::textfile::session::FileSession;
use crate::textfile::window::RowText;

/// 只读文档源契约（行式读取）。
pub trait DocumentSource {
    /// 文件路径（标签展示与重复打开去重用）。
    fn path(&self) -> &Path;

    /// 当前生效编码（状态栏展示）。
    fn encoding(&self) -> FileEncoding;

    /// 显示行总数（虚拟滚动高度依据；超长逻辑行按 8KB 分段计入）。
    fn rows_total(&self) -> u64;

    /// 原始字节总长（状态栏展示；百分比计算基准）。
    fn byte_len(&self) -> u64;

    /// 取 `[start_row, start_row + count)` 显示行文本；越界起点返回空列表。
    ///
    /// 编辑视图的超长行分段附带 `logicalRow` / `baseUtf16` 供光标映射。
    fn fetch_rows(&self, start_row: u64, count: usize) -> Vec<RowText>;

    /// 显示行首对应的阅读百分比（0.0–100.0；行首字节语义，与只读路径一致）。
    fn percent_at_row(&self, row: u64) -> f64;
}

impl DocumentSource for FileSession {
    fn path(&self) -> &Path {
        FileSession::path(self)
    }

    fn encoding(&self) -> FileEncoding {
        FileSession::encoding(self)
    }

    fn rows_total(&self) -> u64 {
        FileSession::rows_total(self)
    }

    fn byte_len(&self) -> u64 {
        FileSession::byte_len(self)
    }

    fn fetch_rows(&self, start_row: u64, count: usize) -> Vec<RowText> {
        FileSession::rows(self, start_row, count)
    }

    fn percent_at_row(&self, row: u64) -> f64 {
        FileSession::percent_at_row(self, row)
    }
}

impl DocumentSource for EditDoc {
    fn path(&self) -> &Path {
        EditDoc::path(self)
    }

    fn encoding(&self) -> FileEncoding {
        EditDoc::encoding(self)
    }

    fn rows_total(&self) -> u64 {
        EditDoc::display_rows_total(self)
    }

    fn byte_len(&self) -> u64 {
        EditDoc::byte_len(self)
    }

    fn fetch_rows(&self, start_row: u64, count: usize) -> Vec<RowText> {
        EditDoc::fetch_display_rows(self, start_row, count)
    }

    fn percent_at_row(&self, row: u64) -> f64 {
        EditDoc::percent_at_seg(self, row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 契约一致性：两实现（只读 / 编辑）经 trait 对象取数结果一致。
    #[test]
    fn trait_objects_agree_across_implementations() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "alpha\nbeta\ngamma\n").expect("写测试文件失败");

        let session = FileSession::open(&path, None, 10).expect("打开会话失败");
        let doc = EditDoc::open(&path, None, 10).expect("打开编辑文档失败");
        let sources: [&dyn DocumentSource; 2] = [&session, &doc];

        for source in sources {
            assert_eq!(source.rows_total(), 3);
            assert_eq!(source.byte_len(), 17);
            assert_eq!(source.encoding(), FileEncoding::Utf8);
            let rows = source.fetch_rows(1, 2);
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].text, "beta");
            assert_eq!(rows[1].text, "gamma");
        }
        let session_percent = sources[0].percent_at_row(1);
        let doc_percent = sources[1].percent_at_row(1);
        assert!(
            (session_percent - doc_percent).abs() < 1e-9,
            "两实现百分比不一致：session={session_percent}, doc={doc_percent}"
        );
        assert!(
            (doc_percent - 6.0 / 17.0 * 100.0).abs() < 1e-9,
            "行首百分比不符合预期：doc={doc_percent}, 期望={}",
            6.0 / 17.0 * 100.0
        );
    }

    /// 越界起点返回空列表（契约边界）。
    #[test]
    fn out_of_range_start_returns_empty() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "x\n").expect("写测试文件失败");
        let session = FileSession::open(&path, None, 10).expect("打开会话失败");
        let source: &dyn DocumentSource = &session;
        assert!(source.fetch_rows(10, 5).is_empty());
    }
}
