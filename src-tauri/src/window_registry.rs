//! 运行时窗口注册表：记录最后聚焦的主窗口 label。
//!
//! 用途：会话保存时写入 `focusedLabel`（启动时激活最后使用的窗口）；
//! 由 `main.rs` 的窗口事件维护，命令层只读。

use std::sync::Mutex;

/// 最后聚焦的主窗口 label（默认 `main`；`main-*` 的 Focused 事件更新）。
pub struct LastFocused(Mutex<String>);

impl Default for LastFocused {
    fn default() -> Self {
        Self(Mutex::new("main".to_string()))
    }
}

impl LastFocused {
    /// 记录聚焦（仅主窗口调用；锁中毒时静默保留旧值）。
    pub fn set(&self, label: &str) {
        if let Ok(mut guard) = self.0.lock() {
            *guard = label.to_string();
        }
    }

    /// 读取最后聚焦窗口（读取失败回退 `main`）。
    pub fn get(&self) -> String {
        self.0
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| "main".to_string())
    }
}
