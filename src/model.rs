//! Task data model and id generation.

use chrono::{DateTime, FixedOffset, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Lifecycle status of a [`Task`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Open,
    Blocked,
    Done,
}

/// A single task record, as stored (one JSON object per line) in
/// `tasks.jsonl` (active/blocked) or `archive.jsonl` (done).
///
/// `#[serde(default)]` is used liberally so that older records (or records
/// missing fields added in a future version) still deserialize instead of
/// crashing the binary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub text: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub due: Option<NaiveDate>,
    pub created_at: DateTime<FixedOffset>,
    #[serde(default)]
    pub completed_at: Option<DateTime<FixedOffset>>,
}

fn default_category() -> String {
    "intake".to_string()
}

impl Task {
    /// Create a new open task, capturing `now` (local time, with offset) as
    /// `created_at`.
    pub fn new(
        text: impl Into<String>,
        category: impl Into<String>,
        project: Option<String>,
        due: Option<NaiveDate>,
    ) -> Self {
        Task {
            id: generate_id(),
            text: text.into(),
            category: category.into(),
            project,
            status: Status::Open,
            due,
            created_at: Local::now().fixed_offset(),
            completed_at: None,
        }
    }
}

/// Generate a short random task id: `t_` followed by 6 lowercase base36
/// characters.
///
/// This intentionally avoids pulling in a dependency such as `rand`: a tiny
/// xorshift PRNG seeded from wall-clock nanoseconds, the process id, and a
/// process-local call counter is more than sufficient entropy for a
/// human-scale identifier that only needs to avoid collisions within one
/// user's task list.
pub fn generate_id() -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let pid = std::process::id() as u64;

    let mut state = nanos ^ (pid << 32) ^ counter.wrapping_mul(0x9E3779B97F4A7C15);
    if state == 0 {
        state = 0x9E3779B97F4A7C15;
    }

    let mut suffix = String::with_capacity(6);
    for _ in 0..6 {
        // xorshift64
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let idx = (state % ALPHABET.len() as u64) as usize;
        suffix.push(ALPHABET[idx] as char);
    }

    format!("t_{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_task() -> Task {
        let offset = FixedOffset::west_opt(5 * 3600).unwrap();
        Task {
            id: "t_abc123".to_string(),
            text: "Fix login flow bug".to_string(),
            category: "engineering".to_string(),
            project: Some("auth-revamp".to_string()),
            status: Status::Open,
            due: Some(NaiveDate::from_ymd_opt(2026, 7, 10).unwrap()),
            created_at: offset.with_ymd_and_hms(2026, 7, 7, 9, 14, 0).unwrap(),
            completed_at: None,
        }
    }

    #[test]
    fn generate_id_has_expected_shape() {
        let id = generate_id();
        assert!(id.starts_with("t_"));
        assert_eq!(id.len(), 8);
        assert!(
            id[2..]
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        );
    }

    #[test]
    fn generate_id_is_not_trivially_repeated() {
        let ids: std::collections::HashSet<String> = (0..64).map(|_| generate_id()).collect();
        // xorshift + counter should not collide across 64 rapid calls.
        assert_eq!(ids.len(), 64);
    }

    #[test]
    fn task_round_trips_through_json() {
        let task = sample_task();
        let json = serde_json::to_string(&task).unwrap();
        let decoded: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(task, decoded);
    }

    #[test]
    fn status_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Status::Open).unwrap(), "\"open\"");
        assert_eq!(
            serde_json::to_string(&Status::Blocked).unwrap(),
            "\"blocked\""
        );
        assert_eq!(serde_json::to_string(&Status::Done).unwrap(), "\"done\"");
    }

    #[test]
    fn deserialize_tolerates_unknown_fields() {
        let json = r#"{
            "id": "t_zzz999",
            "text": "some task",
            "category": "intake",
            "status": "open",
            "created_at": "2026-07-07T09:14:00-05:00",
            "from_the_future": {"nested": true}
        }"#;
        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.id, "t_zzz999");
        assert_eq!(task.project, None);
        assert_eq!(task.due, None);
        assert_eq!(task.completed_at, None);
    }

    #[test]
    fn deserialize_fills_defaults_for_missing_optional_fields() {
        let json = r#"{
            "id": "t_min0001",
            "text": "minimal",
            "created_at": "2026-07-07T09:14:00-05:00"
        }"#;
        let task: Task = serde_json::from_str(json).unwrap();
        assert_eq!(task.category, "intake");
        assert_eq!(task.status, Status::Open);
        assert_eq!(task.project, None);
        assert_eq!(task.due, None);
        assert_eq!(task.completed_at, None);
    }
}
