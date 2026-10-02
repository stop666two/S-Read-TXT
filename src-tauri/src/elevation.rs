//! 管理员权限（UAC）按需使用——仅用于「一次性初始化数据目录权限」。
//!
//! # 背景与设计（2026-10-02 修订，根因见 docs/known-issues.md）
//!
//! 程序坚持「数据全部在程序目录」（便携模式）。按机器安装到 `Program Files` 时，
//! 普通用户令牌无法写该目录。**早期方案「整个应用以管理员重启」被证实与 WebView2
//! 冲突**：提权主进程派生的 WebView2 子进程按普通权限运行，写不进 Program Files，
//! 报「Microsoft Edge 无法读取和写入其数据目录」，导致启动失败。
//!
//! 现方案：**提权只做一次权限初始化，应用始终以普通权限运行**：
//!
//! ```text
//! 普通启动 ──┬─ 数据目录可写 ──────────────────────────────→ 直接启动（零打扰）
//!            ├─ 目录不可写 + SRT_NO_ELEVATION ────────────→ 跳过（前端引导流程兜底）
//!            ├─ 目录不可写 + 非管理员 ──→ UAC 一次 ──→ 提权助手：
//!            │     s-read-txt.exe --prepare-data-dir <路径> --grant-sid <SID>
//!            │     （创建目录 + icacls 授予「当前用户」修改权限，随后立即退出；
//!            │       助手绝不进入 Tauri/WebView2）──→ 主进程继续普通权限启动
//!            └─ 已提权（手动以管理员运行 / 安装器直接拉起）──→ 就地幂等授权，
//!                  确保 WebView2 子进程可用数据目录（覆盖该场景）
//! ```
//!
//! 授权对象固定为**发起进程的当前用户**（SID 由主进程计算并传给助手；即使 UAC 输入
//! 了其他管理员凭据，也授权原始用户）——最小权限。用户取消 UAC → [`AccessOutcome::Declined`]，
//! 前端「数据目录引导」提供「选择可写目录 / 只读运行」兜底。
//!
//! 逃生阀：`SRT_NO_ELEVATION`（存在即禁用提权；自动化测试与用户偏好）。

use std::path::Path;

/// 显式禁用提权的环境变量（值存在即禁用；自动化测试与用户偏好）。
pub const ENV_DISABLE: &str = "SRT_NO_ELEVATION";
/// 助手模式参数 1：数据目录绝对路径（后接一个参数）。
const ARG_PREPARE: &str = "--prepare-data-dir";
/// 助手模式参数 2：被授权用户的 SID（后接一个参数；如 `S-1-5-21-...`）。
const ARG_GRANT_SID: &str = "--grant-sid";
/// 等待提权助手完成的最长时间（毫秒）：icacls 递归大目录可能稍慢。
#[cfg(windows)]
const PREPARE_WAIT_MS: u32 = 60_000;

/// 权限初始化计划（纯逻辑，便于单元测试）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessPlan {
    /// 无需任何动作（目录可写且未提权）。
    Nothing,
    /// 已提权：就地幂等授权（覆盖「安装器以管理员直接拉起应用」场景）。
    FixInPlace,
    /// 不可写且非提权：拉起提权助手（会弹 UAC）。
    Spawn,
    /// 显式禁用提权：跳过（由前端引导流程兜底）。
    Skip,
}

/// 计划函数（纯逻辑）。
///
/// 规则（优先级）：
/// 1. `disabled`（逃生阀）且不可写 → `Skip`；禁用时**绝不**改 ACL（测试依赖此保证）；
/// 2. 已提权 → `FixInPlace`（即使可写也幂等授权：WebView2 子进程为普通权限，
///    需要目录 ACL 对当前用户可写，否则旧故障复现）；
/// 3. 可写 → `Nothing`；
/// 4. 其余（不可写 + 非提权）→ `Spawn`。
fn plan(writable: bool, elevated: bool, disabled: bool) -> AccessPlan {
    if disabled {
        return if writable {
            AccessPlan::Nothing
        } else {
            AccessPlan::Skip
        };
    }
    if elevated {
        return AccessPlan::FixInPlace;
    }
    if writable {
        return AccessPlan::Nothing;
    }
    AccessPlan::Spawn
}

