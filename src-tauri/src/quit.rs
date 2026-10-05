//! 多窗口「退出所有窗口」协调器（两阶段：请求 → 全部就绪 → 放行）。
//!
//! 目的：菜单「退出」在多窗口下必须**全部窗口确认完毕才开始逐一关闭**，
//! 任一窗口取消则整体取消，避免「一半窗口已关、一半仍在询问」的半退出状态。
//!
//! 协议（事件）：
//! - `srt://quit-request`：广播，各主窗口收到后处理自身脏标签并回报就绪；
//! - `srt://quit-proceed`：全部就绪后广播，各窗口落盘会话并关闭自身；
//! - `srt://quit-cancelled`：任一窗口取消后广播，未关闭的窗口放弃待办。

use std::collections::BTreeSet;
use std::sync::Mutex;

/// 事件名：退出请求（广播）。
pub const EVENT_QUIT_REQUEST: &str = "srt://quit-request";
/// 事件名：全部就绪、允许关闭（广播）。
pub const EVENT_QUIT_PROCEED: &str = "srt://quit-proceed";
/// 事件名：退出被取消（广播）。
pub const EVENT_QUIT_CANCELLED: &str = "srt://quit-cancelled";

/// 单次退出协调会话：需就绪的窗口集合与已就绪集合。
#[derive(Debug, Default)]
pub struct QuitSession {
    expected: BTreeSet<String>,
    ready: BTreeSet<String>,
}

impl QuitSession {
    /// 创建会话（expected 为主窗口 label 集合；调用方保证非空）。
    pub fn new(labels: impl IntoIterator<Item = String>) -> Self {
        Self {
            expected: labels.into_iter().collect(),
            ready: BTreeSet::new(),
        }
    }

    /// 标记某窗口已就绪；返回是否全部就绪。
    pub fn mark_ready(&mut self, label: &str) -> bool {
        self.ready.insert(label.to_string());
        !self.expected.is_empty() && self.ready.is_superset(&self.expected)
    }
}

/// 就绪回报的结果。
#[derive(Debug, PartialEq, Eq)]
pub enum ReadyOutcome {
    /// 当前没有进行中的退出会话（忽略）。
    Idle,
    /// 已记录，仍有窗口未就绪。
    Waiting,
    /// 全部就绪（会话已被消费清除）。
    AllReady,
}

/// 托管状态：`None` 表示当前无退出会话。
#[derive(Default)]
pub struct QuitState(Mutex<Option<QuitSession>>);

impl QuitState {
    /// 开启退出会话；单窗口（labels ≤ 1）不建会话并返回 `false`（调用方走窗口本地流程）。
    pub fn begin(&self, labels: Vec<String>) -> bool {
        let mut guard = self.0.lock().expect("quit state poisoned");
        if labels.len() <= 1 {
            *guard = None;
            return false;
        }
        *guard = Some(QuitSession::new(labels));
        true
    }

    /// 标记窗口就绪；全部就绪时消费并清除会话。
    pub fn mark_ready(&self, label: &str) -> ReadyOutcome {
        let mut guard = self.0.lock().expect("quit state poisoned");
        let Some(session) = guard.as_mut() else {
            return ReadyOutcome::Idle;
        };
        if session.mark_ready(label) {
            *guard = None;
            ReadyOutcome::AllReady
        } else {
            ReadyOutcome::Waiting
        }
    }

    /// 取消退出会话；返回是否存在进行中的会话。
    pub fn cancel(&self) -> bool {
        let mut guard = self.0.lock().expect("quit state poisoned");
        guard.take().is_some()
    }

    /// 当前是否有进行中的会话（测试/诊断用）。
    #[cfg(test)]
    pub fn is_active(&self) -> bool {
        self.0.lock().expect("quit state poisoned").is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 单窗口不建会话() {
        let state = QuitState::default();
        assert!(!state.begin(vec!["main".into()]));
        assert!(!state.is_active());
        assert_eq!(state.mark_ready("main"), ReadyOutcome::Idle);
    }

    #[test]
    fn 多窗口需全部就绪才放行() {
        let state = QuitState::default();
        assert!(state.begin(vec!["main".into(), "main-2".into()]));
        assert_eq!(state.mark_ready("main"), ReadyOutcome::Waiting);
        assert!(state.is_active());
        assert_eq!(state.mark_ready("main-2"), ReadyOutcome::AllReady);
        assert!(!state.is_active(), "全部就绪后会话应被消费");
    }

    #[test]
    fn 重复就绪幂等() {
        let state = QuitState::default();
        state.begin(vec!["main".into(), "main-2".into()]);
        assert_eq!(state.mark_ready("main"), ReadyOutcome::Waiting);
        assert_eq!(state.mark_ready("main"), ReadyOutcome::Waiting);
        assert_eq!(state.mark_ready("main-2"), ReadyOutcome::AllReady);
    }

    #[test]
    fn 取消后忽略后续就绪() {
        let state = QuitState::default();
        state.begin(vec!["main".into(), "main-2".into()]);
        assert!(state.cancel());
        assert!(!state.cancel(), "重复取消无副作用");
        assert_eq!(state.mark_ready("main"), ReadyOutcome::Idle);
        assert_eq!(state.mark_ready("main-2"), ReadyOutcome::Idle);
    }

    #[test]
    fn 未知窗口就绪不会误放行() {
        let state = QuitState::default();
        state.begin(vec!["main".into(), "main-2".into()]);
        assert_eq!(state.mark_ready("ghost"), ReadyOutcome::Waiting);
        assert_eq!(state.mark_ready("main"), ReadyOutcome::Waiting);
        assert_eq!(state.mark_ready("main-2"), ReadyOutcome::AllReady);
    }
}
