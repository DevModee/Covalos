//! Supabase rows for Student Mode and the download-merge-upload round trip. Only
//! metrics, lesson state and review cards leave the machine; no code, paths or chat.
//! Schema and RLS policies are in docs/CLOUD_ACCOUNT.md.
use super::curriculum::TrackId;
use super::progress::{LessonProgress, Progress, ReviewCard, Status};
use crate::cloud::{self, Endpoint};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ProfileRow {
    pub(crate) user_id: String,
    pub(crate) active_track: Option<String>,
    pub(crate) streak_days: u32,
    pub(crate) longest_streak: u32,
    pub(crate) last_study_date: Option<String>,
    pub(crate) daily_goal_minutes: u32,
    pub(crate) xp_total: u32,
    pub(crate) updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct LessonRow {
    pub(crate) user_id: String,
    pub(crate) track_id: String,
    pub(crate) lesson_id: String,
    pub(crate) status: String,
    pub(crate) best_score: u8,
    pub(crate) attempts: u32,
    pub(crate) hints_used: u32,
    pub(crate) completed_at: Option<String>,
    pub(crate) updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ReviewRow {
    pub(crate) user_id: String,
    pub(crate) track_id: String,
    pub(crate) lesson_id: String,
    pub(crate) concept: String,
    pub(crate) next_review_at: String,
    pub(crate) interval_days: u32,
    pub(crate) updated_at: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Rows {
    pub(crate) profile: Option<ProfileRow>,
    pub(crate) lessons: Vec<LessonRow>,
    pub(crate) reviews: Vec<ReviewRow>,
}

/// Unix milliseconds as an RFC 3339 UTC timestamp.
pub(crate) fn timestamp(ms: u64) -> String {
    let at = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute(),
        at.second(),
        at.millisecond()
    )
}

/// Accepts what Postgres returns (`+00:00` offsets, microseconds); 0 when unreadable.
pub(crate) fn millis(text: &str) -> u64 {
    time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339)
        .map_or(0, |at| {
            (at.unix_timestamp_nanos() / 1_000_000).max(0) as u64
        })
}

pub(crate) fn to_rows(progress: &Progress, user_id: &str) -> Rows {
    Rows {
        profile: Some(ProfileRow {
            user_id: user_id.to_owned(),
            active_track: progress.active_track.map(|t| t.key().to_owned()),
            streak_days: progress.streak_days,
            longest_streak: progress.longest_streak,
            last_study_date: progress.last_study_date.clone(),
            daily_goal_minutes: progress.daily_goal_minutes,
            xp_total: progress.xp_total,
            updated_at: timestamp(progress.updated_at_ms),
        }),
        lessons: progress
            .lessons
            .iter()
            .map(|l| LessonRow {
                user_id: user_id.to_owned(),
                track_id: l.track.clone(),
                lesson_id: l.lesson.clone(),
                status: l.status.key().to_owned(),
                best_score: l.best_score.min(100),
                attempts: l.attempts,
                hints_used: l.hints_used,
                completed_at: l.completed_at.clone(),
                updated_at: timestamp(l.updated_at_ms),
            })
            .collect(),
        reviews: progress
            .review_queue
            .iter()
            .map(|c| ReviewRow {
                user_id: user_id.to_owned(),
                track_id: c.track.clone(),
                lesson_id: c.lesson.clone(),
                concept: c.concept.clone(),
                next_review_at: c.next_review_at.clone(),
                interval_days: c.interval_days,
                updated_at: timestamp(c.updated_at_ms),
            })
            .collect(),
    }
}

/// The cloud state as a `Progress`, ready to merge into the local one.
pub(crate) fn from_rows(rows: &Rows) -> Progress {
    let mut progress = Progress::default();
    if let Some(p) = &rows.profile {
        progress.active_track = p.active_track.as_deref().and_then(TrackId::from_key);
        progress.streak_days = p.streak_days;
        progress.longest_streak = p.longest_streak;
        progress.last_study_date = p.last_study_date.clone();
        progress.daily_goal_minutes = p.daily_goal_minutes;
        progress.xp_total = p.xp_total;
        progress.updated_at_ms = millis(&p.updated_at);
    }
    progress.lessons = rows
        .lessons
        .iter()
        .map(|l| LessonProgress {
            track: l.track_id.clone(),
            lesson: l.lesson_id.clone(),
            status: Status::from_key(&l.status),
            best_score: l.best_score.min(100),
            attempts: l.attempts,
            hints_used: l.hints_used,
            completed_at: l.completed_at.clone(),
            updated_at_ms: millis(&l.updated_at),
        })
        .collect();
    progress.review_queue = rows
        .reviews
        .iter()
        .map(|c| ReviewCard {
            track: c.track_id.clone(),
            lesson: c.lesson_id.clone(),
            concept: c.concept.clone(),
            next_review_at: c.next_review_at.clone(),
            interval_days: c.interval_days,
            updated_at_ms: millis(&c.updated_at),
        })
        .collect();
    progress
}

fn get<T: for<'de> Deserialize<'de>>(
    endpoint: &Endpoint,
    access: &str,
    table: &str,
    user_id: &str,
) -> Result<Vec<T>, String> {
    let (status, text) = cloud::request(
        endpoint,
        Some(access),
        "GET",
        &format!("/rest/v1/{table}?user_id=eq.{}", cloud::encode(user_id)),
        &[],
        None,
    )?;
    if status != 200 {
        return Err(cloud::server_error(
            "Could not download study progress",
            status,
            &text,
        ));
    }
    serde_json::from_str(&text).map_err(|_| "The server sent unreadable study progress.".into())
}

fn upsert<T: Serialize>(
    endpoint: &Endpoint,
    access: &str,
    table: &str,
    conflict: &str,
    rows: &[T],
) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    let body = serde_json::to_string(rows).map_err(|e| e.to_string())?;
    let (status, text) = cloud::request(
        endpoint,
        Some(access),
        "POST",
        &format!("/rest/v1/{table}?on_conflict={conflict}"),
        &[("Prefer", "resolution=merge-duplicates,return=minimal")],
        Some(&body),
    )?;
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(cloud::server_error(
            "Could not upload study progress",
            status,
            &text,
        ))
    }
}

