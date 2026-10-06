//! 更新检查：可配置更新源解析、版本比较、发布信息拉取与安装包下载校验。
//!
//! 更新源支持三类写法：
//! - GitHub 仓库地址（`https://github.com/owner/repo`，可带 `/releases` 后缀或 `.git`）——
//!   自动换算为 `https://api.github.com/repos/owner/repo/releases/latest`；
//! - GitHub API 地址（`https://api.github.com/...`）——原样使用；
//! - 任意 http(s) 地址（如自建 JSON 服务或本地桩服务器）——原样使用，响应需为
//!   GitHub Release 同构 JSON（`tag_name` / `html_url` / `assets[]`）。

use std::cmp::Ordering;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ipc_error::{
    IpcError, CODE_FILE_NOT_FOUND, CODE_IO, CODE_UPDATE_DIGEST, CODE_UPDATE_FAILED,
    CODE_UPDATE_SOURCE,
};

/// 请求超时（连接与整体读写均受此约束）。
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
/// 发布 JSON 响应体上限（防异常大响应占用内存）。
const MAX_JSON_BYTES: u64 = 2 * 1024 * 1024;
/// 安装包下载上限（内部保护值，防止无限下载写满磁盘）。
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;

/// 更新流程错误（携带 IPC 错误码，由命令层转换为 `IpcError`）。
#[derive(Debug)]
pub struct UpdateError {
    code: &'static str,
    message: String,
}

impl UpdateError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// 转换为 IPC 错误。
    pub fn to_ipc(self) -> IpcError {
        IpcError::new(self.code, self.message)
    }
}

/// 更新信息（`check_update` 返回体）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// 当前版本（原样回显）。
    pub current: String,
    /// 发布方最新版本（已去除 `v` 前缀）。
    pub latest: String,
    /// 最新版本是否高于当前版本。
    pub newer: bool,
    /// 发布页面地址（`html_url`，可能为空）。
    pub release_url: String,
    /// 匹配到的安装包（可能为空，例如仅提供源码包时）。
    pub asset: Option<UpdateAsset>,
}

/// 发布资产（安装包）信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAsset {
    /// 文件名。
    pub name: String,
    /// 下载地址。
    pub url: String,
    /// `sha256:` 摘要（解析失败或未提供时为 None）。
    pub sha256: Option<String>,
    /// 字节数（未提供时为 None）。
    pub size: Option<u64>,
}

/// 下载结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedUpdate {
    /// 落盘路径。
    pub path: String,
    /// 实际字节数。
    pub bytes: u64,
    /// 实际 sha256（小写十六进制）。
    pub sha256: String,
}

/// GitHub Release 同构响应体。
#[derive(Debug, Deserialize)]
struct ReleaseJson {
    tag_name: String,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<AssetJson>,
}

#[derive(Debug, Deserialize)]
struct AssetJson {
    name: String,
    #[serde(default)]
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
    #[serde(default)]
    size: Option<u64>,
}

/// 把用户配置的更新源换算为可请求的发布信息地址。
pub fn release_api_url(source: &str) -> Result<String, UpdateError> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return Err(UpdateError::new(CODE_UPDATE_SOURCE, "未配置更新源地址"));
    }
    if !trimmed.starts_with("https://") && !trimmed.starts_with("http://") {
        return Err(UpdateError::new(
            CODE_UPDATE_SOURCE,
            "更新源地址必须以 http(s):// 开头",
        ));
    }
    if let Some(path) = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
    {
        let path = path.trim_end_matches('/');
        let mut segments = path.split('/').filter(|s| !s.is_empty());
        let owner = segments.next().unwrap_or("");
        let repo = segments.next().unwrap_or("").trim_end_matches(".git");
        if owner.is_empty() || repo.is_empty() {
            return Err(UpdateError::new(
                CODE_UPDATE_SOURCE,
                "GitHub 更新源需形如 https://github.com/owner/repo",
            ));
        }
        return Ok(format!(
            "https://api.github.com/repos/{owner}/{repo}/releases/latest"
        ));
    }
    Ok(trimmed.to_string())
}