/// 权限初始化结果（供启动层记录日志与决定后续流程）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessOutcome {
    /// 目录本就可写：无需动作。
    Writable,
    /// 授权已应用（就地或经提权助手）：目录现在可写。
    Granted,
    /// 用户取消了 UAC：回落前端引导流程。
    Declined,
    /// 被 `SRT_NO_ELEVATION` 跳过或平台不支持提权：回落前端引导流程。
    Skipped,
    /// 授权尝试失败（SID 获取失败 / 助手失败 / 授权后仍不可写）。
    Failed,
}

/// 提权助手参数解析结果。
#[derive(Debug, PartialEq, Eq)]
enum PrepareArgs {
    /// 非助手模式（未出现 `--prepare-data-dir`）。
    NotHelper,
    /// 助手模式但参数不完整（应拒绝执行并退出码 2）。
    Invalid,
    /// 合法助手参数。
    Valid {
        /// 数据目录（绝对路径）。
        dir: std::path::PathBuf,
        /// 被授权用户的 SID 字符串。
        sid: String,
    },
}

/// 解析助手模式参数（纯逻辑，便于测试）。
///
/// 规则：出现 `ARG_PREPARE` 才算助手模式；两个参数都必须提供非空值，否则 `Invalid`。
fn parse_prepare_args(args: &[String]) -> PrepareArgs {
    let mut saw_flag = false;
    let mut dir: Option<String> = None;
    let mut sid: Option<String> = None;
    let mut index = 1; // 跳过 argv[0]（程序路径）
    while index < args.len() {
        match args[index].as_str() {
            ARG_PREPARE => {
                saw_flag = true;
                if index + 1 < args.len() && !args[index + 1].starts_with("--") {
                    dir = Some(args[index + 1].clone());
                    index += 2;
                } else {
                    index += 1;
                }
            }
            ARG_GRANT_SID => {
                if index + 1 < args.len() && !args[index + 1].starts_with("--") {
                    sid = Some(args[index + 1].clone());
                    index += 2;
                } else {
                    index += 1;
                }
            }
            _ => index += 1,
        }
    }
    if !saw_flag {
        return PrepareArgs::NotHelper;
    }
    match (dir, sid) {
        (Some(dir), Some(sid)) if !dir.is_empty() && !sid.is_empty() => PrepareArgs::Valid {
            dir: std::path::PathBuf::from(dir),
            sid,
        },
        _ => PrepareArgs::Invalid,
    }
}

/// 若当前进程以助手模式启动则执行权限初始化，并返回进程退出码。
///
/// 返回 `None` 表示普通启动（调用方继续正常流程）；`Some(code)` 表示助手已完成
/// （调用方应立即 `std::process::exit(code)`）。助手模式不初始化日志/不创建窗口。
#[cfg(windows)]
pub fn maybe_run_prepare_mode() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    match parse_prepare_args(&args) {
        PrepareArgs::NotHelper => None,
        PrepareArgs::Invalid => {
            eprintln!(
                "[s-read-txt] 提权助手参数不完整；用法：{ARG_PREPARE} <数据目录> {ARG_GRANT_SID} <SID>"
            );
            Some(2)
        }
        PrepareArgs::Valid { dir, sid } => Some(match run_prepare(&dir, &sid) {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("[s-read-txt] 提权助手失败：{err}");
                1
            }
        }),
    }
}

/// 非 Windows 平台桩实现（本项目仅发布 Windows，保留以便跨平台编译检查）。
#[cfg(not(windows))]
pub fn maybe_run_prepare_mode() -> Option<i32> {
    None
}

/// 助手核心：创建数据目录并授予指定用户修改权限。
#[cfg(windows)]
fn run_prepare(dir: &Path, sid: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    grant_user_modify(dir, sid)
}

