//! 0-100 grading: correctness from real test output (50), autonomy from hints used (30)
//! and code clarity from simple static checks (20). Pass mark is 70.
use super::curriculum::Lang;
use serde::{Deserialize, Serialize};

pub(crate) const PASS_MARK: u8 = 70;

/// What the test harness reported.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TestReport {
    pub(crate) passed: u32,
    pub(crate) total: u32,
    /// Failure lines, kept short for the result card and the tutor.
    pub(crate) failures: Vec<String>,
}

/// Reads `TOTAL n`, `PASS name` and `FAIL name: detail` lines. A missing `TOTAL`
/// falls back to the number of result lines seen; a crash before any line scores zero.
pub(crate) fn parse_report(output: &str) -> TestReport {
    let mut report = TestReport::default();
    let mut declared = None;
    let mut seen = 0;
    for line in output.lines().map(str::trim) {
        if let Some(n) = line.strip_prefix("TOTAL ") {
            declared = n.trim().parse().ok();
        } else if line.starts_with("PASS ") {
            report.passed += 1;
            seen += 1;
        } else if let Some(rest) = line.strip_prefix("FAIL ") {
            seen += 1;
            if report.failures.len() < 8 {
                report.failures.push(rest.chars().take(200).collect());
            }
        }
    }
    report.total = declared.unwrap_or(seen).max(seen);
    report
}

pub(crate) fn correctness_points(report: &TestReport) -> u8 {
    if report.total == 0 {
        return 0;
    }
    ((report.passed.min(report.total) as f32 / report.total as f32) * 50.0).round() as u8
}

pub(crate) fn autonomy_points(hints: u8) -> u8 {
    match hints {
        0 => 30,
        1 => 22,
        2 => 14,
        _ => 8,
    }
}

/// Starts at 20 and removes points for habits a reviewer would flag. Returns the notes
/// so the student sees why.
pub(crate) fn clarity_points(source: &str, lang: Lang) -> (u8, Vec<String>) {
    let mut points: i32 = 20;
    let mut notes = Vec::new();
    let comment = match lang {
        Lang::Python => "#",
        Lang::TypeScript => "//",
        Lang::Sql => "--",
    };
    let short = short_names(source, lang);
    if !short.is_empty() {
        points -= 5;
        notes.push(format!(
            "Use descriptive names instead of {}.",
            short.join(", ")
        ));
    }
    let dead = source
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with(comment) && looks_like_code(&l[comment.len()..]))
        .count();
    if dead > 0 {
        points -= 5;
        notes.push("Remove commented-out code; git keeps the history.".into());
    }
    if source.lines().any(|l| l.chars().count() > 100) {
        points -= 3;
        notes.push("Keep lines under 100 characters.".into());
    }
    let lower = source.to_lowercase();
    if lower.contains("todo") || lower.contains("hack") || lower.contains("fixme") {
        points -= 3;
        notes.push("Resolve TODO or hack markers before submitting.".into());
    }
    if lang == Lang::Python && source.lines().any(|l| l.trim() == "pass") {
        points -= 4;
        notes.push("A leftover `pass` means part of the code is still empty.".into());
    }
    (points.clamp(0, 20) as u8, notes)
}

fn looks_like_code(text: &str) -> bool {
    let t = text.trim();
    [
        "=", "(", ")", "return ", "def ", "const ", "let ", "SELECT", "print",
    ]
    .iter()
    .any(|m| t.contains(m))
        && !t.ends_with('.')
}

