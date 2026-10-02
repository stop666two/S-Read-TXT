//! 管理员权限（UAC）按需申请。
//!
//! 背景：程序坚持「数据全部在程序目录」（便携模式）。当被**按机器安装**到
//! `C:\Program Files\...` 时，标准用户令牌无法写该目录（历史/设置/日志/WebView 缓存
//! 都写不进去）——此时唯一正确的做法是提升权限运行；而便携运行或「仅为我」安装
//! 的数据目录天然可写，**不应**打扰用户（不弹 UAC）。
//!
//! 决策链（`maybe_relaunch_elevated`）：
//! 1. 数据目录可写 → [`RelaunchOutcome::NotNeeded`]（最常见路径，零打扰）；
//! 2. 显式禁用（`SRT_NO_ELEVATION`）或本轮已尝试过（`SRT_ELEVATION_ATTEMPTED`）→ 跳过；
//! 3. 已是管理员但仍不可写 → 提权无济于事，回落到「数据目录引导」对话框流程；
//! 4. 其余（不可写 + 非管理员）→ `ShellExecuteExW("runas")` 重启自身（UAC 提示），
//!    成功后当前进程立即退出；用户取消则走引导流程（仍可用「选择可写目录/只读运行」）。
//!
//! 防循环：发起提权前设置 `SRT_ELEVATION_ATTEMPTED=1`，子进程继承该变量后不再尝试。

use std::path::Path;

/// 已尝试提权的环境标记（由本模块写入；子进程继承，防止无限提权循环）。
pub const ENV_ATTEMPTED: &str = "SRT_ELEVATION_ATTEMPTED";
/// 显式禁用提权的环境变量（值存在即禁用；自动化测试与用户偏好）。
pub const ENV_DISABLE: &str = "SRT_NO_ELEVATION";

/// 提权决策（纯逻辑，便于单元测试）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationDecision {
    /// 数据目录可写：无需提权（便携运行、「仅为我」安装）。
    NotNeeded,
    /// 环境禁用或本轮已尝试：不再提权。
    Skipped,
    /// 已是管理员但目录仍不可写：提权无济于事，回落引导流程。
    AlreadyElevated,
    /// 尝试以管理员重启（会弹出 UAC）。
    Attempt,
}

/// 决策函数：`writable` = 数据目录可写探测结果。
///
/// 规则（按优先级）：
/// 1. 可写 → `NotNeeded`；
/// 2. 禁用/已尝试 → `Skipped`；
/// 3. 已提权 → `AlreadyElevated`；
/// 4. 否则 → `Attempt`。
pub fn decide(
    writable: bool,
    elevated: bool,
    attempted: bool,
    disabled: bool,
) -> ElevationDecision {
    if writable {
        return ElevationDecision::NotNeeded;
    }
    if disabled || attempted {
        return ElevationDecision::Skipped;
    }
    if elevated {
        return ElevationDecision::AlreadyElevated;
    }
    ElevationDecision::Attempt
}

/// 提权重启结果（供启动层决定日志与后续流程）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelaunchOutcome {
    /// 目录可写，未做任何提权动作。
    NotNeeded,
    /// 被环境变量跳过（已尝试/显式禁用）。
    Skipped,
    /// 已是管理员但目录仍不可写（回落引导流程）。
    AlreadyElevated,
    /// 提权实例已拉起：当前进程应立即退出。
    Spawned,
    /// 用户取消 UAC 或启动失败：继续非提权流程（由引导流程兜底）。
    Declined,
}

