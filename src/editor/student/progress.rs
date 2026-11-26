//! Offline-first progress: saved locally after every change and merged with the cloud
//! copy when an account is signed in. Holds metrics and lesson state only, never code.
use super::curriculum::TrackId;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(crate) const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    #[default]
    Locked,
    InProgress,
    Completed,
}

impl Status {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Locked => "locked",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    pub(crate) fn from_key(key: &str) -> Self {
        match key {
            "completed" => Self::Completed,
            "in_progress" => Self::InProgress,
            _ => Self::Locked,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct LessonProgress {
    pub(crate) track: String,
    pub(crate) lesson: String,
    pub(crate) status: Status,
    pub(crate) best_score: u8,
    pub(crate) attempts: u32,
    pub(crate) hints_used: u32,
    /// `YYYY-MM-DD`.
    pub(crate) completed_at: Option<String>,
    pub(crate) updated_at_ms: u64,
}

/// A weak concept scheduled for spaced repetition.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct ReviewCard {
    pub(crate) track: String,
    pub(crate) lesson: String,
    pub(crate) concept: String,
    /// `YYYY-MM-DD`.
    pub(crate) next_review_at: String,
    pub(crate) interval_days: u32,
    pub(crate) updated_at_ms: u64,
}

/// Area scores (0..=100) from one graded exercise. Git and English are absent when the
/// student turned that coaching off or there was nothing to check.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct SessionScore {
    pub(crate) date: String,
    pub(crate) track: String,
    pub(crate) lesson: String,
    pub(crate) total: u8,
    pub(crate) code: f32,
    pub(crate) autonomy: f32,
    pub(crate) git: Option<f32>,
    pub(crate) english: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Coaching {
    pub(crate) professional_git: bool,
    pub(crate) technical_english: bool,
}

impl Default for Coaching {
    fn default() -> Self {
        Self {
            professional_git: true,
            technical_english: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Progress {
    pub(crate) version: u32,
    pub(crate) active_track: Option<TrackId>,
    pub(crate) streak_days: u32,
    pub(crate) longest_streak: u32,
    pub(crate) last_study_date: Option<String>,
    pub(crate) daily_goal_minutes: u32,
    pub(crate) xp_total: u32,
    pub(crate) lessons: Vec<LessonProgress>,
    pub(crate) review_queue: Vec<ReviewCard>,
    /// Local history for the charts; not uploaded.
    pub(crate) sessions: Vec<SessionScore>,
    pub(crate) coaching: Coaching,
    /// When the profile fields (track, streak, goal) last changed.
    pub(crate) updated_at_ms: u64,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            version: VERSION,
            active_track: None,
            streak_days: 0,
            longest_streak: 0,
            last_study_date: None,
            daily_goal_minutes: 20,
            xp_total: 0,
            lessons: Vec::new(),
            review_queue: Vec::new(),
            sessions: Vec::new(),
            coaching: Coaching::default(),
            updated_at_ms: 0,
        }
    }
}

const MAX_SESSIONS: usize = 200;

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Today's local date as `YYYY-MM-DD`.
pub(crate) fn today() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    format_date(now.date())
}

fn format_date(date: time::Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
}

fn parse_date(text: &str) -> Option<time::Date> {
    let mut parts = text.split('-').map(str::parse::<i32>);
    let (Some(Ok(y)), Some(Ok(m)), Some(Ok(d))) = (parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    let month = time::Month::try_from(u8::try_from(m).ok()?).ok()?;
    time::Date::from_calendar_date(y, month, u8::try_from(d).ok()?).ok()
}

fn add_days(date: &str, days: u32) -> String {
    parse_date(date)
        .and_then(|d| d.checked_add(time::Duration::days(i64::from(days))))
        .map_or_else(|| date.to_owned(), format_date)
}

impl Progress {
    pub(crate) fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<Self>(&text).ok())
            .filter(|p| p.version <= VERSION)
            .unwrap_or_default()
    }

    /// Writes through a temporary file so a crash never leaves half a file behind.
    pub(crate) fn save(&self, path: &Path) -> Result<(), String> {
        let dir = path.parent().ok_or("Invalid progress path.")?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
        std::io::Write::write_all(&mut tmp, text.as_bytes()).map_err(|e| e.to_string())?;
        tmp.persist(path).map_err(|e| e.error.to_string())?;
        Ok(())
    }

    /// Counts a study day: consecutive days grow the streak, a gap restarts it.
    pub(crate) fn record_study_day(&mut self, today: &str, now: u64) {
        if self.last_study_date.as_deref() == Some(today) {
            return;
        }
        let consecutive = self
            .last_study_date
            .as_deref()
            .is_some_and(|last| add_days(last, 1) == today);
        self.streak_days = if consecutive { self.streak_days + 1 } else { 1 };
        self.longest_streak = self.longest_streak.max(self.streak_days);
        self.last_study_date = Some(today.to_owned());
        self.updated_at_ms = now;
    }

