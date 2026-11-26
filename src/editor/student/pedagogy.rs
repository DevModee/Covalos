//! The tutor's state machine and prompts. The machine, not the model, decides the phase,
//! the hint level and when to step down to a bridge exercise; the model only writes the
//! words for the turn it is asked for. Replies are checked so the exercise solution never
//! reaches the student even if the model ignores its instructions.
use super::curriculum::{Exercise, Lesson, TrackId};
use super::rubric::PASS_MARK;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Two short questions to place a student who is new to the track.
    Diagnostic {
        answered: u8,
    },
    /// One 60-second challenge on a concept from an earlier session.
    Review,
    /// Problem, analogy, a different micro-example and a micro-challenge.
    Explain,
    Practice,
    Complete,
}

/// What the next tutor reply must do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Directive {
    Diagnose { question: u8 },
    Review { concept: String },
    Explain,
    Answer,
    Hint { level: u8 },
    RefuseSolution,
    Feedback { total: u8, passed: bool },
    Bridge,
    Celebrate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Passed,
    Retry,
    /// Two scores under the pass mark in a row: switch to the smaller exercise.
    Bridge,
    /// The bridge is done; back to the lesson exercise.
    BridgeCleared,
}

#[derive(Clone, Debug)]
pub(crate) struct Tutor {
    pub(crate) track: TrackId,
    pub(crate) lesson: usize,
    pub(crate) phase: Phase,
    pub(crate) hints: u8,
    pub(crate) low_streak: u8,
    pub(crate) on_bridge: bool,
    review: Option<String>,
}

impl Tutor {
    /// `first_time` starts with the diagnostic; otherwise a due review comes first.
    pub(crate) fn start(
        track: TrackId,
        lesson: usize,
        first_time: bool,
        review: Option<String>,
    ) -> Self {
        let count = track.lessons().len();
        let lesson = lesson.min(count);
        let phase = if lesson >= count {
            Phase::Complete
        } else if first_time {
            Phase::Diagnostic { answered: 0 }
        } else if review.is_some() {
            Phase::Review
        } else {
            Phase::Explain
        };
        Self {
            track,
            lesson,
            phase,
            hints: 0,
            low_streak: 0,
            on_bridge: false,
            review,
        }
    }

    pub(crate) fn current_lesson(&self) -> Option<&'static Lesson> {
        self.track.lessons().get(self.lesson)
    }

    pub(crate) fn current_exercise(&self) -> Option<&'static Exercise> {
        self.current_lesson().map(|l| {
            if self.on_bridge {
                &l.bridge
            } else {
                &l.exercise
            }
        })
    }

    /// The directive for the opening turn of a session.
    pub(crate) fn opening(&self) -> Directive {
        match self.phase {
            Phase::Diagnostic { answered } => Directive::Diagnose {
                question: answered + 1,
            },
            Phase::Review => Directive::Review {
                concept: self.review.clone().unwrap_or_default(),
            },
            Phase::Explain => Directive::Explain,
            Phase::Practice => Directive::Answer,
            Phase::Complete => Directive::Celebrate,
        }
    }

    pub(crate) fn on_student_message(&mut self, text: &str) -> Directive {
        match self.phase {
            Phase::Diagnostic { answered } => {
                let answered = answered + 1;
                if answered >= 2 {
                    self.phase = Phase::Explain;
                    Directive::Explain
                } else {
                    self.phase = Phase::Diagnostic { answered };
                    Directive::Diagnose {
                        question: answered + 1,
                    }
                }
            }
            Phase::Review => {
                self.review = None;
                self.phase = Phase::Explain;
                Directive::Explain
            }
            _ if asks_for_solution(text) => Directive::RefuseSolution,
            Phase::Complete => Directive::Celebrate,
            Phase::Explain | Phase::Practice => Directive::Answer,
        }
    }

    /// The explanation was delivered; returns true when the exercise should open.
    pub(crate) fn on_reply(&mut self) -> bool {
        if self.phase == Phase::Explain {
            self.phase = Phase::Practice;
            return true;
        }
        false
    }

    /// Next rung of the ladder, capped at three.
    pub(crate) fn on_hint(&mut self) -> Directive {
        self.hints = (self.hints + 1).min(3);
        Directive::Hint { level: self.hints }
    }

    pub(crate) fn on_graded(&mut self, total: u8) -> Outcome {
        if total >= PASS_MARK {
            self.low_streak = 0;
            self.hints = 0;
            if self.on_bridge {
                self.on_bridge = false;
                return Outcome::BridgeCleared;
            }
            self.lesson += 1;
            self.phase = if self.lesson >= self.track.lessons().len() {
                Phase::Complete
            } else {
                Phase::Explain
            };
            return Outcome::Passed;
        }
        self.low_streak += 1;
        if self.low_streak >= 2 && !self.on_bridge {
            self.on_bridge = true;
            self.low_streak = 0;
            self.hints = 0;
            return Outcome::Bridge;
        }
        Outcome::Retry
    }

    pub(crate) fn directive_after(&self, outcome: Outcome, total: u8) -> Directive {
        match outcome {
            Outcome::Bridge => Directive::Bridge,
            Outcome::Passed if self.phase == Phase::Complete => Directive::Celebrate,
            _ => Directive::Feedback {
                total,
                passed: total >= PASS_MARK,
            },
        }
    }
}

