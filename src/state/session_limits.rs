//! Reset deadlines from native session-limit failures, never from account usage.
use super::*;
use chrono::{DateTime, Local, TimeZone, Utc};
use regex::Regex;
use std::sync::LazyLock;

pub(super) fn recognized(text: &str) -> bool {
    [
        "you've hit your session limit",
        "you’ve hit your session limit",
        "session limit reached",
        "session_limit",
    ]
    .iter()
    .any(|pattern| text.contains(pattern))
}

fn clock_reset<T: TimeZone>(zone: T, at: f64, hour: u32, minute: u32) -> Option<f64> {
    let observed = DateTime::<Utc>::from_timestamp(at.floor() as i64, 0)?.with_timezone(&zone);
    let date = observed.date_naive();
    let local = date.and_hms_opt(hour, minute, 0)?;
    // A repeated wall-clock hour or a missing DST hour is not an exact deadline.
    let reset = zone.from_local_datetime(&local).single()?.timestamp() as f64;
    if reset + 60.0 >= at {
        return Some(reset);
    }
    let tomorrow = date.succ_opt()?.and_hms_opt(hour, minute, 0)?;
    Some(zone.from_local_datetime(&tomorrow).single()?.timestamp() as f64)
}

pub(super) fn reset_at(error: &Value) -> Option<f64> {
    if let Some(time) = error["reset_at"]
        .as_f64()
        .filter(|v| v.is_finite() && *v > 0.0)
    {
        return Some(time);
    }
    reported_clock(error).or_else(|| {
        error["retry_not_before"]
            .as_f64()
            .filter(|v| v.is_finite() && *v > 0.0)
    })
}

fn reported_clock(error: &Value) -> Option<f64> {
    static CLOCK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\bresets\s+(\d{1,2})(?::(\d{2}))?\s*(am|pm)?(?:\s*\(([^)\r\n]+)\))?\s*$")
            .unwrap()
    });
    let at = error["at"].as_f64().filter(|v| v.is_finite() && *v > 0.0)?;
    let text = string(error, "detail").trim();
    let clock = CLOCK.captures(text)?;
    let mut hour: u32 = clock[1].parse().ok()?;
    let minute = clock.get(2).map_or(Some(0), |v| v.as_str().parse().ok())?;
    if let Some(period) = clock.get(3) {
        if !(1..=12).contains(&hour) {
            return None;
        }
        hour = hour % 12
            + if period.as_str().eq_ignore_ascii_case("pm") {
                12
            } else {
                0
            };
    } else if clock.get(2).is_none() {
        return None;
    }
    if let Some(zone) = clock.get(4) {
        // Honor the provider's timezone, including when observing a remote host.
        clock_reset(
            zone.as_str().parse::<chrono_tz::Tz>().ok()?,
            at,
            hour,
            minute,
        )
    } else {
        clock_reset(Local, at, hour, minute)
    }
}

pub(super) fn enrich(error: &mut Value) {
    if error["error_kind"] == "session_limit" {
        if let Some(reset) = reset_at(error) {
            error["reset_at"] = json!(reset);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn error(detail: &str, at: &str) -> Value {
        json!({"detail":detail,"at":DateTime::parse_from_rfc3339(at).unwrap().timestamp() as f64})
    }
    #[test]
    fn provider_clock_uses_event_date_zone_and_midnight_rollover() {
        for (text, at, reset) in [
            (
                "You've hit your session limit · resets 2:10pm (Europe/Moscow)",
                "2026-10-10T08:37:11Z",
                "2026-10-10T11:10:00Z",
            ),
            (
                "You've hit your session limit · resets 12am (Europe/Moscow)",
                "2026-10-10T20:00:00Z",
                "2026-10-10T21:00:00Z",
            ),
            (
                "You've hit your session limit · resets 00:10 (Asia/Tokyo)",
                "2026-10-10T16:00:00Z",
                "2026-10-11T15:10:00Z",
            ),
            (
                "You've hit your session limit · resets 12pm (UTC)",
                "2026-10-10T08:00:00Z",
                "2026-10-10T12:00:00Z",
            ),
            (
                "You've hit your session limit · resets 2:10pm (Europe/Moscow)",
                "2026-10-10T11:10:20Z",
                "2026-10-10T11:10:00Z",
            ),
        ] {
            assert_eq!(
                reset_at(&error(text, at)),
                Some(DateTime::parse_from_rfc3339(reset).unwrap().timestamp() as f64),
                "{text}"
            );
        }
    }
    #[test]
    fn unknown_or_ambiguous_deadlines_are_not_guessed() {
        for text in [
            "Try after reset",
            "resets 13pm (UTC)",
            "resets 25:10 (UTC)",
            "resets 2:70pm (UTC)",
            "resets 2pm (Unknown/Zone)",
            "resets 2pm (PST)",
            "resets 2 (UTC)",
            "resets 2pm tomorrow (UTC)",
            "resets 2pm (UTC) quoted prose",
        ] {
            assert_eq!(
                reset_at(&error(text, "2026-10-10T08:00:00Z")),
                None,
                "{text}"
            );
        }
        for (text, at) in [
            ("resets 1:30am (America/New_York)", "2026-11-01T04:00:00Z"),
            ("resets 2:30am (America/New_York)", "2026-03-08T05:00:00Z"),
        ] {
            assert_eq!(reset_at(&error(text, at)), None);
        }
    }
    #[test]
    fn reset_clock_is_distinct_from_a_short_retry_after() {
        let mut failure = error(
            "You've hit your session limit · resets 2:10pm (Europe/Moscow)",
            "2026-10-10T08:37:11Z",
        );
        failure["retry_not_before"] = json!(failure["at"].as_f64().unwrap() + 30.0);
        assert_eq!(
            reset_at(&failure),
            Some(
                DateTime::parse_from_rfc3339("2026-10-10T11:10:00Z")
                    .unwrap()
                    .timestamp() as f64
            )
        );
        failure["detail"] = json!("You've hit your session limit");
        assert_eq!(reset_at(&failure), failure["retry_not_before"].as_f64());
    }
}
