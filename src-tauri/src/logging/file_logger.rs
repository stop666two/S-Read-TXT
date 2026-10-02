//! 文件日志：JSON 行写入与大小轮转（独立于 `log` facade，便于单元测试）。
//!
//! 行格式（权威说明见 `docs/configuration.md` §2.6）：
//! `{"time":"<RFC 3339 UTC>","level":"info","module":"...","message":"...","tab":1,"req":2}`
//! - `tab`/`req` 仅在上下文存在时输出；
//! - 时间统一 UTC（RFC 3339 的 `Z` 时区标识）。
//!
//! 轮转：当前文件超过 `max_bytes` 时，`app.log` → `app.log.1` → `app.log.2` …
//! 保留文件总数受 `keep_files` 限制（默认 3：`app.log` + `.1` + `.2`）。

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::logging::context::LogContext;

/// 轮转配置。
#[derive(Debug, Clone, Copy)]
pub struct RotationConfig {
    /// 单文件大小上限（字节）；当前文件写入将超过该值时轮转
    pub max_bytes: u64,
    /// 保留文件总数（含当前 `app.log`；最小 1 = 直接截断，不留备份）
    pub keep_files: u32,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            max_bytes: 5 * 1024 * 1024,
            keep_files: 3,
        }
    }
}

/// 一条日志记录（序列化结构；写入时生成）。
#[derive(Debug, Serialize)]
struct Record<'a> {
    time: String,
    level: &'a str,
    module: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tab: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    req: Option<u64>,
}

/// 内部可变状态（互斥保护）。
struct State {
    /// 当前日志文件句柄（轮转时短暂置 None 以关闭句柄）
    file: Option<File>,
    /// 当前文件已写字节数（含打开前的存量）
    written: u64,
}

/// 文件 logger：`write_line` 可多线程调用（内部串行化）。
pub struct FileLogger {
    /// 日志目录
    dir: PathBuf,
    /// 当前日志文件路径（`<dir>/app.log`）
    path: PathBuf,
    /// 轮转配置
    rotation: RotationConfig,
    /// 受保护状态
    state: Mutex<State>,
}

impl FileLogger {
    /// 打开（或创建）日志文件；已有内容计入大小统计（保证重启后轮转仍正确）。
    pub fn open(dir: &Path, rotation: RotationConfig) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let path = dir.join("app.log");
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata()?.len();
        Ok(Self {
            dir: dir.to_path_buf(),
            path,
            rotation,
            state: Mutex::new(State {
                file: Some(file),
                written,
            }),
        })
    }

    /// 写入一行日志（自动补时间、附加链路标识、按需轮转）。
    ///
    /// 返回：`Ok(())` 已 flush；`Err` 为 IO/序列化错误（由调用侧兜底，避免日志导致崩溃）。
    pub fn write_line(
        &self,
        level: &str,
        module: &str,
        message: &str,
        log_context: LogContext,
    ) -> io::Result<()> {
        let record = Record {
            time: now_rfc3339(),
            level,
            module,
            message,
            tab: log_context.tab,
            req: log_context.req,
        };
        let mut line = serde_json::to_string(&record)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        line.push('\n');

        let mut state = self.lock_state();
        let line_len = line.len() as u64;
        if state.written > 0 && state.written + line_len > self.rotation.max_bytes {
            self.rotate_locked(&mut state)?;
        }
        let file = state
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("日志文件句柄缺失（内部状态异常）"))?;
        file.write_all(line.as_bytes())?;
        file.flush()?;
        state.written += line_len;
        Ok(())
    }

    /// 加锁（Mutex 中毒时取回内部状态：日志是辅助功能，不因他线程 panic 而失效）。
    fn lock_state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 轮转（须持锁）：`app.log` → `.1` → `.2` …；超出 `keep_files` 的最旧文件被替换。
    fn rotate_locked(&self, state: &mut State) -> io::Result<()> {
        state.file = None; // 先关闭句柄（Windows 下重命名要求文件未被占用）
        let keep = self.rotation.keep_files.max(1) as usize;
        if keep == 1 {
            state.file = Some(File::create(&self.path)?);
            state.written = 0;
            return Ok(());
        }
        let numbered = |index: usize| self.dir.join(format!("app.log.{index}"));
        let oldest = numbered(keep - 1);
        if oldest.exists() {
            fs::remove_file(&oldest)?;
        }
        for index in (1..keep - 1).rev() {
            let from = numbered(index);
            if from.exists() {
                fs::rename(&from, numbered(index + 1))?;
            }
        }
        if self.path.exists() {
            fs::rename(&self.path, numbered(1))?;
        }
        state.file = Some(File::create(&self.path)?);
        state.written = 0;
        Ok(())
    }
}

