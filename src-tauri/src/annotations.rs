//! 标注存储（P2-3：书签 / 高亮 / 注释·待办·行内批注）。
//!
//! 设计（D59/D63）：
//! - 按源文件路径哈希分文件持久化于 `data/annotations/<hash>.json`（便携目录内）；
//! - 锚点坐标 = **显示行**（与渲染/编辑坐标系统一，超长行分段后仍稳定）+ 行内 UTF-16 偏移；
//! - **锚点跟随编辑**：每个锚点随存一小段引用摘录（excerpt）；读取时在当前文档中
//!   于提示行附近窗口搜索摘录并校正位置（找不到则保留原坐标，绝不丢弃数据）；
//! - 每类容量上限与字符串上限（防配置膨胀）；所有写入走原子写。

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage::json_io;
use crate::textfile::source::DocumentSource;

/// 文件内 schema 版本（结构变更时递增并提供迁移）。
pub const ANNOTATIONS_SCHEMA_VERSION: u32 = 1;

/// 每类（书签/高亮/注释）容量上限。
pub const MAX_PER_KIND: usize = 10_000;
/// 注释文本上限（字符）。
pub const NOTE_MAX_CHARS: usize = 4_000;
/// 书签标签上限（字符）。
pub const LABEL_MAX_CHARS: usize = 200;
/// 引用摘录长度（字符，用于锚点跟随编辑）。
pub const EXCERPT_CHARS: usize = 24;
/// 锚点解析搜索窗口（提示行上下限，行）。
pub const RESOLVE_WINDOW_ROWS: u64 = 2_048;

/// 注释种类：普通注释 / 待办 / 行内批注。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteKind {
    /// 普通注释
    Note,
    /// 待办事项（带完成状态）
    Todo,
    /// 行内批注（锚定选区内）
    Inline,
}

/// 书签：锚定到单一位置（行列 + 行内 UTF-16 偏移）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    /// 文件内唯一 id（持久化自增）
    pub id: u64,
    /// 显示行号（0 基）
    pub row: u64,
    /// 行内 UTF-16 偏移
    pub utf16: u64,
    /// 可选标签
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 创建时间（RFC 3339，UTC）
    pub created_at: String,
    /// 引用摘录（锚点校验与重定位）
    #[serde(default)]
    pub excerpt: String,
}

/// 高亮：锚定到行内区间 `[start_utf16, end_utf16)`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Highlight {
    /// 文件内唯一 id
    pub id: u64,
    /// 显示行号（0 基）
    pub row: u64,
    /// 区间起点（行内 UTF-16）
    pub start_utf16: u64,
    /// 区间终点（行内 UTF-16，半开）
    pub end_utf16: u64,
    /// 颜色（`#RRGGBB` 等；空 = 主题默认高亮色）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// 附注文本（可选）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// 创建时间（RFC 3339，UTC）
    pub created_at: String,
    /// 引用摘录
    #[serde(default)]
    pub excerpt: String,
}

/// 注释 / 待办 / 行内批注：位置或区间 + 文本。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    /// 文件内唯一 id
    pub id: u64,
    /// 显示行号（0 基）
    pub row: u64,
    /// 行内 UTF-16 偏移（位置锚点；区间批注时为起点）
    pub utf16: u64,
    /// 区间终点（行内 UTF-16；`None` = 单点注释）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_utf16: Option<u64>,
    /// 注释文本
    pub text: String,
    /// 待办完成状态（仅 `Todo` 有意义）
    #[serde(default)]
    pub done: bool,
    /// 注释种类
    pub kind: NoteKind,
    /// 创建时间（RFC 3339，UTC）
    pub created_at: String,
    /// 引用摘录
    #[serde(default)]
    pub excerpt: String,
}

/// 单文件标注集合（对应一个 `<hash>.json` 文件）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAnnotations {
    /// schema 版本
    pub schema_version: u32,
    /// 源文件路径（原样保存，用于哈希校验与展示）
    pub path: String,
    /// 下一个可用 id（持久化自增）
    pub next_id: u64,
    /// 书签
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    /// 高亮
    #[serde(default)]
    pub highlights: Vec<Highlight>,
    /// 注释 / 待办 / 行内批注
    #[serde(default)]
    pub notes: Vec<Note>,
}

