// Tauri 构建脚本（src-tauri/build.rs）
// 职责：读取 tauri.conf.json、生成权限 schema 与编译期资源；配置错误在此阶段暴露（fail fast）
fn main() {
    tauri_build::build();
    inject_test_manifest();
}

/// 为测试/基准目标补注 Windows 资源（Common-Controls v6 清单 + 版本信息）。
///
/// 为什么需要：`tauri_build::build()` 只为**二进制目标**注入 `libresource.a`
/// （`cargo:rustc-link-arg-bins`）；而单元测试 harness 同样会链接 GUI 依赖
/// （tauri → tao/muda 等，导入 `comctl32!TaskDialogIndirect`），缺少 v6 清单时
/// 加载期即报 `0xC0000139`（STATUS_ENTRYPOINT_NOT_FOUND，历史记录见
/// docs/plan/progress.md「跨阶段已知事项」）。这里复用同一资源文件为
/// tests/benches 目标补注，使 `cargo test` 在 GNU 工具链下可运行。
fn inject_test_manifest() {
    #[cfg(windows)]
    {
        let Ok(out_dir) = std::env::var("OUT_DIR") else {
            return;
        };
        let resource = std::path::Path::new(&out_dir).join("libresource.a");
        if resource.exists() {
            println!("cargo:rustc-link-arg={}", resource.display());
        }
    }
}
