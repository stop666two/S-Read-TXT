//! 批量编辑任务的取消登记：命令层与执行线程之间的取消通道。
//!
//! 为什么独立于 `AppState`：批量执行会在整个过程中持有状态锁，取消命令
//! 必须能在不拿状态锁的前提下把取消旗标传入执行线程。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// 按标签 id 登记运行中的批量任务（同一标签同时至多一个任务）。
#[derive(Default)]
pub struct BatchJobs {
    flags: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

impl BatchJobs {
    /// 登记一个批量任务并返回取消旗标（同标签重复登记会替换旧旗标）。
    pub fn register(&self, tab_id: u64) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        if let Ok(mut flags) = self.flags.lock() {
            flags.insert(tab_id, flag.clone());
        }
        flag
    }

    /// 请求取消指定标签的批量任务；无运行中任务时返回 false。
    pub fn cancel(&self, tab_id: u64) -> bool {
        if let Ok(flags) = self.flags.lock() {
            if let Some(flag) = flags.get(&tab_id) {
                flag.store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    /// 任务结束（成功/取消/失败）后清理登记。
    pub fn finish(&self, tab_id: u64) {
        if let Ok(mut flags) = self.flags.lock() {
            flags.remove(&tab_id);
        }
    }
}
