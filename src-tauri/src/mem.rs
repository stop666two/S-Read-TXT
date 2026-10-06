//! 进程内存辅助：工作集修剪。
//!
//! 大文件通过 mmap 访问，检测、行索引、统计、差异计算等全量扫描会把
//! 文件页带入工作集。这些页是干净的文件后备页：修剪只是把它们转入
//! 系统待机列表（再次访问时软缺页即刻换回），不会影响正确性与数据，
//! 可立刻降低进程在任务管理器中的内存显示并释放物理内存余量。

/// 触发修剪的文件规模阈值（字节）：小文件无需处理。
const TRIM_THRESHOLD_BYTES: u64 = 16 * 1024 * 1024;
/// 延迟修剪的等待时长（毫秒）：最后一次大对象活动结束后再修剪。
const SETTLE_DELAY_MS: u64 = 3_000;

/// 是否达到修剪阈值（独立函数便于单测边界）。
fn should_trim(bytes: u64) -> bool {
    bytes >= TRIM_THRESHOLD_BYTES
}

/// 最近一次「大对象活动」时间戳（毫秒）与在途的延迟修剪线程标记。
static LAST_ACTIVITY_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SETTLE_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 将本进程工作集页转入系统待机列表；失败静默（仅调试日志）。
pub fn trim_working_set() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, K32EmptyWorkingSet, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        fn ws_mb() -> u64 {
            let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
            let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c, size) };
            c.WorkingSetSize as u64 / (1024 * 1024)
        }
        let before = ws_mb();
        let ok = K32EmptyWorkingSet(GetCurrentProcess());
        let after = ws_mb();
        log::debug!(target: "sread::mem", "工作集修剪：ok={ok} {before}MB→{after}MB");
    }
}

/// 大对象处理完成后的按需修剪（`bytes` 为本次触碰的文件规模）。
pub fn trim_after_large_work(bytes: u64) {
    if should_trim(bytes) {
        trim_working_set();
        trim_after_settle();
    }
}

/// 延迟修剪（防抖）：打开/统计后的首屏取行会再次触碰文件页，立即修剪
/// 效果有限；记录活动时间并只保留一个等待线程，在最后一次活动结束
/// `SETTLE_DELAY_MS` 后修剪一次，把文件页交给系统待机列表。
pub fn trim_after_settle() {
    use std::sync::atomic::Ordering;
    LAST_ACTIVITY_MS.store(now_ms(), Ordering::Relaxed);
    if SETTLE_PENDING.swap(true, Ordering::Relaxed) {
        return;
    }
    std::thread::spawn(|| {
        loop {
            let last = LAST_ACTIVITY_MS.load(Ordering::Relaxed);
            let now = now_ms();
            if now >= last + SETTLE_DELAY_MS {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(
                (last + SETTLE_DELAY_MS - now).min(SETTLE_DELAY_MS),
            ));
        }
        trim_working_set();
        SETTLE_PENDING.store(false, Ordering::Relaxed);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_threshold_boundary() {
        assert!(!should_trim(TRIM_THRESHOLD_BYTES - 1));
        assert!(should_trim(TRIM_THRESHOLD_BYTES));
    }

    /// 验证进程内修剪对文件后备页有效：mmap 触碰后工作集包含文件页，
    /// 修剪应把工作集降到触碰前水平以下（系统待机列表可回收这些页）。
    #[test]
    fn trim_releases_file_pages() {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::ProcessStatus::{
                GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
            };
            use windows_sys::Win32::System::Threading::GetCurrentProcess;

            fn working_set() -> u64 {
                // SAFETY: PROCESS_MEMORY_COUNTERS 为全零即合法的 POD 结构
                let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
                let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                // SAFETY: 计数器与长度均为本函数内有效的栈对象
                unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) };
                counters.WorkingSetSize as u64
            }

            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("pages.bin");
            std::fs::write(&path, vec![7u8; 64 * 1024 * 1024]).unwrap();
            let file = std::fs::File::open(&path).unwrap();
            // SAFETY: 测试内独占该文件，读写映射生命周期覆盖使用范围
            let mmap = unsafe { memmap2::Mmap::map(&file).unwrap() };
            let checksum: u64 = mmap.iter().map(|byte| u64::from(*byte)).sum();
            assert!(checksum > 0);
            let before = working_set();
            trim_working_set();
            let after = working_set();
            assert!(
                after < before,
                "修剪后工作集应下降：before={before} after={after}"
            );
            drop(mmap);
        }
    }
}