    pub(crate) fn lesson(&self, track: TrackId, lesson: &str) -> Option<&LessonProgress> {
        self.lessons
            .iter()
            .find(|l| l.track == track.key() && l.lesson == lesson)
    }

    fn lesson_mut(&mut self, track: TrackId, lesson: &str) -> &mut LessonProgress {
        let index = match self
            .lessons
            .iter()
            .position(|l| l.track == track.key() && l.lesson == lesson)
        {
            Some(index) => index,
            None => {
                self.lessons.push(LessonProgress {
                    track: track.key().to_owned(),
                    lesson: lesson.to_owned(),
                    ..Default::default()
                });
                self.lessons.len() - 1
            }
        };
        &mut self.lessons[index]
    }

    pub(crate) fn start_lesson(&mut self, track: TrackId, lesson: &str, now: u64) {
        let entry = self.lesson_mut(track, lesson);
        if entry.status == Status::Locked {
            entry.status = Status::InProgress;
            entry.updated_at_ms = now;
        }
    }

    /// Index of the first lesson not completed yet.
    pub(crate) fn next_lesson(&self, track: TrackId) -> usize {
        track
            .lessons()
            .iter()
            .position(|l| {
                self.lesson(track, l.id)
                    .is_none_or(|p| p.status != Status::Completed)
            })
            .unwrap_or(track.lessons().len())
    }

    /// Whether the student has ever started this track.
    pub(crate) fn started(&self, track: TrackId) -> bool {
        self.lessons.iter().any(|l| l.track == track.key())
    }

    /// Records one graded attempt. Passing completes the lesson and earns xp once; a
    /// score under the pass mark puts the lesson's first concept in the review queue.
    pub(crate) fn record_grade(&mut self, grade: &GradeRecord<'_>, now: u64) {
        let (track, lesson, today) = (grade.track, grade.lesson, grade.today);
        let entry = self.lesson_mut(track, lesson);
        entry.attempts += 1;
        entry.hints_used += u32::from(grade.hints);
        entry.best_score = entry.best_score.max(grade.score.total);
        entry.updated_at_ms = now;
        let first_pass = grade.passed && entry.status != Status::Completed;
        if grade.passed {
            entry.status = Status::Completed;
            entry.completed_at.get_or_insert_with(|| today.to_owned());
        } else if entry.status == Status::Locked {
            entry.status = Status::InProgress;
        }
        if first_pass {
            self.xp_total += u32::from(grade.score.total);
        }
        let concept = track
            .lesson(lesson)
            .and_then(|l| l.concepts.first())
            .copied()
            .unwrap_or(lesson);
        let same = |c: &ReviewCard| c.track == track.key() && c.lesson == lesson;
        if grade.passed {
            self.review_queue.retain(|c| !same(c));
        } else if !self.review_queue.iter().any(same) {
            self.review_queue.push(ReviewCard {
                track: track.key().to_owned(),
                lesson: lesson.to_owned(),
                concept: concept.to_owned(),
                next_review_at: add_days(today, 1),
                interval_days: 1,
                updated_at_ms: now,
            });
        }
        self.sessions.push(grade.score.clone());
        if self.sessions.len() > MAX_SESSIONS {
            self.sessions.drain(..self.sessions.len() - MAX_SESSIONS);
        }
        self.record_study_day(today, now);
        self.updated_at_ms = now;
    }

    /// The oldest card due today or earlier for this track.
    pub(crate) fn due_review(&self, track: TrackId, today: &str) -> Option<&ReviewCard> {
        self.review_queue
            .iter()
            .filter(|c| c.track == track.key() && c.next_review_at.as_str() <= today)
            .min_by(|a, b| a.next_review_at.cmp(&b.next_review_at))
    }

    /// The review was answered; the next one comes after twice the interval.
    pub(crate) fn reviewed(&mut self, track: TrackId, lesson: &str, today: &str, now: u64) {
        if let Some(card) = self
            .review_queue
            .iter_mut()
            .find(|c| c.track == track.key() && c.lesson == lesson)
        {
            card.interval_days = (card.interval_days * 2).clamp(1, 30);
            card.next_review_at = add_days(today, card.interval_days);
            card.updated_at_ms = now;
        }
    }

    /// Average best score across completed lessons of a track.
    pub(crate) fn average_score(&self, track: TrackId) -> Option<f32> {
        let scores: Vec<f32> = self
            .lessons
            .iter()
            .filter(|l| l.track == track.key() && l.status == Status::Completed)
            .map(|l| f32::from(l.best_score))
            .collect();
        (!scores.is_empty()).then(|| scores.iter().sum::<f32>() / scores.len() as f32)
    }