/// Requests to hand over the answer, in Spanish and English.
pub(crate) fn asks_for_solution(text: &str) -> bool {
    let t = text.to_lowercase();
    [
        "hacelo vos",
        "hazlo tu",
        "hazlo tú",
        "dame la solucion",
        "dame la solución",
        "pasame el codigo",
        "pasame el código",
        "pásame el código",
        "escribilo vos",
        "resolvelo",
        "resuélvelo",
        "do it for me",
        "write it for me",
        "give me the solution",
        "give me the answer",
        "just give me the code",
        "solve it",
    ]
    .iter()
    .any(|p| t.contains(p))
}

/// Settings the student can change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Coaching {
    pub(crate) professional_git: bool,
    pub(crate) technical_english: bool,
    pub(crate) spanish: bool,
}

pub(crate) fn system_prompt(coaching: Coaching) -> String {
    let language = if coaching.spanish {
        "Write to the student in neutral Spanish. Code, identifiers, commit messages and error messages stay in English."
    } else {
        "Write to the student in plain English."
    };
    let mut prompt = format!(
        "You are the tutor in Covalos Student Mode. Your only goal is that the student learns to think and write code on their own, starting from zero.\n\
         Rules you never break:\n\
         1. Never write the solution to the current exercise or code that can be pasted into it. If asked to do it for them, say no kindly and offer a smaller step.\n\
         2. Keep every reply to one short screen. No walls of text.\n\
         3. A new concept always follows this order: what problem it solves (1-2 sentences), an everyday analogy, a micro-example of 3-6 lines about a different case than the exercise, then one micro-challenge.\n\
         4. Hints climb a ladder. Level 1: describe what happens in their code without naming the line or the syntax. Level 2: name the concept or line and ask a guiding question. Level 3: a 2-line analogous example, never the literal answer.\n\
         5. Do not edit, create or run files. The student types the code; Covalos runs the tests and computes scores.\n\
         6. {language}\n"
    );
    if coaching.professional_git {
        prompt.push_str(
            "Professional practice: before each exercise the student works on a branch like learn/day-01-variables, never main. They commit after each logical step that passes tests, with an English imperative subject such as `Add input validation for user age`. Covalos checks branch and commit messages; explain its notes briefly when they appear. Projects use src/, tests/, .gitignore, a short README.md, configuration in .env and explicit error handling.\n",
        );
    }
    if coaching.technical_english {
        prompt.push_str(
            "Technical English: when a name, comment, error message or description is in Spanish or Spanglish, show the idiomatic form an engineering team would use and say why in one sentence.\n",
        );
    }
    prompt
}

/// Everything a turn needs to know about the lesson; built by the caller.
pub(crate) struct TurnContext<'a> {
    pub(crate) lesson: &'a Lesson,
    pub(crate) exercise: &'a Exercise,
    pub(crate) student_code: Option<&'a str>,
    /// Failing tests and practice notes from the last run, if any.
    pub(crate) results: Option<&'a str>,
    pub(crate) student_text: Option<&'a str>,
}