impl Default for FileAnnotations {
    fn default() -> Self {
        Self::empty("")
    }
}

impl FileAnnotations {
    /// 空集合（绑定到指定源文件）。
    pub fn empty(path: &str) -> Self {
        Self {
            schema_version: ANNOTATIONS_SCHEMA_VERSION,
            path: path.to_string(),
            next_id: 1,
            bookmarks: Vec::new(),
            highlights: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// 分配下一个 id。
    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id.saturating_add(1);
        id
    }

    /// 加载后的归一：版本对齐、id 自增校正、排序、去重、容量与字符串上限。
    pub fn normalize(&mut self) {
        self.schema_version = ANNOTATIONS_SCHEMA_VERSION;
        let max_id = self
            .bookmarks
            .iter()
            .map(|item| item.id)
            .chain(self.highlights.iter().map(|item| item.id))
            .chain(self.notes.iter().map(|item| item.id))
            .max()
            .unwrap_or(0);
        self.next_id = self.next_id.max(max_id.saturating_add(1)).max(1);

        self.bookmarks.sort_by_key(|item| (item.row, item.utf16));
        self.bookmarks
            .dedup_by(|a, b| a.id == b.id || (a.row == b.row && a.utf16 == b.utf16));
        self.bookmarks.truncate(MAX_PER_KIND);
        for item in &mut self.bookmarks {
            if let Some(label) = &mut item.label {
                *label = truncate_chars(label.trim(), LABEL_MAX_CHARS);
                if label.is_empty() {
                    item.label = None;
                }
            }
            item.excerpt = truncate_chars(&item.excerpt, EXCERPT_CHARS);
        }

        self.highlights
            .sort_by_key(|item| (item.row, item.start_utf16));
        self.highlights.dedup_by(|a, b| {
            a.id == b.id
                || (a.row == b.row && a.start_utf16 == b.start_utf16 && a.end_utf16 == b.end_utf16)
        });
        self.highlights.truncate(MAX_PER_KIND);
        for item in &mut self.highlights {
            if item.end_utf16 < item.start_utf16 {
                std::mem::swap(&mut item.end_utf16, &mut item.start_utf16);
            }
            if let Some(note) = &mut item.note {
                *note = truncate_chars(note.trim(), NOTE_MAX_CHARS);
                if note.is_empty() {
                    item.note = None;
                }
            }
            item.excerpt = truncate_chars(&item.excerpt, EXCERPT_CHARS);
        }

        self.notes.sort_by_key(|item| (item.row, item.utf16));
        self.notes.dedup_by(|a, b| a.id == b.id);
        self.notes.truncate(MAX_PER_KIND);
        for item in &mut self.notes {
            item.text = truncate_chars(item.text.trim(), NOTE_MAX_CHARS);
            item.excerpt = truncate_chars(&item.excerpt, EXCERPT_CHARS);
        }
        self.notes.retain(|item| !item.text.is_empty());
    }
}

/// 按字符数截断（不破坏 UTF-8）。
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

/// 源文件路径 → 标注文件名哈希（Windows 路径不区分大小写；FNV-1a 双流 128 位）。
pub fn path_hash(source_path: &str) -> String {
    let normalized = source_path.to_lowercase();
    let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
    let mut h2: u64 = 0x8422_2325_cbf2_9ce4;
    for byte in normalized.as_bytes() {
        h1 ^= u64::from(*byte);
        h1 = h1.wrapping_mul(0x100_0000_01b3);
        h2 = h2.rotate_left(5) ^ u64::from(*byte);
        h2 = h2.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h1:016x}{h2:016x}")
}

/// 标注目录：`<data>/annotations`。
pub fn annotations_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("annotations")
}

/// 单文件标注路径：`<data>/annotations/<hash>.json`。
pub fn annotation_file_path(data_dir: &Path, source_path: &str) -> PathBuf {
    annotations_dir(data_dir).join(format!("{}.json", path_hash(source_path)))
}