/// 当前 UTC 时间的 RFC 3339 字符串（理论不可失败；兜底纪元值以免日志路径 panic）。
fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 读取日志文件并按行解析 JSON。
    fn read_lines(path: &Path) -> Vec<serde_json::Value> {
        fs::read_to_string(path)
            .expect("读取日志失败")
            .lines()
            .map(|line| serde_json::from_str(line).expect("日志行应为合法 JSON"))
            .collect()
    }

    /// 单行包含必需字段；无上下文时不输出 tab/req；时间为 RFC 3339 UTC。
    #[test]
    fn writes_json_line_with_required_fields() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let logger = FileLogger::open(dir.path(), RotationConfig::default()).expect("打开失败");
        logger
            .write_line("info", "sread::test", "你好 hello", LogContext::default())
            .expect("写入失败");
        let lines = read_lines(&dir.path().join("app.log"));
        assert_eq!(lines.len(), 1);
        let value = &lines[0];
        assert_eq!(value["level"], "info");
        assert_eq!(value["module"], "sread::test");
        assert_eq!(value["message"], "你好 hello");
        assert!(value["time"].as_str().unwrap().ends_with('Z'));
        assert!(value.get("tab").is_none());
        assert!(value.get("req").is_none());
    }

    /// 上下文存在时输出 tab/req。
    #[test]
    fn context_ids_are_emitted_when_present() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let logger = FileLogger::open(dir.path(), RotationConfig::default()).expect("打开失败");
        let ctx = LogContext {
            tab: Some(7),
            req: Some(9),
        };
        logger.write_line("warn", "m", "x", ctx).expect("写入失败");
        let lines = read_lines(&dir.path().join("app.log"));
        assert_eq!(lines[0]["tab"], 7);
        assert_eq!(lines[0]["req"], 9);
    }

    /// 超过上限时轮转，文件总数受 keep_files 限制，最新内容在当前文件。
    #[test]
    fn rotates_when_size_exceeds_limit() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let logger = FileLogger::open(
            dir.path(),
            RotationConfig {
                max_bytes: 300,
                keep_files: 3,
            },
        )
        .expect("打开失败");
        for index in 0..12 {
            logger
                .write_line(
                    "info",
                    "m",
                    &format!("message-{index}-padding-padding"),
                    LogContext::default(),
                )
                .expect("写入失败");
        }
        assert!(dir.path().join("app.log").exists());
        assert!(dir.path().join("app.log.1").exists());
        assert!(dir.path().join("app.log.2").exists());
        assert_eq!(fs::read_dir(dir.path()).expect("列目录失败").count(), 3);
        let newest = read_lines(&dir.path().join("app.log"));
        assert!(newest.last().unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("message-11"));
    }

    /// keep_files = 1：直接截断，不留任何备份文件。
    #[test]
    fn keep_files_one_truncates_without_backups() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        let logger = FileLogger::open(
            dir.path(),
            RotationConfig {
                max_bytes: 120,
                keep_files: 1,
            },
        )
        .expect("打开失败");
        for index in 0..6 {
            logger
                .write_line(
                    "info",
                    "m",
                    &format!("m{index}--padding padding padding"),
                    LogContext::default(),
                )
                .expect("写入失败");
        }
        assert!(!dir.path().join("app.log.1").exists());
        assert_eq!(fs::read_dir(dir.path()).expect("列目录失败").count(), 1);
    }

    /// 对已有文件追加而非覆盖（重启后大小统计延续）。
    #[test]
    fn appends_to_existing_file() {
        let dir = tempfile::tempdir().expect("创建临时目录失败");
        fs::write(dir.path().join("app.log"), "{\"pre\":true}\n").expect("预置失败");
        let logger = FileLogger::open(dir.path(), RotationConfig::default()).expect("打开失败");
        logger
            .write_line("info", "m", "second", LogContext::default())
            .expect("写入失败");
        let text = fs::read_to_string(dir.path().join("app.log")).expect("读取失败");
        assert_eq!(text.lines().count(), 2);
    }
}
