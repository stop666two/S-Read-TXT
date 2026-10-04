//! 时间工具：统一 RFC 3339（UTC）生成与解析（历史、会话及后续模块共用）。
//!
//! 约定（设计文档 §10）：时间统一 UTC；序列化使用 RFC 3339；
//! 解析失败返回 `None`，由调用方决定丢弃或回退（绝不 panic）。

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// 当前 UTC 时间的 RFC 3339 字符串（`Z` 时区标识；毫秒精度）。
///
/// 精度说明：历史/会话按时间倒序展示，秒级精度在「快速连续打开」时无法区分先后；
/// 统一使用毫秒精度（仍是合法 RFC 3339，解析端不受影响）。
/// 格式化理论不可失败；为防止日志/历史等辅助路径 panic，兜底为纪元值。
pub fn now_rfc3339() -> String {
    const FORMAT: &[time::format_description::FormatItem<'_>] = time::macros::format_description!(
        "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
    );
    OffsetDateTime::now_utc()
        .format(FORMAT)
        .unwrap_or_else(|_| "1970-01-01T00:00:00.000Z".to_string())
}

/// 当前 UNIX 秒（秒级比较用；与 `now_rfc3339` 同一时刻语义）。
pub fn now_unix_seconds() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// 本地日期字符串（`YYYY-MM-DD`，用于阅读时长统计日界）。
/// 边界：无法获取本地时区时回退 UTC（不会失败）。
pub fn local_day_string() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let format = time::macros::format_description!("[year]-[month]-[day]");
    now.format(format).unwrap_or_else(|_| "1970-01-01".to_string())
}

/// 当前 UNIX 毫秒（毫秒级排序用；与 `now_rfc3339` 的毫秒精度对齐）。
pub fn now_unix_millis() -> i64 {
    let now = OffsetDateTime::now_utc();
    now.unix_timestamp() * 1000 + i64::from(now.millisecond())
}

/// 解析 RFC 3339 时间；非法输入返回 `None`。
pub fn parse_rfc3339(raw: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(raw, &Rfc3339).ok()
}

/// 解析为 UNIX 秒；非法输入返回 `None`。
pub fn unix_seconds(raw: &str) -> Option<i64> {
    parse_rfc3339(raw).map(|moment| moment.unix_timestamp())
}

/// 解析为 UNIX 毫秒（排序键；无小数部分的时间解析为整秒毫秒值）。
pub fn unix_millis(raw: &str) -> Option<i64> {
    parse_rfc3339(raw)
        .map(|moment| moment.unix_timestamp() * 1000 + i64::from(moment.millisecond()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成的时间可解析且带 UTC 标识。
    #[test]
    fn now_is_parseable_and_utc_marked() {
        let now = now_rfc3339();
        assert!(now.ends_with('Z'), "应为 UTC 标识：{now}");
        assert!(parse_rfc3339(&now).is_some());
    }

    /// 非法输入返回 None（不 panic）。
    #[test]
    fn invalid_input_returns_none() {
        assert!(parse_rfc3339("yesterday").is_none());
        assert!(unix_seconds("2026-13-99T00:00:00Z").is_none());
    }

    /// 已知时间点的 UNIX 秒换算正确。
    #[test]
    fn unix_seconds_matches_known_value() {
        assert_eq!(unix_seconds("1970-01-01T00:00:01Z"), Some(1));
    }

    /// 毫秒精度解析：小数部分进入排序键；整秒解析为整千毫秒。
    #[test]
    fn unix_millis_includes_fraction() {
        assert_eq!(unix_millis("1970-01-01T00:00:00.250Z"), Some(250));
        assert_eq!(unix_millis("1970-01-01T00:00:01Z"), Some(1000));
        assert_eq!(unix_millis("not-a-time"), None);
    }

    /// 当前 UNIX 秒为正值（本测试长期有效）。
    #[test]
    fn now_unix_seconds_is_positive() {
        assert!(now_unix_seconds() > 0);
    }
}