/// 读取单文件标注（缺失或损坏 → 空集合；损坏文件由 json_io 备份为 `.corrupt-*`）。
pub fn load(data_dir: &Path, source_path: &str) -> FileAnnotations {
    let path = annotation_file_path(data_dir, source_path);
    let mut annotations = json_io::load_json_or_default(&path, |item: &mut FileAnnotations| {
        item.normalize();
    });
    if annotations.path.is_empty() {
        annotations.path = source_path.to_string();
    }
    // 路径一致性防御：同一哈希但路径不同（理论碰撞）→ 视为空集合，避免串档。
    if annotations.path != source_path {
        log::warn!(
            "标注文件路径不匹配（哈希碰撞？）：{} != {}",
            annotations.path,
            source_path
        );
        annotations = FileAnnotations::empty(source_path);
    }
    annotations
}

/// 保存单文件标注（归一 + 原子写）。
pub fn save(data_dir: &Path, annotations: &mut FileAnnotations) -> std::io::Result<()> {
    annotations.normalize();
    let path = annotation_file_path(data_dir, &annotations.path.clone());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    json_io::write_json_atomic(&path, annotations)
}

/// 计算行内 UTF-16 偏移对应的引用摘录（不足则取至行尾；行文本缺失返回空串）。
fn excerpt_at(row: u64, utf16: u64, source: &dyn DocumentSource) -> String {
    let Some(text) = row_text(source, row) else {
        return String::new();
    };
    let units: Vec<u16> = text.encode_utf16().collect();
    let start = (utf16 as usize).min(units.len());
    String::from_utf16_lossy(&units[start..(start + EXCERPT_CHARS).min(units.len())])
}

/// 取行文本（经 `DocumentSource::fetch_rows` 单行窗口）。
fn row_text(source: &dyn DocumentSource, row: u64) -> Option<String> {
    if row >= source.rows_total() {
        return None;
    }
    let rows = source.fetch_rows(row, 1);
    rows.into_iter().next().map(|item| item.text)
}

/// 搜索窗口内查找摘录并返回 (行, 行内 UTF-16 起点)；找不到返回 None。
fn locate_excerpt(source: &dyn DocumentSource, hint_row: u64, excerpt: &str) -> Option<(u64, u64)> {
    if excerpt.is_empty() {
        return None;
    }
    let total = source.rows_total();
    let start = hint_row.saturating_sub(RESOLVE_WINDOW_ROWS);
    let end = (hint_row + RESOLVE_WINDOW_ROWS).min(total.saturating_sub(1));
    let mut row = start;
    while row <= end {
        let batch = source.fetch_rows(row, 512);
        if batch.is_empty() {
            break;
        }
        for item in &batch {
            if item.text.contains(excerpt) {
                let byte_pos = item.text.find(excerpt)?;
                let utf16_pos = item.text[..byte_pos].encode_utf16().count() as u64;
                return Some((item.row, utf16_pos));
            }
        }
        row = batch.last()?.row + 1;
    }
    None
}

/// 锚点解析：按引用摘录在文档中重定位全部标注；返回是否发生校正。
///
/// 规则：
/// - 摘录为空（如创建时行文本不可得）→ 保留原坐标；
/// - 在提示行 ±[`RESOLVE_WINDOW_ROWS`] 内找到摘录 → 校正行与偏移；
/// - 找不到 → 保留原坐标（不丢数据，由界面按当前行渲染或提示）。
pub fn resolve_against(annotations: &mut FileAnnotations, source: &dyn DocumentSource) -> bool {
    let mut changed = false;
    for bookmark in &mut annotations.bookmarks {
        if let Some((row, utf16)) = locate_excerpt(source, bookmark.row, &bookmark.excerpt) {
            if row != bookmark.row || utf16 != bookmark.utf16 {
                bookmark.row = row;
                bookmark.utf16 = utf16;
                changed = true;
            }
        }
    }
    for highlight in &mut annotations.highlights {
        if let Some((row, utf16)) = locate_excerpt(source, highlight.row, &highlight.excerpt) {
            if row != highlight.row || utf16 != highlight.start_utf16 {
                let len = highlight.end_utf16.saturating_sub(highlight.start_utf16);
                highlight.row = row;
                highlight.start_utf16 = utf16;
                highlight.end_utf16 = utf16 + len;
                changed = true;
            }
        }
    }
    for note in &mut annotations.notes {
        if let Some((row, utf16)) = locate_excerpt(source, note.row, &note.excerpt) {
            if row != note.row || utf16 != note.utf16 {
                if let Some(end) = note.end_utf16 {
                    let len = end.saturating_sub(note.utf16);
                    note.end_utf16 = Some(utf16 + len);
                }
                note.row = row;
                note.utf16 = utf16;
                changed = true;
            }
        }
    }
    changed
}