/// 主入口：探测数据目录 → 决策 → 需要时提权重启自身。
///
/// 参数：`data_dir` 为最终解析出的便携数据目录（程序目录/data 或覆盖目录）。
/// 返回：启动层按结果决定退出或继续（见 [`RelaunchOutcome`]）。
pub fn maybe_relaunch_elevated(data_dir: &Path) -> RelaunchOutcome {
    let decision = decide(
        crate::storage::data_dir::probe_writable(data_dir).is_ok(),
        is_process_elevated(),
        std::env::var_os(ENV_ATTEMPTED).is_some(),
        std::env::var_os(ENV_DISABLE).is_some(),
    );
    match decision {
        ElevationDecision::NotNeeded => RelaunchOutcome::NotNeeded,
        ElevationDecision::Skipped => RelaunchOutcome::Skipped,
        ElevationDecision::AlreadyElevated => RelaunchOutcome::AlreadyElevated,
        ElevationDecision::Attempt => {
            // 先写标记再拉起：子进程继承环境变量，避免提权后再次进入 Attempt。
            std::env::set_var(ENV_ATTEMPTED, "1");
            if relaunch_elevated() {
                RelaunchOutcome::Spawned
            } else {
                RelaunchOutcome::Declined
            }
        }
    }
}

/// 当前进程是否以管理员（已提权）身份运行。
///
/// 实现：打开自身进程令牌，查询 `TokenElevation`；任何一步失败都按「非提权」处理
/// （宁可多弹一次 UAC，不可漏判导致功能不可用）。
#[cfg(windows)]
fn is_process_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
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

/// 以管理员身份重新启动当前可执行文件（UAC）。
///
/// 实现：`ShellExecuteExW` + `"runas"` 谓词。返回 `true` 表示已成功拉起新实例；
/// 用户取消 UAC 或调用失败均返回 `false`（由调用方回落引导流程）。
/// 环境变量（数据目录覆盖、调试端口等）由子进程继承，无需显式传参。
#[cfg(windows)]
fn relaunch_elevated() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    /// UTF-16（含 NUL 结尾）编码，供 Win32 宽字符 API 使用。
    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return false,
    };
    let verb = wide(OsStr::new("runas"));
    let file = wide(exe.as_os_str());

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = std::ptr::null();
    info.lpDirectory = std::ptr::null();
    info.nShow = SW_SHOWNORMAL as i32;

    let started = unsafe { ShellExecuteExW(&mut info) } != 0;
    if started && !info.hProcess.is_null() {
        // 只需要进程句柄已创建（表示实例已拉起），不等待其结束：立即释放句柄。
        unsafe {
            let _ = CloseHandle(info.hProcess);
        }
    }
    started
}

/// 非 Windows 平台桩实现（本项目仅发布 Windows，保留以便跨平台编译检查）。
#[cfg(not(windows))]
fn is_process_elevated() -> bool {
    false
}

/// 非 Windows 平台桩实现（见上）。
#[cfg(not(windows))]
fn relaunch_elevated() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 目录可写（便携/仅为我安装）：无论其他条件，都不提权。
    #[test]
    fn writable_never_elevates() {
        assert_eq!(
            decide(true, false, false, false),
            ElevationDecision::NotNeeded
        );
        assert_eq!(
            decide(true, false, true, false),
            ElevationDecision::NotNeeded
        );
        assert_eq!(
            decide(true, true, false, true),
            ElevationDecision::NotNeeded
        );
    }

    /// 不可写 + 非管理员 + 未尝试 → 发起提权。
    #[test]
    fn unwritable_non_admin_attempts() {
        assert_eq!(
            decide(false, false, false, false),
            ElevationDecision::Attempt
        );
    }

    /// 已尝试过（环境标记）→ 不再提权（防循环）。
    #[test]
    fn attempt_marker_prevents_loop() {
        assert_eq!(
            decide(false, false, true, false),
            ElevationDecision::Skipped
        );
    }

    /// 显式禁用优先于一切提权路径。
    #[test]
    fn disable_flag_wins() {
        assert_eq!(
            decide(false, false, false, true),
            ElevationDecision::Skipped
        );
        assert_eq!(decide(false, false, true, true), ElevationDecision::Skipped);
    }

    /// 已是管理员但目录仍不可写 → 不做无谓提权（回落引导流程）。
    #[test]
    fn elevated_but_unwritable_falls_back() {
        assert_eq!(
            decide(false, true, false, false),
            ElevationDecision::AlreadyElevated
        );
    }
}
