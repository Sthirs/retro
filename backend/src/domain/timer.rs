//! The meeting timer (spec §8.2): a countdown the facilitator starts, pauses,
//! extends and stops. Stored as `meetings.timer_state`; clients count down
//! from `endsAt` using the server time they were given, so no server process
//! ticks.

use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use utoipa::ToSchema;

use crate::error::AppError;

/// Longest duration the timer can be set to: 99:59.
pub const MAX_SECONDS: i64 = 99 * 60 + 59;
/// Most time one "add" can add.
pub const MAX_ADD_SECONDS: i64 = 60 * 60;

/// Whether the countdown is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum TimerStatus {
    Running,
    Paused,
}

/// The timer as stored and as clients see it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Timer {
    pub status: TimerStatus,
    /// When a running countdown reaches zero (it may already have).
    #[serde(with = "time::serde::rfc3339::option", default)]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub ends_at: Option<OffsetDateTime>,
    /// Time left on a paused countdown.
    #[serde(default)]
    pub remaining_ms: Option<i64>,
    /// The duration it was set to, plus the time added since.
    pub duration_ms: i64,
}

/// What the facilitator does with the timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum TimerAction {
    /// Starts a new countdown (replacing any), 1 s to 99:59.
    Start {
        #[serde(rename = "durationSeconds")]
        duration_seconds: i64,
    },
    Pause,
    Resume,
    /// Adds time (1 s to 1 h); a countdown that ran out restarts from now.
    Add {
        seconds: i64,
    },
    /// Removes the timer.
    Stop,
}

impl Timer {
    /// Reads a stored timer; anything unreadable counts as no timer.
    pub fn from_json(value: Option<&serde_json::Value>) -> Option<Self> {
        value.and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a timer serializes")
    }
}

fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.whole_milliseconds()).unwrap_or(i64::MAX)
}

