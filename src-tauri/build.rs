// Tauri 构建脚本（src-tauri/build.rs）
// 职责：读取 tauri.conf.json、生成权限 schema 与编译期资源；配置错误在此阶段暴露（fail fast）
fn main() {
    tauri_build::build()
}