/// 主入口：确保数据目录对当前用户可写（必要时使用一次性提权）。
///
/// 返回值仅用于日志与诊断；无论结果如何调用方都继续启动（不可写时前端引导兜底）。
#[cfg(windows)]
pub fn ensure_data_dir_access(data_dir: &Path) -> AccessOutcome {
    let disabled = std::env::var_os(ENV_DISABLE).is_some();
    let elevated = is_process_elevated();
    let writable = crate::storage::data_dir::probe_writable(data_dir).is_ok();
    match plan(writable, elevated, disabled) {
        AccessPlan::Nothing => AccessOutcome::Writable,
        AccessPlan::Skip => AccessOutcome::Skipped,
        AccessPlan::FixInPlace => {
            match current_user_sid_string() {
                Some(sid) => {
                    if let Err(err) = grant_user_modify(data_dir, &sid) {
                        log::warn!(target: "sread::elevation", "就地授权失败：{err}");
                    }
                }
                None => {
                    log::warn!(target: "sread::elevation", "无法获取当前用户 SID，跳过就地授权")
                }
            }
            finalize_access(data_dir)
        }
        AccessPlan::Spawn => {
            let Some(sid) = current_user_sid_string() else {
                log::warn!(target: "sread::elevation", "无法获取当前用户 SID，无法执行提权授权");
                return AccessOutcome::Failed;
            };
            match spawn_elevated_prepare(data_dir, &sid) {
                PrepareSpawn::Started(handle) => {
                    wait_for_exit(handle);
                    finalize_access(data_dir)
                }
                PrepareSpawn::Cancelled => AccessOutcome::Declined,
                PrepareSpawn::Failed => AccessOutcome::Failed,
            }
        }
    }
}

/// 非 Windows 平台桩实现（见上）。
#[cfg(not(windows))]
pub fn ensure_data_dir_access(_data_dir: &Path) -> AccessOutcome {
    AccessOutcome::Skipped
}

/// 授权后的最终判定：以真实写探针为准（助手可能部分成功或被杀软延迟放行）。
#[cfg(windows)]
fn finalize_access(data_dir: &Path) -> AccessOutcome {
    if crate::storage::data_dir::probe_writable(data_dir).is_ok() {
        AccessOutcome::Granted
    } else {
        AccessOutcome::Failed
    }
}

/// 当前进程是否以管理员（已提权）身份运行。
///
/// 实现：打开自身进程令牌，查询 `TokenElevation`；任何一步失败都按「非提权」处理
/// （宁可多弹一次 UAC，不可漏判导致功能不可用）。
#[cfg(windows)]
fn is_process_elevated() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        let _ = CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// 取当前进程用户（令牌用户）的 SID 字符串（如 `S-1-5-21-...`）。
///
/// 用途：作为 `icacls /grant *<SID>` 的授权对象。**必须由普通权限的主进程计算**：
/// 即使 UAC 弹窗中输入了另一个管理员账号的凭据，被授权的仍是实际使用应用的用户。
/// 任一步失败返回 `None`（调用方跳过提权，由前端引导兜底）。
#[cfg(windows)]
pub fn current_user_sid_string() -> Option<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        // 第一次调用取所需缓冲区大小（预期返回 ERROR_INSUFFICIENT_BUFFER，忽略返回码）。
        let mut needed = 0u32;
        let _ = GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
        if needed == 0 {
            let _ = CloseHandle(token);
            return None;
        }
        // 用 u64 数组保证指针对齐（TOKEN_USER 含指针字段）。
        let mut buffer: Vec<u64> = vec![0; (needed as usize).div_ceil(8)];
        let ok = GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        );
        let _ = CloseHandle(token);
        if ok == 0 {
            return None;
        }
        let user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let mut wide_ptr: *mut u16 = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut wide_ptr) == 0 || wide_ptr.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *wide_ptr.add(len) != 0 {
            len += 1;
        }
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(wide_ptr, len));
        let _ = LocalFree(wide_ptr.cast());
        Some(sid)
    }
}

/// 授予指定用户对目录（含现有子项与未来新项）的修改权限。
///
/// 实现：调用系统自带 `icacls`：
/// - `*<SID>`：以 SID 而非本地化账号名指定主体（跨语言/域环境稳定）；
/// - `(OI)(CI)M`：对象继承 + 容器继承的「修改」权限；
/// - `/T`：应用到现有子项（如旧日志文件）；`/C`：遇到个别失败继续；
/// - `CREATE_NO_WINDOW`：不弹控制台闪窗。
#[cfg(windows)]
fn grant_user_modify(dir: &Path, sid: &str) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    /// Windows 进程创建标志：不创建控制台窗口。
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let status = Command::new("icacls")
        .arg(dir)
        .arg("/grant")
        .arg(format!("*{sid}:(OI)(CI)M"))
        .arg("/T")
        .arg("/C")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("icacls 退出码 {status}")))
    }
}

