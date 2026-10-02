//! 时间工具：统一 RFC 3339（UTC）生成与解析（历史、会话及后续模块共用）。
//!
//! 约定（设计文档 §10）：时间统一 UTC；序列化使用 RFC 3339；
//! 解析失败返回 `None`，由调用方决定丢弃或回退（绝不 panic）。

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// 当前 UTC 时间的 RFC 3339 字符串（`Z` 时区标识）。
///
/// 说明：格式化理论不可失败；为防止日志/历史等辅助路径 panic，兜底为纪元值。
pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

/// 当前 UNIX 秒（秒级比较用；与 `now_rfc3339` 同一时刻语义）。
pub fn now_unix_seconds() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// 解析 RFC 3339 时间；非法输入返回 `None`。
pub fn parse_rfc3339(raw: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(raw, &Rfc3339).ok()
}

/// 解析为 UNIX 秒；非法输入返回 `None`。
pub fn unix_seconds(raw: &str) -> Option<i64> {
    parse_rfc3339(raw).map(|moment| moment.unix_timestamp())
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

    /// 当前 UNIX 秒为正值（本测试长期有效）。
    #[test]
    fn now_unix_seconds_is_positive() {
        assert!(now_unix_seconds() > 0);
    }
}