/// Applies `action` at `now`. Errors: `invalid_duration` (out of range) and
/// `no_timer` (pause, resume or add without a timer).
pub fn apply(
    current: Option<Timer>,
    action: TimerAction,
    now: OffsetDateTime,
) -> Result<Option<Timer>, AppError> {
    let invalid = || AppError::Invalid("invalid_duration");
    let no_timer = || AppError::Conflict("no_timer");
    Ok(match action {
        TimerAction::Start { duration_seconds } => {
            if !(1..=MAX_SECONDS).contains(&duration_seconds) {
                return Err(invalid());
            }
            Some(Timer {
                status: TimerStatus::Running,
                ends_at: Some(now + Duration::seconds(duration_seconds)),
                remaining_ms: None,
                duration_ms: duration_seconds * 1000,
            })
        }
        TimerAction::Stop => None,
        TimerAction::Pause => {
            let timer = current.ok_or_else(no_timer)?;
            match (timer.status, timer.ends_at) {
                (TimerStatus::Running, Some(ends_at)) => Some(Timer {
                    status: TimerStatus::Paused,
                    ends_at: None,
                    remaining_ms: Some(millis(ends_at - now).max(0)),
                    ..timer
                }),
                _ => Some(timer),
            }
        }
        TimerAction::Resume => {
            let timer = current.ok_or_else(no_timer)?;
            match (timer.status, timer.remaining_ms) {
                (TimerStatus::Paused, Some(remaining)) => Some(Timer {
                    status: TimerStatus::Running,
                    ends_at: Some(now + Duration::milliseconds(remaining)),
                    remaining_ms: None,
                    ..timer
                }),
                _ => Some(timer),
            }
        }
        TimerAction::Add { seconds } => {
            if !(1..=MAX_ADD_SECONDS).contains(&seconds) {
                return Err(invalid());
            }
            let timer = current.ok_or_else(no_timer)?;
            let added = Duration::seconds(seconds);
            let duration_ms = timer.duration_ms.saturating_add(seconds * 1000);
            match timer.status {
                TimerStatus::Running => {
                    let ends_at = timer.ends_at.unwrap_or(now).max(now) + added;
                    Some(Timer {
                        ends_at: Some(ends_at),
                        duration_ms,
                        ..timer
                    })
                }
                TimerStatus::Paused => Some(Timer {
                    remaining_ms: Some(timer.remaining_ms.unwrap_or(0) + millis(added)),
                    duration_ms,
                    ..timer
                }),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_800_000_000 + seconds).unwrap()
    }

    fn start(seconds: i64) -> Timer {
        apply(
            None,
            TimerAction::Start {
                duration_seconds: seconds,
            },
            at(0),
        )
        .unwrap()
        .unwrap()
    }

    #[test]
    fn starts_counting_down_from_now() {
        let timer = start(300);
        assert_eq!(timer.status, TimerStatus::Running);
        assert_eq!(timer.ends_at, Some(at(300)));
        assert_eq!(timer.duration_ms, 300_000);
    }

    #[test]
    fn rejects_durations_out_of_range() {
        for seconds in [0, -1, MAX_SECONDS + 1] {
            let action = TimerAction::Start {
                duration_seconds: seconds,
            };
            assert!(matches!(
                apply(None, action, at(0)),
                Err(AppError::Invalid("invalid_duration"))
            ));
        }
        assert!(
            apply(
                None,
                TimerAction::Start {
                    duration_seconds: MAX_SECONDS
                },
                at(0)
            )
            .is_ok()
        );
    }

    #[test]
    fn pause_keeps_the_time_left_and_resume_restarts_from_now() {
        let paused = apply(Some(start(300)), TimerAction::Pause, at(100))
            .unwrap()
            .unwrap();
        assert_eq!(paused.status, TimerStatus::Paused);
        assert_eq!(paused.remaining_ms, Some(200_000));
        assert_eq!(paused.ends_at, None);
        // Pausing again changes nothing.
        assert_eq!(
            apply(Some(paused.clone()), TimerAction::Pause, at(150)).unwrap(),
            Some(paused.clone())
        );
        let resumed = apply(Some(paused), TimerAction::Resume, at(1000))
            .unwrap()
            .unwrap();
        assert_eq!(resumed.status, TimerStatus::Running);
        assert_eq!(resumed.ends_at, Some(at(1200)));
    }

    #[test]
    fn pausing_after_the_end_leaves_nothing() {
        let paused = apply(Some(start(10)), TimerAction::Pause, at(50))
            .unwrap()
            .unwrap();
        assert_eq!(paused.remaining_ms, Some(0));
    }

    #[test]
    fn adding_extends_and_restarts_an_expired_countdown() {
        let running = apply(Some(start(60)), TimerAction::Add { seconds: 60 }, at(10))
            .unwrap()
            .unwrap();
        assert_eq!(running.ends_at, Some(at(120)));
        assert_eq!(running.duration_ms, 120_000);
        // Ran out at 60: one more minute from now.
        let expired = apply(Some(start(60)), TimerAction::Add { seconds: 60 }, at(500))
            .unwrap()
            .unwrap();
        assert_eq!(expired.ends_at, Some(at(560)));
        let paused = apply(Some(start(60)), TimerAction::Pause, at(30)).unwrap();
        let paused = apply(paused, TimerAction::Add { seconds: 60 }, at(40))
            .unwrap()
            .unwrap();
        assert_eq!(paused.remaining_ms, Some(90_000));
    }

    #[test]
    fn needs_a_timer_except_to_start_or_stop() {
        for action in [
            TimerAction::Pause,
            TimerAction::Resume,
            TimerAction::Add { seconds: 60 },
        ] {
            assert!(matches!(
                apply(None, action, at(0)),
                Err(AppError::Conflict("no_timer"))
            ));
        }
        assert_eq!(apply(None, TimerAction::Stop, at(0)).unwrap(), None);
        assert_eq!(
            apply(Some(start(5)), TimerAction::Stop, at(0)).unwrap(),
            None
        );
        assert!(matches!(
            apply(Some(start(5)), TimerAction::Add { seconds: 0 }, at(0)),
            Err(AppError::Invalid("invalid_duration"))
        ));
    }

    #[test]
    fn round_trips_through_json() {
        let timer = start(90);
        assert_eq!(Timer::from_json(Some(&timer.to_json())), Some(timer));
        assert_eq!(Timer::from_json(Some(&serde_json::json!({"bad": 1}))), None);
        assert_eq!(Timer::from_json(None), None);
    }

    #[test]
    fn actions_parse_from_the_request_shape() {
        let start: TimerAction =
            serde_json::from_str(r#"{"action":"start","durationSeconds":300}"#).unwrap();
        assert_eq!(
            start,
            TimerAction::Start {
                duration_seconds: 300
            }
        );
        let add: TimerAction = serde_json::from_str(r#"{"action":"add","seconds":60}"#).unwrap();
        assert_eq!(add, TimerAction::Add { seconds: 60 });
        let stop: TimerAction = serde_json::from_str(r#"{"action":"stop"}"#).unwrap();
        assert_eq!(stop, TimerAction::Stop);
    }
}