    /// Concepts waiting in the review queue for a track.
    pub(crate) fn weak_topics(&self, track: TrackId) -> Vec<&str> {
        self.review_queue
            .iter()
            .filter(|c| c.track == track.key())
            .map(|c| c.concept.as_str())
            .collect()
    }

    /// Merges another copy (usually the cloud one) into this one. Lessons keep the best
    /// score, the most attempts and the most advanced status; profile fields come from
    /// the newer copy; review cards keep the newer edit. Local session history stays.
    pub(crate) fn merge(&mut self, other: &Progress) {
        if other.updated_at_ms > self.updated_at_ms {
            self.active_track = other.active_track.or(self.active_track);
            self.streak_days = other.streak_days;
            self.last_study_date = other.last_study_date.clone();
            self.daily_goal_minutes = other.daily_goal_minutes;
            self.updated_at_ms = other.updated_at_ms;
        }
        self.longest_streak = self.longest_streak.max(other.longest_streak);
        self.xp_total = self.xp_total.max(other.xp_total);
        for theirs in &other.lessons {
            let mine = self
                .lessons
                .iter_mut()
                .find(|l| l.track == theirs.track && l.lesson == theirs.lesson);
            let Some(mine) = mine else {
                self.lessons.push(theirs.clone());
                continue;
            };
            mine.best_score = mine.best_score.max(theirs.best_score);
            mine.attempts = mine.attempts.max(theirs.attempts);
            mine.hints_used = mine.hints_used.max(theirs.hints_used);
            mine.status = mine.status.max(theirs.status);
            mine.completed_at = match (mine.completed_at.take(), theirs.completed_at.clone()) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            mine.updated_at_ms = mine.updated_at_ms.max(theirs.updated_at_ms);
        }
        for theirs in &other.review_queue {
            match self
                .review_queue
                .iter_mut()
                .find(|c| c.track == theirs.track && c.lesson == theirs.lesson)
            {
                Some(mine) if theirs.updated_at_ms > mine.updated_at_ms => *mine = theirs.clone(),
                Some(_) => {}
                None => self.review_queue.push(theirs.clone()),
            }
        }
        // A lesson completed on either side no longer needs reviewing.
        let lessons = &self.lessons;
        self.review_queue.retain(|c| {
            !lessons.iter().any(|l| {
                l.track == c.track && l.lesson == c.lesson && l.status == Status::Completed
            })
        });
    }
}

/// One graded attempt, as passed to [`Progress::record_grade`].
pub(crate) struct GradeRecord<'a> {
    pub(crate) track: TrackId,
    pub(crate) lesson: &'a str,
    pub(crate) score: &'a SessionScore,
    pub(crate) hints: u8,
    pub(crate) passed: bool,
    pub(crate) today: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(total: u8) -> SessionScore {
        SessionScore {
            date: "2026-11-20".into(),
            track: "python".into(),
            lesson: "py-01-basics".into(),
            total,
            code: f32::from(total),
            autonomy: 100.0,
            git: None,
            english: None,
        }
    }

    fn grade(p: &mut Progress, track: TrackId, lesson: &str, total: u8, day: &str, now: u64) {
        let s = score(total);
        p.record_grade(
            &GradeRecord {
                track,
                lesson,
                score: &s,
                hints: 1,
                passed: total >= 70,
                today: day,
            },
            now,
        );
    }

    #[test]
    fn streak_grows_on_consecutive_days_and_resets_after_a_gap() {
        let mut p = Progress::default();
        for (i, day) in ["2026-11-28", "2026-11-29", "2026-11-30", "2026-12-01"]
            .iter()
            .enumerate()
        {
            p.record_study_day(day, i as u64);
        }
        assert_eq!((p.streak_days, p.longest_streak), (4, 4));
        p.record_study_day("2026-12-01", 5);
        assert_eq!(p.streak_days, 4, "same day counts once");
        p.record_study_day("2026-12-05", 6);
        assert_eq!((p.streak_days, p.longest_streak), (1, 4));
    }

    #[test]
    fn dates_handle_month_and_year_boundaries() {
        assert_eq!(add_days("2026-12-31", 1), "2027-01-01");
        assert_eq!(add_days("2028-02-28", 1), "2028-02-29");
        assert_eq!(add_days("not a date", 3), "not a date");
        assert_eq!(today().len(), 10);
    }

