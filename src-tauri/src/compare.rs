//! 比较/合并窗口的文档状态与数据装配：
//! 加载两侧（或三方）文件的解码会话、行哈希与差异/合并结果缓存，
//! 并按需提供行文本窗口（前端虚拟列表按可见区间取行）。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::diff::{diff_lines, DiffError, DiffResult, HunkKind};
use crate::merge3::{merge3, LineSpan, MergeRegion, MergeResult, MergedSource, RegionKind};
use crate::textfile::session::FileSession;
use crate::textfile::session::TextFileError;

/// 比较窗口 label。
pub const COMPARE_WINDOW: &str = "compare";
/// 比较请求更新事件（已存在的比较窗口接收新请求）。
pub const EVENT_COMPARE_REQUEST: &str = "srt://compare-request";
/// 比较加载的文件大小上限（MB）。
pub const COMPARE_MAX_MB: u32 = 512;
const LOAD_BATCH_ROWS: usize = 4096;

/// 比较模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompareMode {
    Diff,
    Merge,
}

/// 打开比较窗口的请求。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRequest {
    pub mode: CompareMode,
    /// 双栏比较的左文件；三方合并的底本（base）。
    pub left: String,
    /// 双栏比较的右文件；三方合并的我方（ours）。
    pub right: String,
    /// 三方合并的他方（theirs）；双栏比较缺省。
    pub extra: Option<String>,
}

/// 行文本来源侧。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompareSide {
    Left,
    Right,
    Extra,
}

/// 比较错误。
#[derive(Debug)]
pub enum CompareError {
    Text(TextFileError),
    Diff(DiffError),
    /// 尚未加载文档（或请求了未加载的侧）。
    NotLoaded,
}

impl std::fmt::Display for CompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompareError::Text(err) => match err {
                TextFileError::NotFound(path) => {
                    write!(f, "文件不存在或不可访问：{}", path.display())
                }
                TextFileError::TooLarge {
                    size_bytes,
                    limit_mb,
                } => {
                    let size_mb = *size_bytes as f64 / (1024.0 * 1024.0);
                    write!(f, "文件过大：{size_mb:.1} MB，超过上限 {limit_mb} MB")
                }
                TextFileError::Io(err) => write!(f, "读取文件失败：{err}"),
            },
            CompareError::Diff(err) => write!(f, "{err}"),
            CompareError::NotLoaded => write!(f, "比较文档尚未加载"),
        }
    }
}

impl From<TextFileError> for CompareError {
    fn from(err: TextFileError) -> Self {
        CompareError::Text(err)
    }
}

impl From<DiffError> for CompareError {
    fn from(err: DiffError) -> Self {
        CompareError::Diff(err)
    }
}

/// 已加载的比较文档。
pub struct LoadedDoc {
    pub name: String,
    session: FileSession,
    pub hashes: Vec<u64>,
}

impl LoadedDoc {
    fn load(path: &Path) -> Result<Self, CompareError> {
        let session = FileSession::open(path, None, COMPARE_MAX_MB)?;
        let mut hashes = Vec::with_capacity(session.rows_total() as usize);
        let mut row = 0u64;
        while row < session.rows_total() {
            let batch = session.rows(row, LOAD_BATCH_ROWS);
            if batch.is_empty() {
                break;
            }
            for item in &batch {
                hashes.push(crate::diff::hash_line(item.text.as_bytes()));
            }
            row += batch.len() as u64;
        }
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        Ok(LoadedDoc {
            name,
            session,
            hashes,
        })
    }

    fn rows(&self, start: u64, count: usize) -> Vec<String> {
        self.session
            .rows(start, count)
            .into_iter()
            .map(|item| item.text)
            .collect()
    }
}

/// 差异块 DTO。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunkDto {
    pub kind: &'static str,
    pub left_start: u64,
    pub left_len: u64,
    pub right_start: u64,
    pub right_len: u64,
}

/// 双栏比较加载结果。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffDocsDto {
    pub left_name: String,
    pub right_name: String,
    pub left_rows: u64,
    pub right_rows: u64,
    pub added: u64,
    pub removed: u64,
    pub hunks: Vec<DiffHunkDto>,
}

/// 合并区域 DTO。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionDto {
    pub kind: &'static str,
    pub base_range: LineSpan,
    pub ours_range: LineSpan,
    pub theirs_range: LineSpan,
    pub merged: Option<MergedDto>,
}

/// 非冲突区域的合并来源 DTO。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedDto {
    pub source: &'static str,
    pub start: u64,
    pub end: u64,
}

