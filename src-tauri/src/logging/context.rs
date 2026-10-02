//! 线程本地日志上下文：为日志行附加 `tab`/`req` 链路标识。
//!
//! 背景：本应用完全离线、无跨服务调用；标识用于把同一标签/同一次 IPC 请求
//! 产生的日志串联起来（W3C Trace Context 的本地化简化）。
//! 语义：上下文仅在当前线程内生效；[`with_context`] 结束（含 panic 展开）后自动恢复。

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

/// 全局单调递增的请求 id（从 1 开始；仅本地日志串联用，无隐私含义）。
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// 分配下一个请求 id。
pub fn next_request_id() -> u64 {
    NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

/// 日志上下文（两个字段均可缺省）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LogContext {
    /// 标签 id（多标签场景）
    pub tab: Option<u64>,
    /// 请求 id（一次 IPC 命令调用）
    pub req: Option<u64>,
}

impl LogContext {
    /// 构造「一次 IPC 请求」上下文：自动分配请求 id，无标签。
    pub fn request() -> Self {
        Self {
            tab: None,
            req: Some(next_request_id()),
        }
    }
}

thread_local! {
    /// 当前线程上下文（默认全空）。
    static CONTEXT: RefCell<LogContext> = RefCell::new(LogContext { tab: None, req: None });
}

/// 恢复守卫：离开作用域（含 panic）时还原上下文。
struct RestoreGuard(LogContext);

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        CONTEXT.with(|cell| {
            cell.replace(self.0);
        });
    }
}

/// 在指定上下文中执行闭包，结束后自动恢复原上下文（panic 安全）。
pub fn with_context<R>(context: LogContext, f: impl FnOnce() -> R) -> R {
    let _guard = CONTEXT.with(|cell| RestoreGuard(cell.replace(context)));
    f()
}

/// 读取当前线程上下文（logger 写行时调用）。
pub fn current_context() -> LogContext {
    CONTEXT.with(|cell| *cell.borrow())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认上下文为空。
    #[test]
    fn default_context_is_empty() {
        assert_eq!(current_context(), LogContext::default());
    }

    /// 设置后可见，退出后恢复。
    #[test]
    fn with_context_sets_and_restores() {
        let ctx = LogContext {
            tab: Some(1),
            req: Some(2),
        };
        let inside = with_context(ctx, current_context);
        assert_eq!(inside, ctx);
        assert_eq!(current_context(), LogContext::default());
    }

    /// 嵌套上下文按栈序恢复。
    #[test]
    fn nested_contexts_restore_in_order() {
        let outer = LogContext {
            tab: Some(1),
            req: None,
        };
        let inner = LogContext {
            tab: Some(2),
            req: Some(3),
        };
        with_context(outer, || {
            assert_eq!(current_context(), outer);
            with_context(inner, || assert_eq!(current_context(), inner));
            assert_eq!(current_context(), outer);
        });
    }

    /// panic 展开时也必须恢复（防止污染后续日志）。
    #[test]
    fn context_restored_after_panic() {
        let ctx = LogContext {
            tab: Some(9),
            req: None,
        };
        let result = std::panic::catch_unwind(|| with_context(ctx, || panic!("boom")));
        assert!(result.is_err());
        assert_eq!(current_context(), LogContext::default());
    }

    /// request() 生成递增且唯一的请求 id，且不带标签。
    #[test]
    fn request_ids_are_unique_and_increasing() {
        let first = LogContext::request();
        let second = LogContext::request();
        assert!(first.req.is_some() && second.req.is_some());
        assert!(second.req.unwrap() > first.req.unwrap());
        assert_eq!(first.tab, None);
    }
}