/// 添加书签（重复位置去重返回既有项）。
pub fn add_bookmark<'a>(
    annotations: &'a mut FileAnnotations,
    source: &dyn DocumentSource,
    row: u64,
    utf16: u64,
    label: Option<String>,
) -> &'a Bookmark {
    if let Some(index) = annotations
        .bookmarks
        .iter()
        .position(|item| item.row == row && item.utf16 == utf16)
    {
        return &annotations.bookmarks[index];
    }
    let bookmark = Bookmark {
        id: annotations.alloc_id(),
        row,
        utf16,
        label: label
            .map(|text| truncate_chars(text.trim(), LABEL_MAX_CHARS))
            .filter(|text| !text.is_empty()),
        created_at: crate::time_util::now_rfc3339(),
        excerpt: excerpt_at(row, utf16, source),
    };
    annotations.bookmarks.push(bookmark);
    annotations.normalize();
    annotations
        .bookmarks
        .iter()
        .find(|item| item.row == row && item.utf16 == utf16)
        .expect("刚插入")
}

/// 删除书签（按 id；返回是否删除）。
pub fn remove_bookmark(annotations: &mut FileAnnotations, id: u64) -> bool {
    let before = annotations.bookmarks.len();
    annotations.bookmarks.retain(|item| item.id != id);
    annotations.bookmarks.len() != before
}

/// 添加高亮（相同区间去重；`end <= start` 拒绝返回 None）。
pub fn add_highlight<'a>(
    annotations: &'a mut FileAnnotations,
    source: &dyn DocumentSource,
    row: u64,
    start_utf16: u64,
    end_utf16: u64,
    color: Option<String>,
) -> Option<&'a Highlight> {
    if end_utf16 <= start_utf16 {
        return None;
    }
    if let Some(index) = annotations.highlights.iter().position(|item| {
        item.row == row && item.start_utf16 == start_utf16 && item.end_utf16 == end_utf16
    }) {
        return Some(&annotations.highlights[index]);
    }
    let highlight = Highlight {
        id: annotations.alloc_id(),
        row,
        start_utf16,
        end_utf16,
        color: color
            .map(|value| truncate_chars(value.trim(), 32))
            .filter(|value| !value.is_empty()),
        note: None,
        created_at: crate::time_util::now_rfc3339(),
        excerpt: excerpt_at(row, start_utf16, source),
    };
    annotations.highlights.push(highlight);
    annotations.normalize();
    annotations.highlights.iter().find(|item| {
        item.row == row && item.start_utf16 == start_utf16 && item.end_utf16 == end_utf16
    })
}

/// 删除高亮（按 id；返回是否删除）。
pub fn remove_highlight(annotations: &mut FileAnnotations, id: u64) -> bool {
    let before = annotations.highlights.len();
    annotations.highlights.retain(|item| item.id != id);
    annotations.highlights.len() != before
}

/// 添加注释 / 待办 / 行内批注（空文本拒绝返回 None）。
pub fn add_note<'a>(
    annotations: &'a mut FileAnnotations,
    source: &dyn DocumentSource,
    row: u64,
    utf16: u64,
    end_utf16: Option<u64>,
    text: String,
    kind: NoteKind,
) -> Option<&'a Note> {
    let text = truncate_chars(text.trim(), NOTE_MAX_CHARS);
    if text.is_empty() {
        return None;
    }
    let note = Note {
        id: annotations.alloc_id(),
        row,
        utf16,
        end_utf16: end_utf16.filter(|end| *end > utf16),
        text,
        done: false,
        kind,
        created_at: crate::time_util::now_rfc3339(),
        excerpt: excerpt_at(row, utf16, source),
    };
    annotations.notes.push(note.clone());
    annotations.normalize();
    annotations.notes.iter().find(|item| item.id == note.id)
}