/// 三方合并加载结果。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeDocsDto {
    pub base_name: String,
    pub ours_name: String,
    pub theirs_name: String,
    pub base_rows: u64,
    pub ours_rows: u64,
    pub theirs_rows: u64,
    pub conflicts: u32,
    pub auto_merged_lines: u64,
    pub regions: Vec<RegionDto>,
}

fn hunk_kind_name(kind: HunkKind) -> &'static str {
    match kind {
        HunkKind::Equal => "equal",
        HunkKind::Change => "change",
        HunkKind::Delete => "delete",
        HunkKind::Insert => "insert",
    }
}

fn region_kind_name(kind: RegionKind) -> &'static str {
    match kind {
        RegionKind::Stable => "stable",
        RegionKind::OursOnly => "oursOnly",
        RegionKind::TheirsOnly => "theirsOnly",
        RegionKind::SameChange => "sameChange",
        RegionKind::Conflict => "conflict",
    }
}

fn merged_source_name(source: MergedSource) -> &'static str {
    match source {
        MergedSource::Base => "base",
        MergedSource::Ours => "ours",
        MergedSource::Theirs => "theirs",
    }
}

fn region_dto(region: &MergeRegion) -> RegionDto {
    RegionDto {
        kind: region_kind_name(region.kind),
        base_range: region.base_range,
        ours_range: region.ours_range,
        theirs_range: region.theirs_range,
        merged: region.merged.map(|(source, (start, end))| MergedDto {
            source: merged_source_name(source),
            start,
            end,
        }),
    }
}

/// 比较状态（窗口级单例；重新加载会整体替换）。
#[derive(Default)]
pub struct CompareState {
    inner: Mutex<CompareInner>,
}

#[derive(Default)]
struct CompareInner {
    request: Option<CompareRequest>,
    left: Option<LoadedDoc>,
    right: Option<LoadedDoc>,
    extra: Option<LoadedDoc>,
    diff: Option<DiffResult>,
    merge: Option<MergeResult>,
}

