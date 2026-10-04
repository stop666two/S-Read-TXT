//! 命令行参数与文件关联（P3-3）：启动参数收集、单实例转发队列。
//!
//! 语义：
//! - `file_args` 从参数列表提取「存在的普通文件」路径（跳过开关、目录、重复项），
//!   绝对化后返回；供启动扫描与单实例回调共用。
//! - `PendingCliFiles` 为待打开队列：第二次实例把文件推入队列并触发
//!   `srt://cli-open` 事件；前端启动时经 `take_cli_files` 排空（事件与队列叠加，不丢失）。

use std::path::Path;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

/// 运行期转发事件名（前端监听后逐个打开）。
pub const CLI_OPEN_EVENT: &str = "srt://cli-open";

/// 待打开路径队列（进程级；`take` 语义天然规避启动与转发的竞态）。
#[derive(Default)]
pub struct PendingCliFiles(pub Mutex<Vec<String>>);

/// 提取参数中的可打开文件路径（保持顺序、去重、尽量绝对化）。
///
/// 规则：跳过空串与以 `-` 开头者（保留未来 CLI 开关空间）；仅接受存在的普通文件；
/// `canonicalize` 失败时保留原样；Windows `\\?\` 前缀会被剥离（显示与后续命令更友好）。
pub fn file_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Vec<String> = Vec::new();
    for raw in args {
        let text = raw.as_ref();
        if text.is_empty() || text.starts_with('-') {
            continue;
        }
        let path = Path::new(text);
        if !path.is_file() {
            continue;
        }
        let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let display = resolved
            .to_string_lossy()
            .strip_prefix(r"\\?\")
            .map(str::to_string)
            .unwrap_or_else(|| resolved.to_string_lossy().into_owned());
        if !out.contains(&display) {
            out.push(display);
        }
    }
    out
}

/// 推入待打开队列并通知前端（前端未就绪时事件自然无人接收，队列兜底不丢）。
pub fn push_pending(app: &AppHandle, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }
    if let Some(state) = app.try_state::<PendingCliFiles>() {
        if let Ok(mut pending) = state.0.lock() {
            pending.extend(paths.iter().cloned());
        }
    }
    let _ = app.emit(CLI_OPEN_EVENT, &paths);
}

/// 取走全部待打开路径（前端启动排空）。
pub fn take_pending(state: &PendingCliFiles) -> Vec<String> {
    state
        .0
        .lock()
        .map(|mut pending| std::mem::take(&mut *pending))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::file_args;

    #[test]
    fn skips_flags_dirs_and_missing_and_dedupes() {
        let dir = tempfile::tempdir().expect("临时目录");
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        std::fs::write(&file_a, b"a").expect("写 a");
        std::fs::write(&file_b, b"b").expect("写 b");
        let args = vec![
            "--flag".to_string(),
            file_a.to_string_lossy().into_owned(),
            dir.path().to_string_lossy().into_owned(),
            dir.path()
                .join("missing.txt")
                .to_string_lossy()
                .into_owned(),
            file_b.to_string_lossy().into_owned(),
            file_a.to_string_lossy().into_owned(),
        ];
        let out = file_args(args);
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(
            out[0].ends_with("a.txt") && out[1].ends_with("b.txt"),
            "{out:?}"
        );
        assert!(!out[0].starts_with(r"\\?\"), "应剥离 \\\\?\\ 前缀：{out:?}");
    }
}