/// 提权助手的拉起结果。
#[cfg(windows)]
enum PrepareSpawn {
    /// 已拉起（含进程句柄，调用方等待其退出）。
    Started(windows_sys::Win32::Foundation::HANDLE),
    /// 用户在 UAC 弹窗中选择取消。
    Cancelled,
    /// 其他失败（ShellExecuteExW 调用失败等）。
    Failed,
}

/// 以管理员身份（`runas`）拉起自身助手模式 `--prepare-data-dir <路径> --grant-sid <SID>`。
///
/// 说明：
/// - 参数经命令行传入（环境变量同样会继承，但显式参数对调试更直观且不受环境清理影响）；
/// - `SW_HIDE`：助手无界面（其内部 icacls 亦被 `CREATE_NO_WINDOW` 静默）；
/// - 请求 `SEE_MASK_NOCLOSEPROCESS` 以获得进程句柄，交由调用方等待退出；
/// - 用户取消 → `ERROR_CANCELLED`(1223) → [`PrepareSpawn::Cancelled`]。
#[cfg(windows)]
fn spawn_elevated_prepare(data_dir: &Path, sid: &str) -> PrepareSpawn {
    use windows_sys::Win32::Foundation::ERROR_CANCELLED;
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let Ok(exe) = std::env::current_exe() else {
        return PrepareSpawn::Failed;
    };
    let params = format!(
        "{ARG_PREPARE} \"{}\" {ARG_GRANT_SID} {sid}",
        data_dir.display()
    );
    let verb = to_wide_os(std::ffi::OsStr::new("runas"));
    let file = to_wide_os(exe.as_os_str());
    let params = to_wide(&params);

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32;
        return if code == ERROR_CANCELLED {
            PrepareSpawn::Cancelled
        } else {
            PrepareSpawn::Failed
        };
    }
    if info.hProcess.is_null() {
        return PrepareSpawn::Failed;
    }
    PrepareSpawn::Started(info.hProcess)
}

/// 等待助手进程退出并释放句柄（超时后放弃等待，改由写探针判定结果）。
#[cfg(windows)]
fn wait_for_exit(handle: windows_sys::Win32::Foundation::HANDLE) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    unsafe {
        let _ = WaitForSingleObject(handle, PREPARE_WAIT_MS);
        let _ = CloseHandle(handle);
    }
}

/// 启动失败等致命错误弹原生消息框（窗口子系统下无控制台，必须可见）。
///
/// 场景：Tauri/WebView2 初始化失败时（例如数据目录异常、系统组件缺失），
/// 避免「双击无反应」的无声退出；用户可据此反馈日志路径。
#[cfg(windows)]
pub fn show_fatal_error(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    let text = to_wide(message);
    let title = to_wide("S-Read-TXT");
    let _ = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        )
    };
}

/// 非 Windows 平台桩实现（见上）。
#[cfg(not(windows))]
pub fn show_fatal_error(message: &str) {
    eprintln!("{message}");
}