    #[test]
    fn grading_tracks_attempts_best_score_reviews_and_xp() {
        let mut p = Progress::default();
        grade(
            &mut p,
            TrackId::Python,
            "py-01-basics",
            50,
            "2026-11-20",
            10,
        );
        let l = p.lesson(TrackId::Python, "py-01-basics").unwrap();
        assert_eq!(
            (l.attempts, l.best_score, l.status),
            (1, 50, Status::InProgress)
        );
        assert_eq!(p.xp_total, 0);
        let due = p.due_review(TrackId::Python, "2026-11-21").unwrap();
        assert_eq!(due.concept, "variables");
        assert!(p.due_review(TrackId::Python, "2026-11-20").is_none());
        assert_eq!(p.weak_topics(TrackId::Python), vec!["variables"]);

        grade(
            &mut p,
            TrackId::Python,
            "py-01-basics",
            85,
            "2026-11-21",
            20,
        );
        let l = p.lesson(TrackId::Python, "py-01-basics").unwrap();
        assert_eq!(
            (l.attempts, l.best_score, l.status, l.hints_used),
            (2, 85, Status::Completed, 2)
        );
        assert_eq!(l.completed_at.as_deref(), Some("2026-11-21"));
        assert_eq!(p.xp_total, 85);
        assert!(p.review_queue.is_empty());
        assert_eq!(p.next_lesson(TrackId::Python), 1);
        assert_eq!(p.average_score(TrackId::Python), Some(85.0));
        assert_eq!(p.sessions.len(), 2);
        assert_eq!(p.streak_days, 2);

        grade(
            &mut p,
            TrackId::Python,
            "py-01-basics",
            95,
            "2026-11-21",
            30,
        );
        assert_eq!(p.xp_total, 85, "xp is earned once per lesson");
    }

    #[test]
    fn reviews_double_their_interval() {
        let mut p = Progress::default();
        grade(&mut p, TrackId::Sql, "sql-01-select", 10, "2026-11-20", 1);
        p.reviewed(TrackId::Sql, "sql-01-select", "2026-11-21", 2);
        let card = &p.review_queue[0];
        assert_eq!(
            (card.interval_days, card.next_review_at.as_str()),
            (2, "2026-11-23")
        );
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("student/progress.json");
        let mut p = Progress {
            active_track: Some(TrackId::TypeScript),
            ..Default::default()
        };
        grade(
            &mut p,
            TrackId::TypeScript,
            "ts-01-types",
            90,
            "2026-11-20",
            5,
        );
        p.save(&path).unwrap();
        assert_eq!(Progress::load(&path), p);
        assert_eq!(
            Progress::load(&dir.path().join("missing.json")),
            Progress::default()
        );
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Progress::load(&path), Progress::default());
    }

    #[test]
    fn coaching_defaults_on_for_old_files() {
        let p: Progress = serde_json::from_str("{\"version\":1}").unwrap();
        assert!(p.coaching.professional_git && p.coaching.technical_english);
    }

    #[test]
    fn merge_keeps_the_best_of_both_copies() {
        let mut local = Progress::default();
        grade(
            &mut local,
            TrackId::Python,
            "py-01-basics",
            60,
            "2026-11-20",
            100,
        );
        grade(
            &mut local,
            TrackId::Python,
            "py-02-collections",
            40,
            "2026-11-20",
            100,
        );

        let mut cloud = Progress::default();
        grade(
            &mut cloud,
            TrackId::Python,
            "py-01-basics",
            90,
            "2026-11-19",
            50,
        );
        grade(
            &mut cloud,
            TrackId::Sql,
            "sql-01-select",
            75,
            "2026-11-19",
            50,
        );
        cloud.longest_streak = 9;
        cloud.xp_total = 500;

        local.merge(&cloud);
        let py1 = local.lesson(TrackId::Python, "py-01-basics").unwrap();
        assert_eq!((py1.best_score, py1.status), (90, Status::Completed));
        assert_eq!(py1.completed_at.as_deref(), Some("2026-11-19"));
        assert!(local.lesson(TrackId::Sql, "sql-01-select").is_some());
        assert!(local.lesson(TrackId::Python, "py-02-collections").is_some());
        assert_eq!((local.longest_streak, local.xp_total), (9, 500));
        // The local profile was newer, so its study date survives.
        assert_eq!(local.last_study_date.as_deref(), Some("2026-11-20"));
        // py-01 is completed on the cloud side, so its review card is gone.
        assert!(
            local
                .review_queue
                .iter()
                .all(|c| c.lesson != "py-01-basics")
        );
        assert!(
            local
                .review_queue
                .iter()
                .any(|c| c.lesson == "py-02-collections")
        );
    }

    #[test]
    fn merge_takes_newer_profile_and_is_idempotent() {
        let mut local = Progress::default();
        local.record_study_day("2026-11-01", 10);
        let mut cloud = Progress::default();
        cloud.record_study_day("2026-11-25", 20);
        cloud.active_track = Some(TrackId::Sql);
        local.merge(&cloud);
        assert_eq!(local.last_study_date.as_deref(), Some("2026-11-25"));
        assert_eq!(local.active_track, Some(TrackId::Sql));
        let once = local.clone();
        local.merge(&cloud);
        assert_eq!(local, once);
    }
}