/// Single-letter names other than the usual loop counters.
fn short_names(source: &str, lang: Lang) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in source.lines() {
        let t = line.trim();
        if t.starts_with(['#', '/', '-']) {
            continue;
        }
        let name = match lang {
            Lang::Python => t
                .split_once('=')
                .filter(|(l, r)| !l.contains(['(', '[', '.', '!', '<', '>']) && !r.starts_with('='))
                .map(|(l, _)| l.trim()),
            Lang::TypeScript => ["const ", "let "]
                .iter()
                .find_map(|k| t.strip_prefix(k))
                .and_then(|r| r.split([' ', '=', ':']).next()),
            Lang::Sql => None,
        };
        if let Some(name) = name
            && name.len() == 1
            && !["i", "j", "k", "_"].contains(&name)
        {
            let quoted = format!("`{name}`");
            if !out.contains(&quoted) {
                out.push(quoted);
            }
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Grade {
    pub(crate) correctness: u8,
    pub(crate) autonomy: u8,
    pub(crate) clarity: u8,
    pub(crate) notes: Vec<String>,
}

impl Grade {
    pub(crate) fn total(&self) -> u8 {
        self.correctness + self.autonomy + self.clarity
    }

    pub(crate) fn passed(&self) -> bool {
        self.total() >= PASS_MARK
    }

    /// Code area for the chart: correctness and clarity out of 70, scaled to 100.
    pub(crate) fn code_area(&self) -> f32 {
        f32::from(self.correctness + self.clarity) / 70.0 * 100.0
    }

    pub(crate) fn autonomy_area(&self) -> f32 {
        f32::from(self.autonomy) / 30.0 * 100.0
    }
}

pub(crate) fn grade(report: &TestReport, hints: u8, source: &str, lang: Lang) -> Grade {
    let correctness = correctness_points(report);
    let (mut clarity, notes) = clarity_points(source, lang);
    // Clarity is earned on working code; an empty or broken file cannot be clear.
    if report.passed == 0 {
        clarity = 0;
    }
    Grade {
        correctness,
        autonomy: autonomy_points(hints),
        clarity,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(passed: u32, total: u32) -> TestReport {
        TestReport {
            passed,
            total,
            failures: vec![],
        }
    }

    #[test]
    fn parses_harness_output() {
        let r = parse_report("TOTAL 3\nPASS a\nFAIL b: expected 1, got 2\nPASS c\n");
        assert_eq!((r.passed, r.total), (2, 3));
        assert_eq!(r.failures, vec!["b: expected 1, got 2".to_owned()]);
    }

    #[test]
    fn crash_after_total_counts_missing_tests_as_failed() {
        let r = parse_report("TOTAL 4\nFAIL load: SyntaxError\n");
        assert_eq!((r.passed, r.total), (0, 4));
        assert_eq!(correctness_points(&r), 0);
    }

    #[test]
    fn missing_total_uses_lines_seen() {
        let r = parse_report("PASS a\nPASS b\n");
        assert_eq!((r.passed, r.total), (2, 2));
        assert_eq!(correctness_points(&parse_report("")), 0);
    }

    #[test]
    fn correctness_is_proportional() {
        assert_eq!(correctness_points(&report(3, 4)), 38);
        assert_eq!(correctness_points(&report(5, 5)), 50);
    }

    #[test]
    fn autonomy_follows_the_hint_table() {
        assert_eq!([0, 1, 2, 3, 9].map(autonomy_points), [30, 22, 14, 8, 8]);
    }

    #[test]
    fn clean_code_keeps_full_clarity() {
        let src = "def classify_age(age):\n    if age < 0:\n        return \"invalid\"\n    return \"adult\"\n";
        assert_eq!(clarity_points(src, Lang::Python), (20, vec![]));
    }

    #[test]
    fn clarity_flags_short_names_dead_code_and_markers() {
        let src = "x = 3\n# y = x + 1\n# TODO tidy\nprint(x)\n";
        let (points, notes) = clarity_points(src, Lang::Python);
        assert_eq!(points, 20 - 5 - 5 - 3);
        assert_eq!(notes.len(), 3);
        let (ts, _) = clarity_points(
            "const n = 1;\nfor (let i = 0; i < n; i++) {}\n",
            Lang::TypeScript,
        );
        assert_eq!(ts, 15);
    }

    #[test]
    fn passing_and_failing_grades() {
        let g = grade(
            &report(5, 5),
            0,
            "def f(age):\n    return age\n",
            Lang::Python,
        );
        assert_eq!(g.total(), 100);
        assert!(g.passed());
        assert_eq!(g.code_area(), 100.0);

        let g = grade(
            &report(2, 4),
            3,
            "def f(age):\n    return age\n",
            Lang::Python,
        );
        assert_eq!(g.total(), 25 + 8 + 20);
        assert!(!g.passed());

        assert_eq!(grade(&report(0, 4), 0, "", Lang::Python).total(), 30);
    }

    #[test]
    fn pass_mark_is_inclusive() {
        let g = Grade {
            correctness: 40,
            autonomy: 22,
            clarity: 8,
            notes: vec![],
        };
        assert_eq!(g.total(), PASS_MARK);
        assert!(g.passed());
    }
}