impl CompareState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录待处理请求（窗口未创建时由 `take_request` 取走）。
    pub fn set_request(&self, request: CompareRequest) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.request = Some(request);
        }
    }

    /// 取走待处理请求（窗口启动时调用）。
    pub fn take_request(&self) -> Option<CompareRequest> {
        self.inner.lock().ok().and_then(|mut guard| guard.request.take())
    }

    /// 加载两个文件并计算行级差异。
    pub fn load_diff(&self, left: &str, right: &str) -> Result<DiffDocsDto, CompareError> {
        let left_doc = LoadedDoc::load(Path::new(left))?;
        let right_doc = LoadedDoc::load(Path::new(right))?;
        let diff = diff_lines(&left_doc.hashes, &right_doc.hashes)?;
        let dto = DiffDocsDto {
            left_name: left_doc.name.clone(),
            right_name: right_doc.name.clone(),
            left_rows: left_doc.hashes.len() as u64,
            right_rows: right_doc.hashes.len() as u64,
            added: diff.added,
            removed: diff.removed,
            hunks: diff
                .hunks
                .iter()
                .map(|hunk| DiffHunkDto {
                    kind: hunk_kind_name(hunk.kind),
                    left_start: hunk.left_start,
                    left_len: hunk.left_len,
                    right_start: hunk.right_start,
                    right_len: hunk.right_len,
                })
                .collect(),
        };
        let mut guard = self.inner.lock().map_err(|_| CompareError::NotLoaded)?;
        guard.left = Some(left_doc);
        guard.right = Some(right_doc);
        guard.extra = None;
        guard.diff = Some(diff);
        guard.merge = None;
        Ok(dto)
    }

    /// 加载三个文件并计算三方合并。
    pub fn load_merge(
        &self,
        base: &str,
        ours: &str,
        theirs: &str,
    ) -> Result<MergeDocsDto, CompareError> {
        let base_doc = LoadedDoc::load(Path::new(base))?;
        let ours_doc = LoadedDoc::load(Path::new(ours))?;
        let theirs_doc = LoadedDoc::load(Path::new(theirs))?;
        let merge = merge3(&base_doc.hashes, &ours_doc.hashes, &theirs_doc.hashes)?;
        let dto = MergeDocsDto {
            base_name: base_doc.name.clone(),
            ours_name: ours_doc.name.clone(),
            theirs_name: theirs_doc.name.clone(),
            base_rows: base_doc.hashes.len() as u64,
            ours_rows: ours_doc.hashes.len() as u64,
            theirs_rows: theirs_doc.hashes.len() as u64,
            conflicts: merge.conflicts,
            auto_merged_lines: merge.auto_merged_lines,
            regions: merge.regions.iter().map(region_dto).collect(),
        };
        let mut guard = self.inner.lock().map_err(|_| CompareError::NotLoaded)?;
        guard.left = Some(base_doc);
        guard.right = Some(ours_doc);
        guard.extra = Some(theirs_doc);
        guard.diff = None;
        guard.merge = Some(merge);
        Ok(dto)
    }

    /// 取侧行文本窗口。
    pub fn rows(&self, side: CompareSide, start: u64, count: usize) -> Result<Vec<String>, CompareError> {
        let guard = self.inner.lock().map_err(|_| CompareError::NotLoaded)?;
        let doc = match side {
            CompareSide::Left => guard.left.as_ref(),
            CompareSide::Right => guard.right.as_ref(),
            CompareSide::Extra => guard.extra.as_ref(),
        };
        doc.map(|doc| doc.rows(start, count)).ok_or(CompareError::NotLoaded)
    }

    /// 合并结果引用（写回时取来源行）。
    pub fn merge_snapshot(&self) -> Result<Option<(MergeResult, Vec<Vec<u64>>, Vec<PathBuf>)>, CompareError> {
        let guard = self.inner.lock().map_err(|_| CompareError::NotLoaded)?;
        let Some(merge) = guard.merge.clone() else {
            return Ok(None);
        };
        let docs = [guard.left.as_ref(), guard.right.as_ref(), guard.extra.as_ref()];
        let mut hashes = Vec::new();
        let mut paths = Vec::new();
        for doc in docs.into_iter().flatten() {
            hashes.push(doc.hashes.clone());
            paths.push(doc.session.path().to_path_buf());
        }
        Ok(Some((merge, hashes, paths)))
    }

    /// 三方合并的源文件路径（base, ours, theirs）。
    pub fn merge_paths(&self) -> Result<Option<(String, String, String)>, CompareError> {
        let guard = self.inner.lock().map_err(|_| CompareError::NotLoaded)?;
        match (&guard.left, &guard.right, &guard.extra) {
            (Some(base), Some(ours), Some(theirs)) => Ok(Some((
                base.session.path().to_string_lossy().into_owned(),
                ours.session.path().to_string_lossy().into_owned(),
                theirs.session.path().to_string_lossy().into_owned(),
            ))),
            _ => Ok(None),
        }
    }

    /// 取合并输出行（按来源与区间）。
    pub fn merge_rows(
        &self,
        source: MergedSource,
        start: u64,
        count: usize,
    ) -> Result<Vec<String>, CompareError> {
        let side = match source {
            MergedSource::Base => CompareSide::Left,
            MergedSource::Ours => CompareSide::Right,
            MergedSource::Theirs => CompareSide::Extra,
        };
        self.rows(side, start, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
        path
    }

    #[test]
    fn load_diff_reports_hunks_and_rows() {
        let dir = tempfile::tempdir().unwrap();
        let left = write_file(dir.path(), "a.txt", "one\ntwo\nthree\n");
        let right = write_file(dir.path(), "b.txt", "one\nTWO\nthree\nfour\n");
        let state = CompareState::new();
        let dto = state
            .load_diff(&left.to_string_lossy(), &right.to_string_lossy())
            .unwrap();
        assert_eq!(dto.left_name, "a.txt");
        assert_eq!(dto.right_rows, 4);
        assert!(dto.hunks.iter().any(|h| h.kind == "change"));
        assert_eq!(dto.added, 2);
        assert_eq!(dto.removed, 1);
        let rows = state.rows(CompareSide::Right, 1, 2).unwrap();
        assert_eq!(rows, vec!["TWO".to_string(), "three".to_string()]);
    }

    #[test]
    fn load_merge_classifies_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let base = write_file(dir.path(), "base.txt", "a\nb\nc\n");
        let ours = write_file(dir.path(), "ours.txt", "a\nX\nc\n");
        let theirs = write_file(dir.path(), "theirs.txt", "a\nY\nc\n");
        let state = CompareState::new();
        let dto = state
            .load_merge(
                &base.to_string_lossy(),
                &ours.to_string_lossy(),
                &theirs.to_string_lossy(),
            )
            .unwrap();
        assert_eq!(dto.conflicts, 1);
        assert!(dto.regions.iter().any(|region| region.kind == "conflict"));
        let rows = state
            .merge_rows(MergedSource::Theirs, 1, 1)
            .unwrap();
        assert_eq!(rows, vec!["Y".to_string()]);
        let paths = state.merge_paths().unwrap().unwrap();
        assert!(paths.1.ends_with("ours.txt"));
    }

    #[test]
    fn missing_file_reports_text_error() {
        let state = CompareState::new();
        let err = state.load_diff("Z:/definitely/missing.txt", "Z:/nope.txt").unwrap_err();
        assert!(matches!(err, CompareError::Text(_)));
    }
}