/// 更新注释文本与完成状态（不含 id 的裁剪；返回是否更新）。
pub fn update_note(
    annotations: &mut FileAnnotations,
    id: u64,
    text: Option<String>,
    done: Option<bool>,
) -> bool {
    let Some(note) = annotations.notes.iter_mut().find(|item| item.id == id) else {
        return false;
    };
    if let Some(text) = text {
        let text = truncate_chars(text.trim(), NOTE_MAX_CHARS);
        if text.is_empty() {
            return false;
        }
        note.text = text;
    }
    if let Some(done) = done {
        note.done = done;
    }
    true
}

/// 删除注释（按 id；返回是否删除）。
pub fn remove_note(annotations: &mut FileAnnotations, id: u64) -> bool {
    let before = annotations.notes.len();
    annotations.notes.retain(|item| item.id != id);
    annotations.notes.len() != before
}

/// 清空单文件全部标注（返回是否有删除）。
pub fn clear_all(annotations: &mut FileAnnotations) -> bool {
    let had = !annotations.bookmarks.is_empty()
        || !annotations.highlights.is_empty()
        || !annotations.notes.is_empty();
    annotations.bookmarks.clear();
    annotations.highlights.clear();
    annotations.notes.clear();
    had
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::textfile::editing::edit_doc::EditDoc;

    /// 构造带内容的编辑文档（临时目录内实体文件）。
    fn doc(dir: &Path, text: &str) -> EditDoc {
        let path = dir.join("source.txt");
        std::fs::write(&path, text).expect("写源文件");
        EditDoc::open(&path, None, 4096).expect("打开编辑文档")
    }

    /// 路径哈希：稳定、大小写不敏感、不同路径不同。
    #[test]
    fn path_hash_is_stable_and_case_insensitive() {
        let a = path_hash("D:\\Docs\\A.txt");
        let b = path_hash("d:\\docs\\a.txt");
        let c = path_hash("D:\\Docs\\B.txt");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 32);
    }

    /// CRUD + 持久化往返。
    #[test]
    fn crud_roundtrip_persists() {
        let dir = tempfile::tempdir().expect("临时目录");
        let source = doc(dir.path(), "alpha beta\ngamma delta\n");
        let mut annotations = FileAnnotations::empty("D:\\Docs\\A.txt");
        add_bookmark(&mut annotations, &source, 1, 6, Some("重要".into()));
        add_highlight(&mut annotations, &source, 0, 0, 5, Some("#FFEE00".into()));
        add_note(
            &mut annotations,
            &source,
            1,
            0,
            None,
            "看一下".into(),
            NoteKind::Todo,
        );
        save(dir.path(), &mut annotations).expect("保存");
        let loaded = load(dir.path(), "D:\\Docs\\A.txt");
        assert_eq!(loaded.bookmarks.len(), 1);
        assert_eq!(loaded.highlights.len(), 1);
        assert_eq!(loaded.notes.len(), 1);
        assert_eq!(loaded.bookmarks[0].excerpt, "delta");
        assert_eq!(loaded.highlights[0].excerpt, "alpha beta");
        assert_eq!(loaded.notes[0].kind, NoteKind::Todo);
    }

    /// 锚点跟随编辑：上方插入行后解析校正。
    #[test]
    fn resolve_tracks_edits_above() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut source = doc(dir.path(), "alpha beta\ngamma delta\nepsilon zeta\n");
        let mut annotations = FileAnnotations::empty("X.txt");
        add_bookmark(&mut annotations, &source, 2, 0, None);
        assert_eq!(annotations.bookmarks[0].excerpt, "epsilon zeta");
        // 在第 0 行前插入两行，原第 2 行应移动到第 4 行。
        source
            .apply_edits(&[crate::textfile::editing::edit_doc::EditOp::Insert {
                row: 0,
                utf16: 0,
                text: "new1\nnew2\n".into(),
            }])
            .expect("编辑");
        let changed = resolve_against(&mut annotations, &source);
        assert!(changed);
        assert_eq!(annotations.bookmarks[0].row, 4);
        assert_eq!(annotations.bookmarks[0].utf16, 0);
    }

    /// 摘录消失（内容被改写）→ 保留原坐标不丢数据。
    #[test]
    fn resolve_keeps_position_when_excerpt_lost() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut source = doc(dir.path(), "alpha beta\ngamma delta\n");
        let mut annotations = FileAnnotations::empty("X.txt");
        add_bookmark(&mut annotations, &source, 1, 6, None);
        source
            .apply_edits(&[crate::textfile::editing::edit_doc::EditOp::Replace {
                start_row: 1,
                start_utf16: 0,
                end_row: 1,
                end_utf16: 11,
                text: "完全改写".into(),
            }])
            .expect("编辑");
        let changed = resolve_against(&mut annotations, &source);
        assert!(!changed);
        assert_eq!(annotations.bookmarks[0].row, 1);
        assert_eq!(annotations.bookmarks[0].utf16, 6);
    }

    /// 去重与容量/字符串上限。
    #[test]
    fn dedupe_and_limits() {
        let dir = tempfile::tempdir().expect("临时目录");
        let source = doc(dir.path(), "aaaaaaaaaa\n");
        let mut annotations = FileAnnotations::empty("X.txt");
        add_bookmark(&mut annotations, &source, 0, 3, None);
        add_bookmark(&mut annotations, &source, 0, 3, Some("重复".into()));
        assert_eq!(annotations.bookmarks.len(), 1);
        let long = "字".repeat(NOTE_MAX_CHARS + 100);
        add_note(&mut annotations, &source, 0, 0, None, long, NoteKind::Note);
        assert_eq!(annotations.notes[0].text.chars().count(), NOTE_MAX_CHARS);
        assert!(add_note(
            &mut annotations,
            &source,
            0,
            0,
            None,
            "   ".into(),
            NoteKind::Note
        )
        .is_none());
        assert!(add_highlight(&mut annotations, &source, 0, 5, 5, None).is_none());
    }

    /// 更新与删除。
    #[test]
    fn update_and_remove() {
        let dir = tempfile::tempdir().expect("临时目录");
        let source = doc(dir.path(), "line one\nline two\n");
        let mut annotations = FileAnnotations::empty("X.txt");
        let id = add_note(
            &mut annotations,
            &source,
            0,
            0,
            None,
            "待办".into(),
            NoteKind::Todo,
        )
        .expect("添加")
        .id;
        assert!(update_note(
            &mut annotations,
            id,
            Some("办完了".into()),
            Some(true)
        ));
        assert_eq!(annotations.notes[0].text, "办完了");
        assert!(annotations.notes[0].done);
        assert!(remove_note(&mut annotations, id));
        assert!(annotations.notes.is_empty());
        add_bookmark(&mut annotations, &source, 1, 0, None);
        let bookmark_id = annotations.bookmarks[0].id;
        assert!(remove_bookmark(&mut annotations, bookmark_id));
        assert!(clear_all(&mut annotations) == false);
    }

    /// 路径不匹配（哈希碰撞防御）→ 返回空集合。
    #[test]
    fn path_mismatch_returns_empty() {
        let dir = tempfile::tempdir().expect("临时目录");
        let mut annotations = FileAnnotations::empty("A.txt");
        save(dir.path(), &mut annotations).expect("保存");
        // 直接改写文件内容模拟碰撞：把内部 path 改为 B.txt
        let path = annotation_file_path(dir.path(), "A.txt");
        let raw = std::fs::read_to_string(&path).expect("读取");
        std::fs::write(&path, raw.replace("A.txt", "B.txt")).expect("改写");
        let loaded = load(dir.path(), "A.txt");
        assert!(loaded.bookmarks.is_empty());
        assert_eq!(loaded.path, "A.txt");
    }
}