pub(crate) fn download(endpoint: &Endpoint, access: &str, user_id: &str) -> Result<Rows, String> {
    Ok(Rows {
        profile: get::<ProfileRow>(endpoint, access, "student_profiles", user_id)?
            .into_iter()
            .next(),
        lessons: get(endpoint, access, "student_lesson_progress", user_id)?,
        reviews: get(endpoint, access, "student_review_queue", user_id)?,
    })
}

pub(crate) fn upload(endpoint: &Endpoint, access: &str, rows: &Rows) -> Result<(), String> {
    if let Some(profile) = &rows.profile {
        upsert(
            endpoint,
            access,
            "student_profiles",
            "user_id",
            std::slice::from_ref(profile),
        )?;
        // Cards for passed lessons are gone locally; clear the table before re-adding.
        let (status, text) = cloud::request(
            endpoint,
            Some(access),
            "DELETE",
            &format!(
                "/rest/v1/student_review_queue?user_id=eq.{}",
                cloud::encode(&profile.user_id)
            ),
            &[("Prefer", "return=minimal")],
            None,
        )?;
        if !(200..300).contains(&status) {
            return Err(cloud::server_error(
                "Could not update the review queue",
                status,
                &text,
            ));
        }
    }
    let key = "user_id,track_id,lesson_id";
    upsert(
        endpoint,
        access,
        "student_lesson_progress",
        key,
        &rows.lessons,
    )?;
    upsert(endpoint, access, "student_review_queue", key, &rows.reviews)
}