/// 字符串 → 宽字符（含 NUL 结尾），供 Win32 宽字符 API。
#[cfg(windows)]
fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// OsStr → 宽字符（含 NUL 结尾），供路径参数使用。
#[cfg(windows)]
fn to_wide_os(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 计划表：全组合覆盖（禁用优先、提权幂等授权、可写直通、不可写提权）。
    #[test]
    fn plan_table() {
        assert_eq!(plan(true, false, false), AccessPlan::Nothing);
        assert_eq!(plan(true, true, false), AccessPlan::FixInPlace);
        assert_eq!(plan(false, true, false), AccessPlan::FixInPlace);
        assert_eq!(plan(false, false, false), AccessPlan::Spawn);
        assert_eq!(plan(false, false, true), AccessPlan::Skip);
        assert_eq!(plan(true, false, true), AccessPlan::Nothing);
    }

    /// 禁用标记在可写时也直通（自动化测试依赖「禁用 = 绝不改 ACL」）。
    #[test]
    fn disabled_never_fixes_acl() {
        assert_eq!(plan(true, true, true), AccessPlan::Nothing);
    }

    /// 参数解析：普通启动不进入助手模式。
    #[test]
    fn parse_not_helper() {
        let args = vec!["s-read-txt.exe".to_string()];
        assert_eq!(parse_prepare_args(&args), PrepareArgs::NotHelper);
    }

    /// 参数解析：合法助手参数（路径可含空格）。
    #[test]
    fn parse_valid_helper_args() {
        let args: Vec<String> = [
            "s-read-txt.exe",
            "--prepare-data-dir",
            "D:\\Program Files\\S-Read-TXT\\data",
            "--grant-sid",
            "S-1-5-21-111-222-333-1001",
        ]
        .iter()
        .map(|value| value.to_string())
        .collect();
        match parse_prepare_args(&args) {
            PrepareArgs::Valid { dir, sid } => {
                assert_eq!(
                    dir,
                    std::path::PathBuf::from("D:\\Program Files\\S-Read-TXT\\data")
                );
                assert_eq!(sid, "S-1-5-21-111-222-333-1001");
            }
            other => panic!("应解析为 Valid，实际 {other:?}"),
        }
    }

    /// 参数解析：缺值 → Invalid（调用方以退出码 2 拒绝，防止误入 Tauri 启动）。
    #[test]
    fn parse_invalid_when_values_missing() {
        let only_flag = vec![
            "s-read-txt.exe".to_string(),
            "--prepare-data-dir".to_string(),
        ];
        assert_eq!(parse_prepare_args(&only_flag), PrepareArgs::Invalid);
        let missing_sid = vec![
            "s-read-txt.exe".to_string(),
            "--prepare-data-dir".to_string(),
            "D:\\x\\data".to_string(),
            "--grant-sid".to_string(),
        ];
        assert_eq!(parse_prepare_args(&missing_sid), PrepareArgs::Invalid);
    }

    /// 参数顺序无关：`--grant-sid` 在前同样识别。
    #[test]
    fn parse_argument_order_independent() {
        let args: Vec<String> = [
            "s-read-txt.exe",
            "--grant-sid",
            "S-1-5-21-1",
            "--prepare-data-dir",
            "D:\\x\\data",
        ]
        .iter()
        .map(|value| value.to_string())
        .collect();
        assert!(matches!(
            parse_prepare_args(&args),
            PrepareArgs::Valid { .. }
        ));
    }

    /// ACL 真实回归（Windows、非提权环境）：只读 ACL → 探测失败 → 授权 → 可写。
    ///
    /// 说明：提权环境（如 CI 管理员）跳过——管理员令牌会掩盖「只读」效果，
    /// 该用例的价值在普通用户环境（真实场景）。
    #[cfg(windows)]
    #[test]
    fn acl_grant_restores_writability() {
        use std::process::Command;

        if is_process_elevated() {
            eprintln!("跳过：当前进程已提权，无法可靠模拟不可写目录");
            return;
        }
        let Some(sid) = current_user_sid_string() else {
            eprintln!("跳过：无法获取当前用户 SID");
            return;
        };
        let tmp = tempfile::tempdir().expect("创建临时目录失败");
        let dir = tmp.path().join("data");
        std::fs::create_dir(&dir).expect("创建测试目录失败");

        // 移除继承 ACL，仅保留当前用户只读 → 真实不可写。
        let readonly = Command::new("icacls")
            .arg(&dir)
            .arg("/inheritance:r")
            .arg("/grant:r")
            .arg(format!("*{sid}:(RX)"))
            .status()
            .expect("调用 icacls 失败");
        assert!(readonly.success(), "设置只读 ACL 失败");
        assert!(
            crate::storage::data_dir::probe_writable(&dir).is_err(),
            "只读 ACL 下写探测应失败"
        );

        // 授权后应恢复可写（与提权助手同一代码路径）。
        grant_user_modify(&dir, &sid).expect("授权失败");
        assert!(
            crate::storage::data_dir::probe_writable(&dir).is_ok(),
            "授权后写探测应成功"
        );
        // 临时目录随 TempDir 释放自动清理（授权已恢复删除权限）。
    }
}
