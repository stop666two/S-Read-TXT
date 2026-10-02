// S-Read-TXT 主进程入口（src-tauri/src/main.rs）
// 阶段 0：最小可运行骨架（主窗口 + dialog 插件 + get_app_info 冒烟命令）。
// 阶段 1 起将按设计文档 §6 的命令清单扩充分模块注册。

// 发布构建隐藏 Windows 控制台窗口；调试构建保留控制台以便查看日志
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;

/// 应用信息（IPC 返回体；字段序列化为 camelCase 供前端直接消费）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    /// 应用版本号（取自 Cargo.toml，与 tauri.conf.json 保持同步）
    version: String,
    /// 数据目录绝对路径（阶段 1 将替换为便携目录解析模块：程序目录/data）
    data_dir: String,
}

/// 冒烟命令：返回应用版本与数据目录，用于验证 IPC 链路。
///
/// 返回：Ok(AppInfo) 或 Err(中文错误文案)
/// 说明：阶段 1 将由 storage 模块接管目录解析与可写性检测
#[tauri::command]
fn get_app_info() -> Result<AppInfo, String> {
    let exe = std::env::current_exe().map_err(|e| format!("无法定位程序路径：{e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "无法解析程序所在目录".to_string())?
        .join("data");
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir: dir.to_string_lossy().into_owned(),
    })
}

fn main() {
    tauri::Builder::default()
        // 原生对话框能力（文件选择/目录选择/消息框）
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![get_app_info])
        .run(tauri::generate_context!())
        .expect("Tauri 应用启动失败");
}