/// 解析版本号为「数字段 + 可选预发布标识」。
fn parse_version(version: &str) -> (Vec<u64>, Option<Vec<String>>) {
    let trimmed = version.trim().trim_start_matches('v');
    match trimmed.split_once('-') {
        Some((core, pre)) => (
            parse_numeric_segments(core),
            Some(pre.split('.').map(str::to_string).collect()),
        ),
        None => (parse_numeric_segments(trimmed), None),
    }
}

fn parse_numeric_segments(core: &str) -> Vec<u64> {
    core.split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

/// 语义化版本比较（数字段逐位比较；正式版高于预发布版；预发布标识按 SemVer 1 号规则比较）。
pub fn compare_versions(left: &str, right: &str) -> Ordering {
    let (left_numbers, left_pre) = parse_version(left);
    let (right_numbers, right_pre) = parse_version(right);
    let core_len = left_numbers.len().max(right_numbers.len());
    for index in 0..core_len {
        let a = left_numbers.get(index).copied().unwrap_or(0);
        let b = right_numbers.get(index).copied().unwrap_or(0);
        match a.cmp(&b) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    match (left_pre, right_pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => compare_prerelease(&a, &b),
    }
}

fn compare_prerelease(left: &[String], right: &[String]) -> Ordering {
    for (a, b) in left.iter().zip(right.iter()) {
        let a_num = a.parse::<u64>().ok();
        let b_num = b.parse::<u64>().ok();
        let ord = match (a_num, b_num) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => a.cmp(b),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    left.len().cmp(&right.len())
}

/// 最新版本是否高于当前版本。
pub fn is_newer(current: &str, latest: &str) -> bool {
    compare_versions(latest, current) == Ordering::Greater
}

/// 解析 `sha256:<hex>`（或裸 64 位十六进制）为规范小写摘要。
pub fn parse_sha256(digest: &str) -> Option<String> {
    let trimmed = digest.trim();
    let hex = trimmed
        .strip_prefix("sha256:")
        .or_else(|| trimmed.strip_prefix("SHA256:"))
        .or_else(|| trimmed.strip_prefix("Sha256:"))
        .unwrap_or(trimmed);
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(hex.to_ascii_lowercase())
}

/// 从发布 JSON 解析更新信息。
pub fn parse_release_json(body: &str, current: &str) -> Result<UpdateInfo, UpdateError> {
    let release: ReleaseJson = serde_json::from_str(body).map_err(|err| {
        UpdateError::new(CODE_UPDATE_FAILED, format!("更新源响应解析失败：{err}"))
    })?;
    let latest = release.tag_name.trim().trim_start_matches('v').to_string();
    if latest.is_empty() {
        return Err(UpdateError::new(
            CODE_UPDATE_FAILED,
            "更新源响应缺少 tag_name",
        ));
    }
    let asset = pick_asset(&release.assets).map(|item| UpdateAsset {
        name: item.name.clone(),
        url: item.browser_download_url.clone(),
        sha256: item.digest.as_deref().and_then(parse_sha256),
        size: item.size,
    });
    Ok(UpdateInfo {
        current: current.to_string(),
        newer: is_newer(current, &latest),
        latest,
        release_url: release.html_url.unwrap_or_default(),
        asset,
    })
}

/// 选择安装包：优先 NSIS 安装程序，其次任意 exe，最后取第一个资产。
fn pick_asset(assets: &[AssetJson]) -> Option<&AssetJson> {
    assets
        .iter()
        .find(|item| item.name.ends_with("-setup.exe"))
        .or_else(|| assets.iter().find(|item| item.name.ends_with(".exe")))
        .or_else(|| assets.first())
}

/// 构建带超时与 UA 的 HTTP 客户端。
fn build_agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(HTTP_TIMEOUT).build()
}

fn map_transport(error: ureq::Error) -> UpdateError {
    match error {
        ureq::Error::Status(code, _) => {
            UpdateError::new(CODE_UPDATE_FAILED, format!("更新源返回 HTTP {code}"))
        }
        ureq::Error::Transport(err) => {
            UpdateError::new(CODE_UPDATE_FAILED, format!("更新源请求失败：{err}"))
        }
    }
}

/// 拉取并解析更新信息（阻塞；由命令层调度到阻塞线程池）。
pub fn check_update(source: &str, current: &str) -> Result<UpdateInfo, UpdateError> {
    let api_url = release_api_url(source)?;
    let agent = build_agent();
    let response = agent
        .get(&api_url)
        .set("User-Agent", "S-Read-TXT-update-check")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(map_transport)?;
    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_JSON_BYTES)
        .read_to_string(&mut body)
        .map_err(|err| {
            UpdateError::new(CODE_UPDATE_FAILED, format!("读取更新源响应失败：{err}"))
        })?;
    parse_release_json(&body, current)
}

/// 清洗下载文件名（防路径穿越；仅保留常见安全字符）。
pub fn sanitize_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '(' | ')'))
        .collect();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return "update.exe".to_string();
    }
    cleaned
}

