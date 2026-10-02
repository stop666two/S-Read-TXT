//! 提权助手模式集成测试（真实可执行文件，Windows）。
//!
//! 覆盖两条关键路径：
//! - 合法参数：创建数据目录 + 对指定用户授权 + 退出码 0 + 当前（普通权限）进程可写；
//! - 参数不完整：退出码 2（拒绝执行，防止误入 Tauri/WebView2 启动流程）。
//!
//! 说明：真实 UAC 弹窗无法自动化；本测试直接以普通权限运行助手模式
//! （icacls 对自有目录授权无需管理员），验证助手本身的正确性。
//! 完整 UAC 链路由用户人工验收（见 docs/known-issues.md）。

#![cfg(windows)]

use std::process::Command;

/// 合法助手参数应完成「建目录 + 授权」并以 0 退出。
#[test]
fn prepare_mode_creates_directory_and_grants_access() {
    let exe = env!("CARGO_BIN_EXE_s-read-txt");
    let Some(sid) = s_read_txt::elevation::current_user_sid_string() else {
        eprintln!("跳过：无法获取当前用户 SID");
        return;
    };
    let tmp = tempfile::tempdir().expect("创建临时目录失败");
    let data_dir = tmp.path().join("nested").join("data");

    let status = Command::new(exe)
        .arg("--prepare-data-dir")
        .arg(&data_dir)
        .arg("--grant-sid")
        .arg(&sid)
        .status()
        .expect("启动助手模式失败");

    assert_eq!(status.code(), Some(0), "助手模式应以退出码 0 结束");
    assert!(data_dir.is_dir(), "助手模式应创建数据目录");
    assert!(
        s_read_txt::storage::data_dir::probe_writable(&data_dir).is_ok(),
        "授权后当前用户应可写数据目录"
    );
}

/// 参数不完整应以退出码 2 拒绝（防止误入 Tauri/WebView2 启动）。
#[test]
fn prepare_mode_rejects_incomplete_args() {
    let exe = env!("CARGO_BIN_EXE_s-read-txt");
    let status = Command::new(exe)
        .arg("--prepare-data-dir")
        .status()
        .expect("启动 exe 失败");
    assert_eq!(status.code(), Some(2), "参数缺失应退出码 2");
}