/// Downloads the cloud copy, merges it into `local` and uploads the result. Returns the
/// merged progress for the caller to save locally.
pub(crate) fn sync(
    endpoint: &Endpoint,
    access: &str,
    user_id: &str,
    local: &Progress,
) -> Result<Progress, String> {
    let remote = from_rows(&download(endpoint, access, user_id)?);
    let mut merged = local.clone();
    merged.merge(&remote);
    upload(endpoint, access, &to_rows(&merged, user_id))?;
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::student::progress::{GradeRecord, SessionScore};

    fn sample() -> Progress {
        let mut p = Progress {
            active_track: Some(TrackId::Python),
            ..Default::default()
        };
        let score = SessionScore {
            date: "2026-11-25".into(),
            track: "python".into(),
            lesson: "py-01-basics".into(),
            total: 40,
            code: 30.0,
            autonomy: 100.0,
            git: Some(80.0),
            english: Some(60.0),
        };
        p.record_grade(
            &GradeRecord {
                track: TrackId::Python,
                lesson: "py-01-basics",
                score: &score,
                hints: 2,
                passed: false,
                today: "2026-11-25",
            },
            1_764_000_000_123,
        );
        p
    }

    #[test]
    fn timestamps_round_trip() {
        let ms = 1_764_000_000_123;
        assert_eq!(timestamp(ms), "2025-11-24T16:00:00.123Z");
        assert_eq!(millis(&timestamp(ms)), ms);
        assert_eq!(millis("2025-11-24T16:00:00.123456+00:00"), ms);
        assert_eq!(millis("garbage"), 0);
    }

    #[test]
    fn rows_round_trip_without_local_history() {
        let p = sample();
        let rows = to_rows(&p, "user-1");
        assert_eq!(rows.lessons.len(), 1);
        assert_eq!(rows.lessons[0].status, "in_progress");
        assert_eq!(rows.reviews.len(), 1);
        let back = from_rows(&rows);
        assert_eq!(back.lessons, p.lessons);
        assert_eq!(back.review_queue, p.review_queue);
        assert_eq!(back.streak_days, p.streak_days);
        assert_eq!(back.active_track, Some(TrackId::Python));
        assert!(back.sessions.is_empty(), "session history stays local");
    }

    #[test]
    fn uploaded_rows_hold_no_code_or_paths() {
        let mut p = sample();
        p.sessions[0].lesson = "def classify_age(age): return 'secret'".into();
        let rows = to_rows(&p, "user-1");
        let json = serde_json::to_string(&(&rows.profile, &rows.lessons, &rows.reviews)).unwrap();
        assert!(!json.contains("def "));
        assert!(!json.contains("secret"));
        assert!(!json.contains(":\\") && !json.contains("src/"));
    }

    #[test]
    fn cloud_rows_merge_into_local_progress() {
        let mut local = sample();
        let mut cloud_side = sample();
        cloud_side.lessons[0].best_score = 95;
        cloud_side.lessons[0].status = Status::Completed;
        local.merge(&from_rows(&to_rows(&cloud_side, "user-1")));
        assert_eq!(local.lessons[0].best_score, 95);
        assert_eq!(local.lessons[0].status, Status::Completed);
        assert!(local.review_queue.is_empty());
        assert_eq!(local.sessions.len(), 1);
    }

    #[test]
    fn unknown_values_from_the_server_are_tolerated() {
        let rows = Rows {
            profile: Some(ProfileRow {
                user_id: "u".into(),
                active_track: Some("cobol".into()),
                streak_days: 1,
                longest_streak: 1,
                last_study_date: None,
                daily_goal_minutes: 20,
                xp_total: 0,
                updated_at: "not a date".into(),
            }),
            lessons: vec![LessonRow {
                user_id: "u".into(),
                track_id: "python".into(),
                lesson_id: "py-01-basics".into(),
                status: "weird".into(),
                best_score: 250,
                attempts: 1,
                hints_used: 0,
                completed_at: None,
                updated_at: String::new(),
            }],
            reviews: vec![],
        };
        let p = from_rows(&rows);
        assert_eq!(p.active_track, None);
        assert_eq!(p.lessons[0].status, Status::Locked);
        assert_eq!(p.lessons[0].best_score, 100);
    }
}