/// 从 URL 末段推断文件名（管道下载未显式提供文件名时使用）。
fn file_name_from_url(url: &str) -> String {
    let base = url.split(['?', '#']).next().unwrap_or(url);
    sanitize_file_name(base)
}

/// 下载安装包到指定目录；提供摘要时强校验（不匹配即删除临时文件并报错）。
pub fn download_asset(
    url: &str,
    expected_sha256: Option<&str>,
    dest_dir: &Path,
    file_name: Option<&str>,
) -> Result<DownloadedUpdate, UpdateError> {
    let trimmed = url.trim();
    if !trimmed.starts_with("https://") && !trimmed.starts_with("http://") {
        return Err(UpdateError::new(
            CODE_UPDATE_SOURCE,
            "下载地址必须以 http(s):// 开头",
        ));
    }
    let requested = file_name
        .map(str::to_string)
        .unwrap_or_else(|| file_name_from_url(trimmed));
    let safe_name = sanitize_file_name(&requested);
    fs::create_dir_all(dest_dir)
        .map_err(|err| UpdateError::new(CODE_IO, format!("创建下载目录失败：{err}")))?;
    let dest = dest_dir.join(&safe_name);
    let temp = dest_dir.join(format!("{safe_name}.part"));

    let agent = build_agent();
    let response = agent
        .get(trimmed)
        .set("User-Agent", "S-Read-TXT-update-check")
        .call()
        .map_err(map_transport)?;

    let mut reader = response.into_reader();
    let mut file = fs::File::create(&temp)
        .map_err(|err| UpdateError::new(CODE_IO, format!("创建下载文件失败：{err}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| UpdateError::new(CODE_UPDATE_FAILED, format!("下载中断：{err}")))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > MAX_DOWNLOAD_BYTES {
            let _ = fs::remove_file(&temp);
            return Err(UpdateError::new(
                CODE_UPDATE_FAILED,
                "安装包超出内部下载上限（512MB）",
            ));
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|err| UpdateError::new(CODE_IO, format!("写入下载文件失败：{err}")))?;
    }
    file.flush()
        .map_err(|err| UpdateError::new(CODE_IO, format!("刷新下载文件失败：{err}")))?;
    drop(file);

    let computed = format!("{:x}", hasher.finalize());
    if let Some(expected) = expected_sha256 {
        let expected = expected.trim().to_ascii_lowercase();
        if !expected.is_empty() && expected != computed {
            let _ = fs::remove_file(&temp);
            return Err(UpdateError::new(
                CODE_UPDATE_DIGEST,
                "安装包校验失败：文件摘要与发布信息不一致",
            ));
        }
    }

    if dest.exists() {
        let _ = fs::remove_file(&dest);
    }
    fs::rename(&temp, &dest)
        .map_err(|err| UpdateError::new(CODE_IO, format!("落盘安装包失败：{err}")))?;
    Ok(DownloadedUpdate {
        path: dest.to_string_lossy().to_string(),
        bytes: total,
        sha256: computed,
    })
}

/// 校验待打开的本地文件是否存在于数据目录内的更新文件夹（或至少为存在的普通文件）。
pub fn ensure_revealable(path: &str) -> Result<PathBuf, UpdateError> {
    let candidate = PathBuf::from(path);
    if !candidate.is_file() {
        return Err(UpdateError::new(
            CODE_FILE_NOT_FOUND,
            "文件不存在或不是普通文件",
        ));
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare_orders_core_numbers() {
        assert_eq!(compare_versions("0.0.1", "0.0.2"), Ordering::Less);
        assert_eq!(compare_versions("1.2.0", "1.2"), Ordering::Equal);
        assert_eq!(compare_versions("2.0.0", "10.0.0"), Ordering::Less);
        assert_eq!(compare_versions("v0.3.0", "0.2.9"), Ordering::Greater);
    }

    #[test]
    fn version_compare_handles_prerelease() {
        assert_eq!(compare_versions("0.1.0", "0.1.0-beta"), Ordering::Greater);
        assert_eq!(
            compare_versions("0.1.0-beta", "0.1.0-beta.2"),
            Ordering::Less
        );
        assert_eq!(
            compare_versions("0.1.0-beta.2", "0.1.0-beta.10"),
            Ordering::Less
        );
        assert_eq!(
            compare_versions("0.1.0-beta", "0.1.0-alpha"),
            Ordering::Greater
        );
    }

    #[test]
    fn release_api_url_normalizes_github_forms() {
        assert_eq!(
            release_api_url("https://github.com/owner/repo").unwrap(),
            "https://api.github.com/repos/owner/repo/releases/latest"
        );
        assert_eq!(
            release_api_url("https://github.com/owner/repo.git/").unwrap(),
            "https://api.github.com/repos/owner/repo/releases/latest"
        );
        assert_eq!(
            release_api_url("https://github.com/owner/repo/releases/tag/v1").unwrap(),
            "https://api.github.com/repos/owner/repo/releases/latest"
        );
        assert_eq!(
            release_api_url("https://api.github.com/repos/o/r/releases/latest").unwrap(),
            "https://api.github.com/repos/o/r/releases/latest"
        );
        assert_eq!(
            release_api_url("http://127.0.0.1:9/latest.json").unwrap(),
            "http://127.0.0.1:9/latest.json"
        );
    }

    #[test]
    fn release_api_url_rejects_invalid_sources() {
        assert_eq!(release_api_url("  ").unwrap_err().code, CODE_UPDATE_SOURCE);
        assert_eq!(
            release_api_url("ftp://example.com/x").unwrap_err().code,
            CODE_UPDATE_SOURCE
        );
        assert_eq!(
            release_api_url("https://github.com/only-owner")
                .unwrap_err()
                .code,
            CODE_UPDATE_SOURCE
        );
    }

    #[test]
    fn sha256_parsing_accepts_both_forms_and_rejects_bad() {
        let hex = "a".repeat(64);
        assert_eq!(parse_sha256(&format!("sha256:{hex}")).unwrap(), hex);
        assert_eq!(
            parse_sha256(&format!("SHA256:{}", hex.to_uppercase())).unwrap(),
            hex
        );
        assert_eq!(parse_sha256(&hex).unwrap(), hex);
        assert!(parse_sha256("sha256:xyz").is_none());
        assert!(parse_sha256("sha512:abc").is_none());
    }

    #[test]
    fn parse_release_prefers_setup_asset() {
        let body = format!(
            r#"{{"tag_name":"v0.0.2-beta","html_url":"https://example.com/r","assets":[{{"name":"source.zip","browser_download_url":"https://example.com/s.zip"}},{{"name":"S-Read-TXT_0.0.2-beta_x64-setup.exe","browser_download_url":"https://example.com/setup.exe","digest":"sha256:{}","size":123}}]}}"#,
            "b".repeat(64)
        );
        let info = parse_release_json(&body, "0.0.1-beta").unwrap();
        assert_eq!(info.latest, "0.0.2-beta");
        assert!(info.newer);
        let asset = info.asset.unwrap();
        assert_eq!(asset.name, "S-Read-TXT_0.0.2-beta_x64-setup.exe");
        assert_eq!(asset.sha256.unwrap(), "b".repeat(64));
        assert_eq!(asset.size, Some(123));
    }

    #[test]
    fn parse_release_reports_same_version() {
        let body = r#"{"tag_name":"0.0.1-beta","assets":[]}"#;
        let info = parse_release_json(body, "0.0.1-beta").unwrap();
        assert!(!info.newer);
        assert!(info.asset.is_none());
    }

    #[test]
    fn sanitize_file_name_blocks_traversal() {
        assert_eq!(sanitize_file_name("../../evil.exe"), "evil.exe");
        assert_eq!(sanitize_file_name("dir\\setup v1.exe"), "setupv1.exe");
        assert_eq!(sanitize_file_name(".."), "update.exe");
        assert_eq!(sanitize_file_name(""), "update.exe");
    }
}