pub(crate) fn turn_prompt(directive: &Directive, ctx: &TurnContext<'_>, fence: &str) -> String {
    let lesson = ctx.lesson;
    let task = match directive {
        Directive::Diagnose { question } => format!(
            "Diagnostic question {question} of 2 for the lesson \"{}\". Ask one short question or micro-challenge to find out whether the student already knows {}. Do not teach yet.",
            lesson.title,
            lesson.concepts.join(", ")
        ),
        Directive::Review { concept } => format!(
            "Start with a 60-second review challenge about \"{concept}\" from an earlier session: one question or a 3-line snippet to predict. Wait for the answer."
        ),
        Directive::Explain => format!(
            "Teach the lesson \"{}\" (concepts: {}) using the required order: problem, analogy, a micro-example about a different case, micro-challenge. Then say the exercise is open in the editor and that they should create the branch `{}` first.",
            lesson.title,
            lesson.concepts.join(", "),
            lesson.branch
        ),
        Directive::Answer => "Answer the student's message briefly. If it is about the exercise, guide with a question instead of giving code.".to_owned(),
        Directive::Hint { level } => format!(
            "The student asked for help. Give a level {level} hint exactly as the hint ladder defines it."
        ),
        Directive::RefuseSolution => "The student asked you to solve it. Decline kindly in one sentence, then offer a smaller first step they can do in under two minutes.".to_owned(),
        Directive::Feedback { total, passed: true } => format!(
            "Covalos graded the exercise {total}/100 and it passed. Congratulate briefly, mention one thing to improve from the notes, and remind them to commit with a good message."
        ),
        Directive::Feedback { total, passed: false } => format!(
            "Covalos graded the exercise {total}/100, under the pass mark of {PASS_MARK}. Explain the most important failing test in plain words and ask a guiding question. No code from the solution."
        ),
        Directive::Bridge => "The student scored under 70 twice. A simpler bridge exercise is now open. Encourage them in one sentence and explain the smaller goal.".to_owned(),
        Directive::Celebrate => "The track is complete. Summarise in three short lines what they can now do and suggest a small project to try alone.".to_owned(),
    };
    let mut out = format!(
        "[Covalos Student Mode turn]\nTask: {task}\nExercise ({}): {}\n",
        ctx.exercise.file, ctx.exercise.brief
    );
    if let Some(code) = ctx.student_code.filter(|c| !c.trim().is_empty()) {
        let code: String = code.chars().take(4000).collect();
        out.push_str(&format!("Student code:\n```{fence}\n{code}\n```\n"));
    }
    if let Some(results) = ctx.results.filter(|r| !r.trim().is_empty()) {
        out.push_str(&format!("Latest results:\n{results}\n"));
    }
    if let Some(text) = ctx.student_text {
        out.push_str(&format!("Student says: {text}\n"));
    }
    out
}

const HIDDEN: &str = "(Code hidden: it was too close to the exercise solution.)\n";

/// Replaces code blocks that contain a guarded definition with a short notice. Returns
/// the cleaned reply and whether anything was hidden.
pub(crate) fn guard_reply(reply: &str, exercise: &Exercise) -> (String, bool) {
    let mut out = String::with_capacity(reply.len());
    let mut hidden = false;
    let mut block: Option<String> = None;
    let mut flush = |code: String, out: &mut String| {
        if leaks(&code, exercise) {
            hidden = true;
            out.push_str(HIDDEN);
        } else {
            out.push_str(&code);
        }
    };
    for line in reply.split_inclusive('\n') {
        let fence = line.trim_start().starts_with("```");
        match block.as_mut() {
            None if fence => block = Some(line.to_owned()),
            None => out.push_str(line),
            Some(code) => {
                code.push_str(line);
                if fence {
                    flush(block.take().unwrap_or_default(), &mut out);
                }
            }
        }
    }
    if let Some(code) = block {
        flush(code, &mut out);
    }
    (out, hidden)
}

fn leaks(code: &str, exercise: &Exercise) -> bool {
    let lower = code.to_lowercase();
    let body_lines = code
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("```"))
        .count();
    body_lines > 2
        && exercise
            .guarded
            .iter()
            .any(|g| lower.contains(&g.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn python() -> Tutor {
        Tutor::start(TrackId::Python, 0, false, None)
    }

    #[test]
    fn new_students_get_two_diagnostic_questions_then_the_lesson() {
        let mut t = Tutor::start(TrackId::Python, 0, true, None);
        assert_eq!(t.opening(), Directive::Diagnose { question: 1 });
        assert_eq!(
            t.on_student_message("no idea"),
            Directive::Diagnose { question: 2 }
        );
        assert_eq!(t.on_student_message("a box?"), Directive::Explain);
        assert_eq!(t.phase, Phase::Explain);
        assert!(t.on_reply());
        assert_eq!(t.phase, Phase::Practice);
        assert!(!t.on_reply());
    }

    #[test]
    fn returning_students_start_with_a_review() {
        let mut t = Tutor::start(TrackId::Python, 1, false, Some("if / elif".into()));
        assert_eq!(
            t.opening(),
            Directive::Review {
                concept: "if / elif".into()
            }
        );
        assert_eq!(t.on_student_message("teen"), Directive::Explain);
        assert_eq!(python().opening(), Directive::Explain);
    }

    #[test]
    fn hint_ladder_climbs_and_caps_at_three() {
        let mut t = python();
        let levels: Vec<_> = (0..5).map(|_| t.on_hint()).collect();
        assert_eq!(
            levels,
            [1, 2, 3, 3, 3].map(|level| Directive::Hint { level })
        );
    }

    #[test]
    fn asking_for_the_solution_is_refused() {
        let mut t = python();
        t.phase = Phase::Practice;
        assert_eq!(
            t.on_student_message("Hacelo vos porfa"),
            Directive::RefuseSolution
        );
        assert_eq!(
            t.on_student_message("just give me the code"),
            Directive::RefuseSolution
        );
        assert_eq!(t.on_student_message("what is elif?"), Directive::Answer);
    }

    #[test]
    fn passing_moves_to_the_next_lesson_and_resets_hints() {
        let mut t = python();
        t.phase = Phase::Practice;
        t.on_hint();
        assert_eq!(t.on_graded(70), Outcome::Passed);
        assert_eq!((t.lesson, t.hints, t.phase), (1, 0, Phase::Explain));
    }

    #[test]
    fn two_low_scores_in_a_row_open_the_bridge() {
        let mut t = python();
        t.phase = Phase::Practice;
        assert_eq!(t.on_graded(50), Outcome::Retry);
        assert_eq!(t.on_graded(69), Outcome::Bridge);
        assert!(t.on_bridge);
        assert_eq!(t.current_exercise().unwrap().file, "src/day01/adult.py");
        assert_eq!(t.directive_after(Outcome::Bridge, 69), Directive::Bridge);
        assert_eq!(t.on_graded(90), Outcome::BridgeCleared);
        assert!(!t.on_bridge);
        assert_eq!(t.lesson, 0, "the lesson exercise still has to pass");
        assert_eq!(t.current_exercise().unwrap().file, "src/day01/age.py");
    }

    #[test]
    fn a_pass_between_low_scores_resets_the_streak() {
        let mut t = python();
        assert_eq!(t.on_graded(40), Outcome::Retry);
        assert_eq!(t.on_graded(80), Outcome::Passed);
        assert_eq!(t.on_graded(40), Outcome::Retry);
        assert!(!t.on_bridge);
    }

    #[test]
    fn finishing_the_last_lesson_completes_the_track() {
        let last = TrackId::Sql.lessons().len() - 1;
        let mut t = Tutor::start(TrackId::Sql, last, false, None);
        assert_eq!(t.on_graded(100), Outcome::Passed);
        assert_eq!(t.phase, Phase::Complete);
        assert_eq!(
            t.directive_after(Outcome::Passed, 100),
            Directive::Celebrate
        );
        assert!(t.current_exercise().is_none());
        assert_eq!(
            Tutor::start(TrackId::Sql, 99, false, None).phase,
            Phase::Complete
        );
    }

    #[test]
    fn system_prompt_carries_the_rules_and_toggles() {
        let prompt = system_prompt(Coaching {
            professional_git: true,
            technical_english: true,
            spanish: true,
        });
        assert!(prompt.contains("Never write the solution"));
        assert!(prompt.contains("Level 3"));
        assert!(prompt.contains("micro-example"));
        assert!(prompt.contains("learn/day-01-variables"));
        assert!(prompt.contains("Technical English"));
        assert!(prompt.contains("Spanish"));
        let plain = system_prompt(Coaching {
            professional_git: false,
            technical_english: false,
            spanish: false,
        });
        assert!(!plain.contains("branch"));
        assert!(!plain.contains("Technical English"));
    }

    #[test]
    fn turn_prompt_includes_code_and_results() {
        let lesson = &TrackId::Python.lessons()[0];
        let ctx = TurnContext {
            lesson,
            exercise: &lesson.exercise,
            student_code: Some("def classify_age(age):\n    return 1\n"),
            results: Some("FAIL child under 13"),
            student_text: Some("why?"),
        };
        let p = turn_prompt(&Directive::Hint { level: 2 }, &ctx, "python");
        assert!(p.contains("level 2 hint"));
        assert!(p.contains("```python\ndef classify_age"));
        assert!(p.contains("FAIL child under 13"));
        assert!(p.contains("Student says: why?"));
    }

    #[test]
    fn guard_hides_solution_blocks_but_keeps_other_examples() {
        let ex = &TrackId::Python.lessons()[0].exercise;
        let leak = "Try this:\n```python\ndef classify_age(age):\n    if age < 0:\n        return \"invalid\"\n```\nGood luck";
        let (clean, hidden) = guard_reply(leak, ex);
        assert!(hidden);
        assert!(!clean.contains("def classify_age"));
        assert!(clean.starts_with("Try this:\n") && clean.ends_with("Good luck"));

        let other = "Example:\n```python\ndef is_cold(temp):\n    return temp < 10\n    # different case\n```\n";
        assert_eq!(guard_reply(other, ex), (other.to_owned(), false));

        let unterminated = "```python\ndef classify_age(age):\n    x = 1\n    return x\n";
        assert!(guard_reply(unterminated, ex).1);
    }
}
