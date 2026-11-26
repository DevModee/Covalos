//! Student Mode controller and view: the lesson chat, the exercise card with Run & Grade,
//! branch and commit steps, the tips popout, the progress chart and the voice composer.
//! Everything outside this module goes through [`StudentWorkspaceHost`].
use super::chart::{self, ChartAxis};
use super::curriculum::{Lang, TrackId};
use super::host::{StudentWorkspaceHost, TutorAccount, TutorAccounts, TutorPoll, TutorTicket};
use super::pedagogy::{self, Coaching, Directive, Outcome, Phase, TurnContext, Tutor};
use super::practice::{self, Check};
use super::progress::{self, GradeRecord, Progress, SessionScore};
use super::rubric::{self, Grade, TestReport};
use super::runner::{self, RunOutput};
use super::voice::{self, MicLevel, VoiceBand};
use crate::editor::dot_loader::{LoaderPattern, dot_glyph, dot_loader};
use crate::editor::i18n::tr;
use crate::editor::theme::{BODY, HEADING, Icon, SMALL, colors, draw_icon, text_field};
use eframe::egui::{self, RichText, Ui};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

const COLUMN: f32 = 760.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Who {
    Tutor,
    Student,
    Note,
}

#[derive(Clone, Debug)]
struct Message {
    who: Who,
    text: String,
}

struct Exercise {
    root: PathBuf,
    path: PathBuf,
    lang: Lang,
    test_file: &'static str,
    brief: &'static str,
    file: &'static str,
    branch: &'static str,
}

struct RunCard {
    grade: Grade,
    report: TestReport,
    timed_out: bool,
    stderr: String,
    english: Option<Check>,
    git: Option<Check>,
    outcome: Outcome,
}

#[derive(Default)]
struct TipPopout {
    open: bool,
    conversation: Option<String>,
    pending: Option<TutorTicket>,
    messages: Vec<Message>,
    draft: String,
}

#[derive(Default)]
pub(crate) struct StudentMode {
    pub(crate) open: bool,
    loaded: bool,
    progress: Progress,
    progress_path: Option<PathBuf>,
    config: Option<PathBuf>,
    tutor: Option<Tutor>,
    review_lesson: Option<String>,
    messages: Vec<Message>,
    conversation: Option<String>,
    pending: Option<TutorTicket>,
    draft: String,
    exercise: Option<Exercise>,
    branch: Option<String>,
    branch_draft: String,
    commit_draft: String,
    commit_score: Option<u8>,
    run: Option<Receiver<Result<RunOutput, String>>>,
    result: Option<RunCard>,
    tip: TipPopout,
    mic: Option<MicLevel>,
    band: Option<VoiceBand>,
    show_progress: bool,
    chart_selected: Option<usize>,
    sync: Option<Receiver<Result<Progress, String>>>,
    synced_once: bool,
    status: Option<(bool, String)>,
    /// Tutor account picked by the student, by account id.
    tutor_account: Option<String>,
    /// Prompts held while the AI accounts finish their sign-in check.
    queued: Vec<Queued>,
}

struct Queued {
    prompt: String,
    root: PathBuf,
    tip: bool,
}

impl StudentMode {
    fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.config = crate::paths::config_dir();
        self.progress_path = self
            .config
            .as_ref()
            .map(|c| c.join("student").join("progress.json"));
        if let Some(path) = &self.progress_path {
            self.progress = Progress::load(path);
        }
    }

    fn save(&mut self) {
        if let Some(path) = &self.progress_path
            && let Err(e) = self.progress.save(path)
        {
            self.status = Some((true, format!("{} {e}", tr("Could not save progress."))));
        }
    }

    fn coaching(&self, host: &dyn StudentWorkspaceHost) -> Coaching {
        Coaching {
            professional_git: self.progress.coaching.professional_git,
            technical_english: self.progress.coaching.technical_english,
            spanish: host.spanish(),
        }
    }

    fn track(&self) -> Option<TrackId> {
        self.tutor.as_ref().map(|t| t.track)
    }

    /// Starts (or restarts) a session on a track: diagnostic for new tracks, a review
    /// card when one is due, otherwise the next lesson.
    fn start_track(&mut self, track: TrackId, host: &mut dyn StudentWorkspaceHost) {
        let today = progress::today();
        let review = self
            .progress
            .due_review(track, &today)
            .map(|c| (c.lesson.clone(), c.concept.clone()));
        let first_time = !self.progress.started(track);
        let lesson = self.progress.next_lesson(track);
        let tutor = Tutor::start(
            track,
            lesson,
            first_time,
            review.as_ref().map(|r| r.1.clone()),
        );
        self.review_lesson = review.map(|r| r.0);
        self.progress.active_track = Some(track);
        self.progress.updated_at_ms = progress::now_ms();
        if let Some(l) = tutor.current_lesson() {
            self.progress.start_lesson(track, l.id, progress::now_ms());
        }
        self.save();
        self.messages.clear();
        self.conversation = None;
        self.pending = None;
        self.exercise = None;
        self.result = None;
        self.tip = TipPopout::default();
        let opening = tutor.opening();
        self.tutor = Some(tutor);
        self.ask(host, &opening, None);
    }

    /// Sends one tutor turn. The system prompt rides along with the first message of a
    /// conversation; the agent session keeps it from then on.
    fn ask(
        &mut self,
        host: &mut dyn StudentWorkspaceHost,
        directive: &Directive,
        said: Option<&str>,
    ) {
        let Some(tutor) = &self.tutor else { return };
        let track = tutor.track;
        let Some(lesson) = tutor.current_lesson().or(track.lessons().last()) else {
            return;
        };
        let exercise = tutor.current_exercise().unwrap_or(&lesson.exercise);
        let code = self
            .exercise
            .as_ref()
            .and_then(|e| host.buffer_text(&e.path));
        let results = self.result.as_ref().map(summary);
        let ctx = TurnContext {
            lesson,
            exercise,
            student_code: code.as_deref(),
            results: results.as_deref(),
            student_text: said,
        };
        let mut prompt = pedagogy::turn_prompt(directive, &ctx, track.language().fence());
        if self.conversation.is_none() {
            prompt = format!("{}\n{prompt}", pedagogy::system_prompt(self.coaching(host)));
        }
        let Some(root) = self.workspace(track) else {
            return;
        };
        self.dispatch(
            host,
            Queued {
                prompt,
                root,
                tip: false,
            },
        );
    }

    /// The account the tutor runs on: the student's pick while it stays ready, else
    /// the first ready one (the account chosen in Agents comes first).
    fn pick_account(&mut self, ready: &[TutorAccount]) -> Option<String> {
        let keep = self
            .tutor_account
            .as_ref()
            .filter(|id| ready.iter().any(|a| &a.id == *id));
        let id = keep
            .cloned()
            .or_else(|| ready.first().map(|a| a.id.clone()))?;
        self.tutor_account = Some(id.clone());
        Some(id)
    }

    /// Sends a prompt now, or keeps it until the AI accounts finish their sign-in check.
    fn dispatch(&mut self, host: &mut dyn StudentWorkspaceHost, queued: Queued) {
        let ready = match host.tutor_accounts() {
            TutorAccounts::Ready(ready) => ready,
            TutorAccounts::Checking => {
                self.queued.push(queued);
                return;
            }
            TutorAccounts::Missing => {
                let text = tr("Connect an AI in Agents to use the tutor.").to_owned();
                if queued.tip {
                    self.tip.messages.push(Message {
                        who: Who::Note,
                        text,
                    });
                } else {
                    self.note(text);
                }
                return;
            }
        };
        let Some(account) = self.pick_account(&ready) else {
            return;
        };
        let conversation = if queued.tip {
            self.tip.conversation.clone()
        } else {
            self.conversation.clone()
        };
        match host.ask_tutor(
            &account,
            conversation.as_deref(),
            &queued.prompt,
            &queued.root,
        ) {
            Ok(ticket) if queued.tip => {
                self.tip.conversation = Some(ticket.conversation.clone());
                self.tip.pending = Some(ticket);
            }
            Ok(ticket) => {
                self.conversation = Some(ticket.conversation.clone());
                self.pending = Some(ticket);
            }
            Err(e) if queued.tip => self.tip.messages.push(Message {
                who: Who::Note,
                text: e,
            }),
            Err(e) => self.note(format!("{} {e}", tr("The tutor is unavailable."))),
        }
    }

    /// Switches the tutor to another account; the next message starts a new session.
    fn choose_account(&mut self, id: String, label: &str) {
        if self.tutor_account.as_deref() == Some(id.as_str()) {
            return;
        }
        self.tutor_account = Some(id);
        self.conversation = None;
        self.tip.conversation = None;
        self.note(format!("{} {label}.", tr("The tutor now runs on")));
    }

    fn workspace(&mut self, track: TrackId) -> Option<PathBuf> {
        let root = runner::workspace_root(self.config.as_ref()?, track);
        if let Err(e) = runner::prepare_workspace(&root, track) {
            self.status = Some((true, e));
            return None;
        }
        Some(root)
    }

    fn note(&mut self, text: String) {
        self.messages.push(Message {
            who: Who::Note,
            text,
        });
    }

    fn open_exercise(&mut self, host: &mut dyn StudentWorkspaceHost) {
        let Some(tutor) = &self.tutor else { return };
        let (track, Some(lesson), Some(ex)) = (
            tutor.track,
            tutor.current_lesson(),
            tutor.current_exercise(),
        ) else {
            return;
        };
        let Some(root) = self.workspace(track) else {
            return;
        };
        match runner::write_exercise(&root, track.language(), ex) {
            Ok(path) => {
                host.open_exercise(&path);
                self.branch = runner::current_branch(&root);
                self.branch_draft = lesson.branch.to_owned();
                self.commit_draft.clear();
                self.commit_score = None;
                self.exercise = Some(Exercise {
                    root,
                    path,
                    lang: track.language(),
                    test_file: ex.test_file,
                    brief: ex.brief,
                    file: ex.file,
                    branch: lesson.branch,
                });
            }
            Err(e) => self.status = Some((true, e)),
        }
    }

    fn send_student_message(&mut self, host: &mut dyn StudentWorkspaceHost) {
        let text = self.draft.trim().to_owned();
        if text.is_empty() || self.pending.is_some() {
            return;
        }
        self.draft.clear();
        self.messages.push(Message {
            who: Who::Student,
            text: text.clone(),
        });
        let Some(tutor) = self.tutor.as_mut() else {
            return;
        };
        let was_review = tutor.phase == Phase::Review;
        let directive = tutor.on_student_message(&text);
        let track = tutor.track;
        if was_review && let Some(lesson) = self.review_lesson.take() {
            self.progress
                .reviewed(track, &lesson, &progress::today(), progress::now_ms());
            self.save();
        }
        self.ask(host, &directive, Some(&text));
    }

    fn ask_hint(&mut self, host: &mut dyn StudentWorkspaceHost) {
        if self.pending.is_some() {
            return;
        }
        let Some(tutor) = self.tutor.as_mut() else {
            return;
        };
        let directive = tutor.on_hint();
        let level = tutor.hints;
        self.messages.push(Message {
            who: Who::Student,
            text: format!("{} {level}/3", tr("Hint")),
        });
        self.ask(host, &directive, None);
    }

    fn run_and_grade(&mut self, host: &mut dyn StudentWorkspaceHost, ctx: &egui::Context) {
        let Some(ex) = &self.exercise else { return };
        if self.run.is_some() {
            return;
        }
        if let Err(e) = host.save_buffer(&ex.path) {
            self.status = Some((true, e));
            return;
        }
        self.run = Some(runner::spawn_tests(
            ex.root.clone(),
            ex.lang,
            ex.test_file,
            ctx.clone(),
        ));
    }

    fn finish_run(
        &mut self,
        output: Result<RunOutput, String>,
        host: &mut dyn StudentWorkspaceHost,
    ) {
        let output = match output {
            Ok(output) => output,
            Err(e) => {
                self.status = Some((true, e));
                return;
            }
        };
        let (Some(ex), Some(tutor)) = (&self.exercise, self.tutor.as_mut()) else {
            return;
        };
        let source = host.buffer_text(&ex.path).unwrap_or_default();
        let report = rubric::parse_report(&output.stdout);
        let hints = tutor.hints;
        let grade = rubric::grade(&report, hints, &source, ex.lang);
        let coaching = &self.progress.coaching;
        let english = (coaching.technical_english && ex.lang != Lang::Sql)
            .then(|| practice::check_identifiers(&source, ex.lang));
        // Git area: branch, project layout and, once there is one, the last commit message.
        let git = coaching.professional_git.then(|| {
            let mut check = practice::check_branch(self.branch.as_deref().unwrap_or(""));
            let layout = practice::check_structure(&ex.root);
            let mut scores = vec![check.score, layout.score];
            scores.extend(self.commit_score);
            check.notes.extend(layout.notes);
            let sum: u16 = scores.iter().map(|&s| u16::from(s)).sum();
            check.score = (sum / scores.len() as u16) as u8;
            check
        });
        let track = tutor.track;
        let lesson_id = tutor.current_lesson().map_or("", |l| l.id);
        let today = progress::today();
        let score = SessionScore {
            date: today.clone(),
            track: track.key().to_owned(),
            lesson: lesson_id.to_owned(),
            total: grade.total(),
            code: grade.code_area(),
            autonomy: grade.autonomy_area(),
            git: git.as_ref().map(|c| f32::from(c.score)),
            english: english.as_ref().map(|c| f32::from(c.score)),
        };
        // Bridge exercises are practice; only the lesson exercise completes a lesson.
        let on_bridge = tutor.on_bridge;
        let outcome = tutor.on_graded(grade.total());
        let directive = tutor.directive_after(outcome, grade.total());
        if !on_bridge {
            self.progress.record_grade(
                &GradeRecord {
                    track,
                    lesson: lesson_id,
                    score: &score,
                    hints,
                    passed: grade.passed(),
                    today: &today,
                },
                progress::now_ms(),
            );
            self.save();
            self.synced_once = false;
        }
        self.result = Some(RunCard {
            grade,
            report,
            timed_out: output.timed_out,
            stderr: output.stderr.chars().take(600).collect(),
            english,
            git,
            outcome,
        });
        match outcome {
            Outcome::Bridge | Outcome::BridgeCleared => self.open_exercise(host),
            Outcome::Passed => {
                if let Some(next) = self.tutor.as_ref().and_then(Tutor::current_lesson) {
                    self.progress
                        .start_lesson(track, next.id, progress::now_ms());
                    self.save();
                }
            }
            Outcome::Retry => {}
        }
        self.ask(host, &directive, None);
    }

    fn create_branch(&mut self) {
        let Some(ex) = &self.exercise else { return };
        let name = self.branch_draft.trim().to_owned();
        match runner::switch_branch(&ex.root, &name) {
            Ok(()) => {
                self.branch = runner::current_branch(&ex.root);
                self.status = Some((false, format!("{} {name}", tr("Switched to"))));
            }
            Err(e) => self.status = Some((true, e)),
        }
    }

    fn commit(&mut self, host: &mut dyn StudentWorkspaceHost) {
        let Some(ex) = &self.exercise else { return };
        let message = self.commit_draft.trim().to_owned();
        let check = practice::check_commit_message(&message);
        if let Err(e) = host.save_buffer(&ex.path) {
            self.status = Some((true, e));
            return;
        }
        if runner::changed_files(&ex.root) == 0 {
            self.status = Some((false, tr("Nothing to commit yet.").to_owned()));
            return;
        }
        match runner::commit(&ex.root, &message) {
            Ok(hash) => {
                self.commit_score = Some(check.score);
                self.commit_draft.clear();
                self.status = Some((false, format!("{} {hash}", tr("Committed"))));
            }
            Err(e) => self.status = Some((true, e)),
        }
    }

    fn ask_tip(&mut self, host: &mut dyn StudentWorkspaceHost, topic: String) {
        if self.tip.pending.is_some() {
            return;
        }
        let Some(track) = self.track() else { return };
        let Some(root) = self.workspace(track) else {
            return;
        };
        self.tip.open = true;
        self.tip.messages.push(Message {
            who: Who::Student,
            text: topic.clone(),
        });
        let brief = self.exercise.as_ref().map_or("", |e| e.brief);
        let results = self.result.as_ref().map(summary).unwrap_or_default();
        let mut prompt = format!(
            "[Covalos Student Mode tip]\nAnswer in at most five short lines. The same rules apply: no solution code for the exercise.\nExercise: {brief}\nLatest results:\n{results}\nQuestion: {topic}\n"
        );
        if self.tip.conversation.is_none() {
            prompt = format!("{}\n{prompt}", pedagogy::system_prompt(self.coaching(host)));
        }
        self.dispatch(
            host,
            Queued {
                prompt,
                root,
                tip: true,
            },
        );
    }

    /// Hides solution code before a reply reaches the screen.
    fn guarded(&self, text: String) -> String {
        match self.tutor.as_ref().and_then(Tutor::current_exercise) {
            Some(ex) => pedagogy::guard_reply(&text, ex).0,
            None => text,
        }
    }

    fn poll(&mut self, host: &mut dyn StudentWorkspaceHost) {
        // Prompts waiting for a sign-in check go out as soon as an account is ready.
        if !self.queued.is_empty() && host.tutor_accounts() != TutorAccounts::Checking {
            for queued in std::mem::take(&mut self.queued) {
                self.dispatch(host, queued);
            }
        }
        if let Some(ticket) = &self.pending {
            match host.tutor_reply(ticket) {
                TutorPoll::Pending => {}
                TutorPoll::Reply(text) => {
                    self.pending = None;
                    let text = self.guarded(text);
                    self.messages.push(Message {
                        who: Who::Tutor,
                        text,
                    });
                    if self.tutor.as_mut().is_some_and(Tutor::on_reply) {
                        self.open_exercise(host);
                    }
                }
                TutorPoll::Failed(e) => {
                    self.pending = None;
                    self.note(e);
                }
            }
        }
        if let Some(ticket) = &self.tip.pending {
            match host.tutor_reply(ticket) {
                TutorPoll::Pending => {}
                TutorPoll::Reply(text) => {
                    self.tip.pending = None;
                    let text = self.guarded(text);
                    self.tip.messages.push(Message {
                        who: Who::Tutor,
                        text,
                    });
                }
                TutorPoll::Failed(e) => {
                    self.tip.pending = None;
                    self.tip.messages.push(Message {
                        who: Who::Note,
                        text: e,
                    });
                }
            }
        }
        if let Some(rx) = &self.run {
            match rx.try_recv() {
                Ok(output) => {
                    self.run = None;
                    self.finish_run(output, host);
                }
                Err(TryRecvError::Disconnected) => self.run = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.sync {
            match rx.try_recv() {
                Ok(Ok(merged)) => {
                    self.sync = None;
                    // Merging again keeps anything recorded while the sync ran.
                    self.progress.merge(&merged);
                    self.save();
                    self.status = Some((false, tr("Progress synced.").to_owned()));
                }
                Ok(Err(e)) => {
                    self.sync = None;
                    self.status = Some((true, e));
                }
                Err(TryRecvError::Disconnected) => self.sync = None,
                Err(TryRecvError::Empty) => {}
            }
        }
    }

    /// Offline first: sync runs after the local save, in the background, and only when
    /// an account is signed in.
    fn maybe_sync(&mut self, host: &dyn StudentWorkspaceHost, ctx: &egui::Context) {
        if self.synced_once || self.sync.is_some() {
            return;
        }
        self.synced_once = true;
        let Some(cloud) = host.cloud() else { return };
        let local = self.progress.clone();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(super::sync::sync(
                &cloud.endpoint,
                &cloud.access,
                &cloud.user_id,
                &local,
            ));
            ctx.request_repaint();
        });
        self.sync = Some(rx);
    }

    fn toggle_mic(&mut self) {
        if self.mic.take().is_some() {
            return;
        }
        match MicLevel::start() {
            Ok(mic) => {
                self.mic = Some(mic);
                self.band.get_or_insert_with(|| VoiceBand::new(72));
            }
            Err(e) => self.status = Some((true, tr(&e).to_owned())),
        }
    }

    fn axes(&self) -> Vec<ChartAxis> {
        let track = self.track().or(self.progress.active_track);
        let sessions: Vec<&SessionScore> = self
            .progress
            .sessions
            .iter()
            .filter(|s| track.is_none_or(|t| s.track == t.key()))
            .collect();
        let series = |f: fn(&SessionScore) -> Option<f32>| -> Vec<f32> {
            sessions.iter().filter_map(|s| f(s)).collect()
        };
        let axis = |label: &str, color: egui::Color32, history: Vec<f32>| ChartAxis {
            label: tr(label).to_owned(),
            score: history.last().copied(),
            color,
            history,
        };
        vec![
            axis(
                "Code and correctness",
                egui::Color32::from_rgb(242, 153, 74),
                series(|s| Some(s.code)),
            ),
            axis(
                "Autonomy",
                egui::Color32::from_rgb(76, 175, 120),
                series(|s| Some(s.autonomy)),
            ),
            axis(
                "Professional Git",
                egui::Color32::from_rgb(86, 140, 230),
                series(|s| s.git),
            ),
            axis(
                "Technical English",
                egui::Color32::from_rgb(150, 120, 230),
                series(|s| s.english),
            ),
        ]
    }

    pub(crate) fn ui(&mut self, ui: &mut Ui, host: &mut dyn StudentWorkspaceHost) {
        self.ensure_loaded();
        let ctx = ui.ctx().clone();
        self.poll(host);
        self.maybe_sync(host, &ctx);
        let busy = self.pending.is_some()
            || !self.queued.is_empty()
            || self.tip.pending.is_some()
            || self.run.is_some()
            || self.sync.is_some();
        if busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        let level = self.mic.as_ref().map_or(0.0, MicLevel::level);
        if let Some(band) = self.band.as_mut() {
            band.update(level, dt);
            if self.mic.is_some() || !band.idle() {
                ctx.request_repaint_after(std::time::Duration::from_millis(33));
            }
        }

        let full = ui.available_rect_before_wrap();
        let wide = full.width() > 1180.0 && self.exercise.is_some();
        let side = if wide { 380.0 } else { 0.0 };
        let main =
            egui::Rect::from_min_max(full.min, egui::pos2(full.right() - side, full.bottom()));
        let width = (main.width() - 48.0).clamp(280.0, COLUMN);
        let column = egui::Rect::from_min_max(
            egui::pos2(main.center().x - width * 0.5, main.top() + 12.0),
            egui::pos2(main.center().x + width * 0.5, main.bottom() - 10.0),
        );
        let composer = egui::Rect::from_min_max(
            egui::pos2(column.left(), column.bottom() - 96.0),
            column.max,
        );
        let body =
            egui::Rect::from_min_max(column.min, egui::pos2(column.right(), composer.top() - 8.0));

        ui.scope_builder(egui::UiBuilder::new().max_rect(body), |ui| {
            self.header(ui, host);
            if self.tutor.is_none() {
                self.landing(ui, host);
            } else {
                if !wide && self.exercise.is_some() {
                    egui::CollapsingHeader::new(RichText::new(tr("Exercise")).size(BODY).strong())
                        .id_salt("student-exercise")
                        .default_open(true)
                        .show(ui, |ui| self.exercise_card(ui, host, &ctx));
                }
                self.chat(ui);
            }
        });
        ui.scope_builder(egui::UiBuilder::new().max_rect(composer), |ui| {
            self.composer(ui, host, composer);
        });
        if wide {
            let panel = egui::Rect::from_min_max(
                egui::pos2(full.right() - side + 8.0, full.top() + 12.0),
                egui::pos2(full.right() - 16.0, full.bottom() - 10.0),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(panel), |ui| {
                card_frame().show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("student-side")
                        .show(ui, |ui| self.exercise_card(ui, host, &ctx));
                });
            });
        }
        self.tip_popout(&ctx, host, full);
    }

    fn header(&mut self, ui: &mut Ui, host: &mut dyn StudentWorkspaceHost) {
        ui.horizontal(|ui| {
            if let Some(tutor) = &self.tutor {
                let lessons = tutor.track.lessons();
                let last = lessons.last().map_or(0, |l| l.day);
                let day = tutor.current_lesson().map_or(last, |l| l.day);
                ui.label(
                    RichText::new(tr(tutor.track.title()))
                        .size(BODY)
                        .strong()
                        .color(colors().text),
                );
                let title = tutor.current_lesson().map_or("", |l| l.title);
                ui.label(
                    RichText::new(format!("{} {day}/{last}  {}", tr("Day"), tr(title)))
                        .size(SMALL)
                        .color(colors().secondary),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button(RichText::new(tr("Practice")).size(SMALL), |ui| {
                    let mut git = self.progress.coaching.professional_git;
                    let mut english = self.progress.coaching.technical_english;
                    ui.checkbox(&mut git, tr("Professional Git"));
                    ui.checkbox(&mut english, tr("Technical English"));
                    if git != self.progress.coaching.professional_git
                        || english != self.progress.coaching.technical_english
                    {
                        self.progress.coaching.professional_git = git;
                        self.progress.coaching.technical_english = english;
                        self.save();
                    }
                    ui.separator();
                    if ui.button(tr("Sync progress now")).clicked() {
                        if host.cloud().is_none() {
                            self.status = Some((
                                true,
                                tr("Sign in to a COVALOS account to sync.").to_owned(),
                            ));
                        } else {
                            self.synced_once = false;
                        }
                        ui.close();
                    }
                });
                if ui
                    .selectable_label(
                        self.show_progress,
                        RichText::new(tr("Progress")).size(SMALL),
                    )
                    .clicked()
                {
                    self.show_progress = !self.show_progress;
                }
                if self.progress.streak_days > 0 {
                    ui.label(
                        RichText::new(format!(
                            "{} {}  ·  {} xp",
                            self.progress.streak_days,
                            tr("day streak"),
                            self.progress.xp_total
                        ))
                        .size(SMALL)
                        .color(colors().secondary),
                    );
                }
                if self.sync.is_some() {
                    dot_glyph(ui, 14.0, LoaderPattern::Orbit, colors().accent)
                        .on_hover_text(tr("Syncing progress"));
                }
            });
        });
        if let Some((error, text)) = &self.status {
            let color = if *error {
                colors().error
            } else {
                colors().secondary
            };
            let mut dismiss = false;
            ui.horizontal(|ui| {
                ui.label(RichText::new(text).size(SMALL).color(color));
                dismiss = ui.small_button("×").on_hover_text(tr("Dismiss")).clicked();
            });
            if dismiss {
                self.status = None;
            }
        }
        if self.show_progress {
            ui.add_space(6.0);
            let axes = self.axes();
            let track = self.track().or(self.progress.active_track);
            card_frame().show(ui, |ui| {
                chart::score_donut(ui, "student-progress", &axes, &mut self.chart_selected);
                if let Some(track) = track {
                    let avg = self
                        .progress
                        .average_score(track)
                        .map_or("--".to_owned(), |a| format!("{a:.0}"));
                    ui.label(
                        RichText::new(format!(
                            "{}: {avg}   {}: {}",
                            tr("Average score"),
                            tr("Longest streak"),
                            self.progress.longest_streak
                        ))
                        .size(SMALL)
                        .color(colors().secondary),
                    );
                    let weak = self.progress.weak_topics(track).join(", ");
                    if !weak.is_empty() {
                        ui.label(
                            RichText::new(format!("{}: {weak}", tr("To review")))
                                .size(SMALL)
                                .color(colors().secondary),
                        );
                    }
                }
            });
        }
        ui.add_space(8.0);
    }

    fn landing(&mut self, ui: &mut Ui, host: &mut dyn StudentWorkspaceHost) {
        ui.add_space((ui.available_height() * 0.26).max(12.0));
        let mut chosen = None;
        ui.vertical_centered(|ui| {
            dot_glyph(ui, 34.0, LoaderPattern::Diamond, colors().text);
            ui.add_space(10.0);
            ui.label(
                RichText::new(tr("What do you want to learn?"))
                    .size(HEADING)
                    .color(colors().text),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(tr(
                    "Short lessons, your own code, real tests. The tutor guides; you write.",
                ))
                .size(SMALL)
                .color(colors().secondary),
            );
            ui.add_space(18.0);
        });
        ui.horizontal_wrapped(|ui| {
            let total = 3.0 * 196.0 + 2.0 * ui.spacing().item_spacing.x;
            ui.add_space(((ui.available_width() - total) * 0.5).max(0.0));
            for track in TrackId::ALL {
                let count = track.lessons().len();
                let detail = if self.progress.started(track) {
                    let next = (self.progress.next_lesson(track) + 1).min(count);
                    format!("{} {next}/{count}", tr("Lesson"))
                } else {
                    format!("{count} {}", tr("lessons"))
                };
                if track_card(ui, tr(track.title()), &detail).clicked() {
                    chosen = Some(track);
                }
            }
        });
        if let Some(track) = chosen {
            self.start_track(track, host);
        }
    }

    fn chat(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .id_salt("student-chat")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for message in &self.messages {
                    message_bubble(ui, message);
                    ui.add_space(8.0);
                }
                if self.pending.is_some() {
                    dot_loader(ui, tr("Thinking"), LoaderPattern::Ring, colors().accent);
                } else if self.queued.iter().any(|q| !q.tip) {
                    dot_loader(
                        ui,
                        tr("Connecting to your AI"),
                        LoaderPattern::Orbit,
                        colors().accent,
                    );
                }
            });
    }

    fn exercise_card(
        &mut self,
        ui: &mut Ui,
        host: &mut dyn StudentWorkspaceHost,
        ctx: &egui::Context,
    ) {
        let Some(ex) = &self.exercise else { return };
        let (brief, file, branch_hint, path) = (ex.brief, ex.file, ex.branch, ex.path.clone());
        let on_bridge = self.tutor.as_ref().is_some_and(|t| t.on_bridge);
        ui.label(
            RichText::new(if on_bridge {
                tr("Bridge exercise")
            } else {
                tr("Exercise")
            })
            .size(SMALL)
            .color(colors().secondary),
        );
        ui.add(egui::Label::new(RichText::new(brief).size(BODY).color(colors().text)).wrap());
        ui.label(
            RichText::new(file)
                .monospace()
                .size(SMALL)
                .color(colors().muted),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let running = self.run.is_some();
            let run = ui.add_enabled(
                !running,
                egui::Button::new(RichText::new(tr("Run & Grade")).strong())
                    .fill(colors().accent_bg)
                    .corner_radius(8),
            );
            if run.clicked() {
                self.run_and_grade(host, ctx);
            }
            if running {
                dot_loader(ui, tr("Running"), LoaderPattern::Wave, colors().accent);
            }
            if ui.button(tr("Open file")).clicked() {
                host.open_exercise(&path);
            }
        });
        ui.horizontal_wrapped(|ui| {
            let failing = self
                .result
                .as_ref()
                .and_then(|r| r.report.failures.first().cloned());
            if let Some(failure) = failing
                && ui.button(tr("Explain this error")).clicked()
            {
                self.ask_tip(
                    host,
                    format!("Explain this failing test in plain words: {failure}"),
                );
            }
            if ui.button(tr("Show tip")).clicked() {
                self.ask_tip(
                    host,
                    "Give me one quick tip for the next small step.".into(),
                );
            }
            let hints = self.tutor.as_ref().map_or(0, |t| t.hints);
            if ui
                .add_enabled(
                    self.pending.is_none() && hints < 3,
                    egui::Button::new(format!("{} {hints}/3", tr("Hint"))),
                )
                .on_hover_text(tr("Each hint lowers the autonomy score."))
                .clicked()
            {
                self.ask_hint(host);
            }
        });
        if let Some(result) = &self.result {
            ui.add_space(8.0);
            result_card(ui, result);
        }
        if self.progress.coaching.professional_git {
            self.git_steps(ui, host, branch_hint);
        }
    }

    fn git_steps(&mut self, ui: &mut Ui, host: &mut dyn StudentWorkspaceHost, hint: &str) {
        ui.add_space(10.0);
        ui.label(
            RichText::new(tr("Git"))
                .size(SMALL)
                .color(colors().secondary),
        );
        ui.label(
            RichText::new(format!(
                "{}: {}",
                tr("Branch"),
                self.branch.as_deref().unwrap_or("-")
            ))
            .size(SMALL)
            .monospace(),
        );
        let on_main = matches!(self.branch.as_deref(), None | Some("main" | "master"));
        if on_main {
            let check = practice::check_branch(&self.branch_draft);
            let mut create = false;
            ui.horizontal(|ui| {
                text_field(
                    ui,
                    "student-branch",
                    egui::TextEdit::singleline(&mut self.branch_draft)
                        .hint_text(hint)
                        .desired_width(190.0),
                );
                create = ui
                    .add_enabled(check.score >= 70, egui::Button::new(tr("Create branch")))
                    .clicked();
            });
            notes(ui, &check.notes);
            if create {
                self.create_branch();
            }
        }
        let check = practice::check_commit_message(&self.commit_draft);
        let mut commit = false;
        ui.horizontal(|ui| {
            text_field(
                ui,
                "student-commit",
                egui::TextEdit::singleline(&mut self.commit_draft)
                    .hint_text("Add input validation for user age")
                    .desired_width(210.0),
            );
            commit = ui
                .add_enabled(
                    check.score >= 70 && !on_main,
                    egui::Button::new(tr("Commit")),
                )
                .on_hover_text(tr("Commit after each step that passes its tests."))
                .clicked();
        });
        if !self.commit_draft.trim().is_empty() {
            notes(ui, &check.notes);
        }
        if commit {
            self.commit(host);
        }
    }

    fn composer(&mut self, ui: &mut Ui, host: &mut dyn StudentWorkspaceHost, rect: egui::Rect) {
        let listening = self.mic.is_some();
        let painter = ui.painter_at(rect.expand(2.0));
        painter.rect(
            rect,
            18.0,
            colors().surface.gamma_multiply(0.92),
            egui::Stroke::new(1.0_f32, colors().border),
            egui::StrokeKind::Inside,
        );
        if let Some(band) = &self.band {
            let alpha = if listening {
                1.0
            } else {
                band.level().min(1.0)
            };
            let strip = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 18.0, rect.bottom() - 30.0),
                egui::pos2(rect.right() - 18.0, rect.bottom() - 2.0),
            );
            voice::paint_voice_band(&ui.painter_at(rect.shrink(1.0)), strip, band, alpha);
        }
        let inner = rect.shrink2(egui::vec2(14.0, 10.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            let enabled = self.tutor.is_some();
            let hint = if listening {
                tr("Listening...")
            } else if enabled {
                tr("Answer the tutor or ask a question")
            } else {
                tr("Pick a track above to start")
            };
            let edit = ui.add_enabled(
                enabled,
                egui::TextEdit::multiline(&mut self.draft)
                    .hint_text(hint)
                    .frame(false)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
            let enter = edit.has_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
            if enter {
                // The multiline edit already inserted the newline.
                while self.draft.ends_with('\n') {
                    self.draft.pop();
                }
            }
            let mut switch = None;
            let mut send = false;
            let mut mic = false;
            let mut account_choice = None;
            let mut open_agents = false;
            let accounts = host.tutor_accounts();
            ui.horizontal(|ui| {
                let label = self.track().map_or(tr("Choose a track"), |t| tr(t.title()));
                ui.menu_button(RichText::new(label).size(SMALL), |ui| {
                    for track in TrackId::ALL {
                        if ui.button(tr(track.title())).clicked() {
                            switch = Some(track);
                            ui.close();
                        }
                    }
                });
                let muted = |text: String| RichText::new(text).size(SMALL).color(colors().muted);
                match &accounts {
                    TutorAccounts::Ready(ready) => {
                        let current = self
                            .tutor_account
                            .as_ref()
                            .and_then(|id| ready.iter().find(|a| &a.id == id))
                            .or(ready.first());
                        let name = current.map_or("", |a| a.label.as_str());
                        let title = format!("{}: {name}", tr("Tutor"));
                        if ready.len() > 1 {
                            ui.menu_button(RichText::new(title).size(SMALL), |ui| {
                                for account in ready {
                                    let on = current.is_some_and(|c| c.id == account.id);
                                    if ui.selectable_label(on, &account.label).clicked() {
                                        account_choice =
                                            Some((account.id.clone(), account.label.clone()));
                                        ui.close();
                                    }
                                }
                            });
                        } else {
                            ui.label(muted(title));
                        }
                    }
                    TutorAccounts::Checking => {
                        ui.label(muted(tr("Checking AI accounts").to_owned()));
                    }
                    TutorAccounts::Missing => {
                        open_agents = ui
                            .small_button(tr("Connect an AI"))
                            .on_hover_text(tr(
                                "Open Agents to sign in to Claude Code, Codex or Antigravity.",
                            ))
                            .clicked();
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can_send =
                        enabled && self.pending.is_none() && !self.draft.trim().is_empty();
                    let button = round_button(ui, tr("Send (Enter)"), can_send);
                    let ink = if can_send {
                        colors().text
                    } else {
                        colors().disabled
                    };
                    draw_icon(ui.painter(), button.rect.center(), Icon::Send, ink);
                    send = (button.clicked() || enter) && can_send;
                    let tip = if listening {
                        tr("Stop listening")
                    } else {
                        tr("Microphone level (dictation is not available yet)")
                    };
                    let button = round_button(ui, tip, true);
                    let ink = if listening {
                        colors().accent
                    } else {
                        colors().secondary
                    };
                    paint_mic(ui.painter(), button.rect.center(), ink);
                    mic = button.clicked();
                    if listening {
                        ui.label(
                            RichText::new(tr("Listening..."))
                                .size(SMALL)
                                .color(colors().secondary),
                        );
                    }
                });
            });
            if let Some((id, label)) = account_choice {
                self.choose_account(id, &label);
            }
            if open_agents {
                host.open_agents();
            }
            if let Some(track) = switch {
                self.start_track(track, host);
            }
            if send {
                self.send_student_message(host);
            }
            if mic {
                self.toggle_mic();
            }
        });
    }

    fn tip_popout(
        &mut self,
        ctx: &egui::Context,
        host: &mut dyn StudentWorkspaceHost,
        area: egui::Rect,
    ) {
        let id = egui::Id::new("student-tip-popout");
        let t = ctx.animate_bool_with_time(id, self.tip.open, 0.22);
        if t <= 0.0 {
            return;
        }
        let eased = egui::emath::easing::cubic_out(t);
        let size = egui::vec2(340.0, 300.0);
        let pos = egui::pos2(
            area.right() - size.x - 20.0,
            area.bottom() - size.y - 140.0 + (1.0 - eased) * 18.0,
        );
        let mut close = false;
        let mut question = None;
        egui::Area::new(id)
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                ui.multiply_opacity(eased);
                egui::Frame::new()
                    .fill(colors().elevated)
                    .stroke(egui::Stroke::new(1.0_f32, colors().border))
                    .corner_radius(12)
                    .shadow(egui::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(120),
                    })
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(size.x - 24.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(tr("Tips")).size(BODY).strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let button = ui.add(
                                        egui::Button::new("")
                                            .frame(false)
                                            .min_size(egui::vec2(18.0, 18.0)),
                                    );
                                    draw_icon(
                                        ui.painter(),
                                        button.rect.center(),
                                        Icon::Close,
                                        colors().secondary,
                                    );
                                    close = button.on_hover_text(tr("Close")).clicked();
                                },
                            );
                        });
                        egui::ScrollArea::vertical()
                            .id_salt("student-tip-scroll")
                            .max_height(size.y - 90.0)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for message in &self.tip.messages {
                                    message_bubble(ui, message);
                                    ui.add_space(6.0);
                                }
                                if self.tip.pending.is_some() {
                                    dot_loader(
                                        ui,
                                        tr("Thinking"),
                                        LoaderPattern::Pulse,
                                        colors().accent,
                                    );
                                }
                            });
                        let edit = text_field(
                            ui,
                            "student-tip-input",
                            egui::TextEdit::singleline(&mut self.tip.draft)
                                .hint_text(tr("Ask a quick question"))
                                .desired_width(f32::INFINITY),
                        );
                        if edit.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            && !self.tip.draft.trim().is_empty()
                        {
                            question = Some(std::mem::take(&mut self.tip.draft));
                        }
                    });
            });
        if close {
            self.tip.open = false;
        }
        if let Some(q) = question {
            self.ask_tip(host, q.trim().to_owned());
        }
    }
}

/// Plain-text summary of the last run for the tutor and the tips chat.
fn summary(card: &RunCard) -> String {
    let mut out = format!(
        "Score {}/100 (correctness {}/50, autonomy {}/30, clarity {}/20). Tests {}/{} passed.",
        card.grade.total(),
        card.grade.correctness,
        card.grade.autonomy,
        card.grade.clarity,
        card.report.passed,
        card.report.total
    );
    if card.timed_out {
        out.push_str(" The run timed out, possibly an infinite loop.");
    }
    for failure in &card.report.failures {
        out.push_str(&format!("\nFAIL {failure}"));
    }
    if card.report.total == 0 && !card.stderr.is_empty() {
        out.push_str(&format!("\nError output: {}", card.stderr));
    }
    for note in card
        .grade
        .notes
        .iter()
        .chain(card.english.iter().flat_map(|c| &c.notes))
        .chain(card.git.iter().flat_map(|c| &c.notes))
    {
        out.push_str(&format!("\nNote: {note}"));
    }
    out
}

fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(colors().surface.gamma_multiply(0.9))
        .stroke(egui::Stroke::new(1.0_f32, colors().border))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(12))
}

fn notes(ui: &mut Ui, notes: &[String]) {
    for note in notes {
        ui.add(
            egui::Label::new(
                RichText::new(format!("· {note}"))
                    .size(SMALL)
                    .color(colors().secondary),
            )
            .wrap(),
        );
    }
}

fn result_card(ui: &mut Ui, result: &RunCard) {
    let grade = &result.grade;
    let color = if grade.passed() {
        colors().success
    } else {
        colors().warning
    };
    card_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(grade.total().to_string())
                    .size(HEADING + 6.0)
                    .monospace()
                    .color(color),
            );
            ui.label(RichText::new("/100").size(SMALL).color(colors().muted));
            ui.label(
                RichText::new(match result.outcome {
                    Outcome::Passed => tr("Passed. Next lesson unlocked."),
                    Outcome::BridgeCleared => tr("Bridge done. Back to the lesson exercise."),
                    Outcome::Bridge => tr("A smaller step first."),
                    Outcome::Retry => tr("Under 70. Fix it and run again."),
                })
                .size(SMALL)
                .color(colors().secondary),
            );
        });
        for (label, points, max) in [
            (tr("Correctness"), grade.correctness, 50u8),
            (tr("Autonomy"), grade.autonomy, 30),
            (tr("Clarity"), grade.clarity, 20),
        ] {
            score_row(
                ui,
                label,
                f32::from(points) / f32::from(max),
                &format!("{points}/{max}"),
            );
        }
        if let Some(git) = &result.git {
            score_row(
                ui,
                tr("Git"),
                f32::from(git.score) / 100.0,
                &git.score.to_string(),
            );
        }
        if let Some(english) = &result.english {
            score_row(
                ui,
                tr("English"),
                f32::from(english.score) / 100.0,
                &english.score.to_string(),
            );
        }
        ui.label(
            RichText::new(format!(
                "{} {}/{}",
                tr("Tests passed"),
                result.report.passed,
                result.report.total
            ))
            .size(SMALL)
            .color(colors().secondary),
        );
        if result.timed_out {
            notes(
                ui,
                &[tr("The run took longer than 20 seconds and was stopped.").to_owned()],
            );
        }
        let failures: Vec<String> = result
            .report
            .failures
            .iter()
            .map(|f| format!("FAIL {f}"))
            .collect();
        notes(ui, &failures);
        if result.report.total == 0 && !result.stderr.trim().is_empty() {
            ui.label(
                RichText::new(result.stderr.trim())
                    .monospace()
                    .size(SMALL)
                    .color(colors().error),
            );
        }
        notes(ui, &grade.notes);
        for check in result.english.iter().chain(result.git.iter()) {
            notes(ui, &check.notes);
        }
    });
}

/// A dot-matrix bar: twenty square dots, lit in proportion.
fn score_row(ui: &mut Ui, label: &str, fraction: f32, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized(
            [96.0, 16.0],
            egui::Label::new(RichText::new(label).size(SMALL).color(colors().secondary)),
        );
        let (rect, _) = ui.allocate_exact_size(egui::vec2(140.0, 10.0), egui::Sense::hover());
        let lit = (fraction.clamp(0.0, 1.0) * 20.0).round() as usize;
        for i in 0..20 {
            let center = egui::pos2(rect.left() + i as f32 * 7.0 + 2.5, rect.center().y);
            let color = if i < lit {
                colors().accent
            } else {
                colors().muted.gamma_multiply(0.25)
            };
            ui.painter().rect_filled(
                egui::Rect::from_center_size(center, egui::vec2(4.5, 4.5)),
                1.0,
                color,
            );
        }
        ui.label(
            RichText::new(value)
                .size(SMALL)
                .monospace()
                .color(colors().text),
        );
    });
}

fn message_bubble(ui: &mut Ui, message: &Message) {
    match message.who {
        Who::Tutor => crate::editor::hub::markdown(ui, &message.text),
        Who::Student => {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                egui::Frame::new()
                    .fill(colors().elevated)
                    .corner_radius(12)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width() * 0.8);
                        ui.add(egui::Label::new(RichText::new(&message.text).size(BODY)).wrap());
                    });
            });
        }
        Who::Note => {
            ui.label(
                RichText::new(&message.text)
                    .size(SMALL)
                    .color(colors().warning),
            );
        }
    }
}

fn track_card(ui: &mut Ui, title: &str, detail: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(196.0, 64.0), egui::Sense::click());
    let hover = ui
        .ctx()
        .animate_bool_with_time(response.id, response.hovered(), 0.12);
    let blend = crate::editor::motion::blend;
    let painter = ui.painter();
    painter.rect(
        rect,
        12.0,
        blend(colors().surface, colors().elevated, hover).gamma_multiply(0.92),
        egui::Stroke::new(
            1.0_f32,
            blend(colors().border, colors().accent, hover * 0.6),
        ),
        egui::StrokeKind::Inside,
    );
    painter.text(
        egui::pos2(rect.left() + 14.0, rect.top() + 22.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional(BODY),
        colors().text,
    );
    painter.text(
        egui::pos2(rect.left() + 14.0, rect.top() + 44.0),
        egui::Align2::LEFT_CENTER,
        detail,
        egui::FontId::proportional(SMALL),
        colors().secondary,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    crate::editor::theme::pointer(response)
}

/// Empty circular button; the caller paints its glyph on top.
fn round_button(ui: &mut Ui, tip: &str, enabled: bool) -> egui::Response {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), sense);
    let fill = if response.hovered() && enabled {
        colors().hover
    } else {
        colors().elevated
    };
    ui.painter().circle(
        rect.center(),
        15.0,
        fill,
        egui::Stroke::new(1.0_f32, colors().border),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    crate::editor::theme::pointer(response.on_hover_text(tip))
}

/// A small microphone glyph; the icon set has none.
fn paint_mic(painter: &egui::Painter, center: egui::Pos2, color: egui::Color32) {
    let stroke = egui::Stroke::new(1.5_f32, color);
    let body = egui::Rect::from_center_size(center - egui::vec2(0.0, 2.0), egui::vec2(6.0, 10.0));
    painter.rect_stroke(body, 3.0, stroke, egui::StrokeKind::Middle);
    let arc: Vec<egui::Pos2> = (0..=12)
        .map(|i| {
            let a = std::f32::consts::PI * (i as f32 / 12.0);
            center + egui::vec2(-5.5 * a.cos(), 1.0 + 4.5 * a.sin())
        })
        .collect();
    painter.add(egui::Shape::line(arc, stroke));
    painter.line_segment(
        [center + egui::vec2(0.0, 5.5), center + egui::vec2(0.0, 8.0)],
        stroke,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::Path;

    /// A host that records prompts and answers the tutor from a script.
    #[derive(Default)]
    struct FakeHost {
        asked: RefCell<Vec<String>>,
        replies: Vec<String>,
        opened: Vec<PathBuf>,
        /// `None` means two ready accounts, Claude Code first.
        accounts: Option<TutorAccounts>,
        /// Account id used for each prompt.
        used: Vec<String>,
        agents_opened: bool,
    }

    impl StudentWorkspaceHost for FakeHost {
        fn tutor_accounts(&mut self) -> TutorAccounts {
            self.accounts.clone().unwrap_or_else(|| {
                TutorAccounts::Ready(vec![
                    TutorAccount {
                        id: "claude".into(),
                        label: "Claude Code".into(),
                    },
                    TutorAccount {
                        id: "codex".into(),
                        label: "Codex".into(),
                    },
                ])
            })
        }
        fn ask_tutor(
            &mut self,
            account: &str,
            conversation: Option<&str>,
            prompt: &str,
            _root: &Path,
        ) -> Result<TutorTicket, String> {
            self.used.push(account.to_owned());
            self.asked.borrow_mut().push(prompt.to_owned());
            Ok(TutorTicket {
                conversation: conversation.unwrap_or("c1").to_owned(),
                seen: self.asked.borrow().len() - 1,
            })
        }
        fn tutor_reply(&self, ticket: &TutorTicket) -> TutorPoll {
            self.replies
                .get(ticket.seen)
                .map_or(TutorPoll::Pending, |r| TutorPoll::Reply(r.clone()))
        }
        fn open_agents(&mut self) {
            self.agents_opened = true;
        }
        fn open_exercise(&mut self, path: &Path) {
            self.opened.push(path.to_owned());
        }
        fn buffer_text(&self, path: &Path) -> Option<String> {
            std::fs::read_to_string(path).ok()
        }
        fn save_buffer(&mut self, _path: &Path) -> Result<(), String> {
            Ok(())
        }
        fn cloud(&self) -> Option<super::super::host::CloudCredentials> {
            None
        }
        fn spanish(&self) -> bool {
            false
        }
    }

    fn mode(dir: &Path) -> StudentMode {
        StudentMode {
            loaded: true,
            config: Some(dir.to_owned()),
            progress_path: Some(dir.join("student/progress.json")),
            ..Default::default()
        }
    }

    #[test]
    fn a_lesson_flows_from_explanation_to_exercise_and_grade() {
        let dir = tempfile::tempdir().unwrap();
        let mut student = mode(dir.path());
        // Not the first time on this track, so the diagnostic is skipped.
        student
            .progress
            .start_lesson(TrackId::Python, "py-01-basics", 1);
        let mut host = FakeHost {
            replies: vec![
                "Here is the idea.\n```python\ndef classify_age(age):\n    if age < 0:\n        return 'invalid'\n```".into(),
                "Feedback".into(),
            ],
            ..Default::default()
        };
        student.start_track(TrackId::Python, &mut host);
        assert!(host.asked.borrow()[0].contains("Never write the solution"));
        assert!(host.asked.borrow()[0].contains("Teach the lesson"));
        student.poll(&mut host);
        assert!(!student.messages[0].text.contains("def classify_age"));
        assert_eq!(host.opened.len(), 1);
        let ex = student.exercise.as_ref().unwrap();
        assert!(ex.path.ends_with("src/day01/age.py"));
        assert!(ex.root.join("README.md").exists());

        std::fs::write(&ex.path, "def classify_age(age):\n    return \"adult\"\n").unwrap();
        let output = RunOutput {
            stdout: "TOTAL 5\nFAIL a: x\nFAIL b: x\nFAIL c: x\nFAIL d: x\nPASS 18 is an adult\n"
                .into(),
            stderr: String::new(),
            timed_out: false,
        };
        student.finish_run(Ok(output), &mut host);
        let card = student.result.as_ref().unwrap();
        assert_eq!(card.grade.correctness, 10);
        assert_eq!(card.outcome, Outcome::Retry);
        assert!(
            host.asked
                .borrow()
                .last()
                .unwrap()
                .contains("under the pass mark")
        );
        let saved = Progress::load(&dir.path().join("student/progress.json"));
        let lesson = saved.lesson(TrackId::Python, "py-01-basics").unwrap();
        assert_eq!(lesson.attempts, 1);
        assert_eq!(saved.sessions.len(), 1);
        assert!(saved.sessions[0].english.is_some());
    }

    #[test]
    fn two_low_scores_open_the_bridge_without_completing_the_lesson() {
        let dir = tempfile::tempdir().unwrap();
        let mut student = mode(dir.path());
        student
            .progress
            .start_lesson(TrackId::Python, "py-01-basics", 1);
        let mut host = FakeHost {
            replies: vec!["Lesson".into()],
            ..Default::default()
        };
        student.start_track(TrackId::Python, &mut host);
        student.poll(&mut host);
        let fail = || {
            Ok(RunOutput {
                stdout: "TOTAL 5\nFAIL a: x\n".into(),
                stderr: String::new(),
                timed_out: false,
            })
        };
        student.finish_run(fail(), &mut host);
        student.finish_run(fail(), &mut host);
        assert!(student.tutor.as_ref().unwrap().on_bridge);
        assert!(host.opened.last().unwrap().ends_with("src/day01/adult.py"));
        assert!(
            host.asked
                .borrow()
                .last()
                .unwrap()
                .contains("bridge exercise")
        );
    }

    #[test]
    fn asking_for_the_solution_is_sent_as_a_refusal() {
        let dir = tempfile::tempdir().unwrap();
        let mut student = mode(dir.path());
        student
            .progress
            .start_lesson(TrackId::Sql, "sql-01-select", 1);
        let mut host = FakeHost {
            replies: vec!["Explained".into(), "No".into()],
            ..Default::default()
        };
        student.start_track(TrackId::Sql, &mut host);
        student.poll(&mut host);
        student.draft = "hacelo vos".into();
        student.send_student_message(&mut host);
        assert!(
            host.asked
                .borrow()
                .last()
                .unwrap()
                .contains("Decline kindly")
        );
    }

    #[test]
    fn prompts_wait_for_sign_in_checks_and_then_go_out() {
        let dir = tempfile::tempdir().unwrap();
        let mut student = mode(dir.path());
        let mut host = FakeHost {
            accounts: Some(TutorAccounts::Checking),
            replies: vec!["Hello".into()],
            ..Default::default()
        };
        student.start_track(TrackId::Python, &mut host);
        assert!(
            host.asked.borrow().is_empty(),
            "nothing is sent while checking"
        );
        assert_eq!(student.queued.len(), 1);
        assert!(
            student.messages.is_empty(),
            "no error while accounts are checked"
        );
        host.accounts = None;
        student.poll(&mut host);
        assert!(student.queued.is_empty());
        assert_eq!(host.used, vec!["claude"]);
        // The same poll sends the prompt and reads the reply.
        assert!(student.messages.iter().any(|m| m.text == "Hello"));
    }

    #[test]
    fn no_connected_ai_explains_how_to_connect() {
        let dir = tempfile::tempdir().unwrap();
        let mut student = mode(dir.path());
        let mut host = FakeHost {
            accounts: Some(TutorAccounts::Missing),
            ..Default::default()
        };
        student.start_track(TrackId::Python, &mut host);
        assert!(host.asked.borrow().is_empty());
        assert!(student.messages[0].text.contains("Connect an AI"));
    }

    #[test]
    fn switching_the_tutor_account_starts_a_new_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut student = mode(dir.path());
        let mut host = FakeHost {
            replies: vec!["q1".into(), "q2".into()],
            ..Default::default()
        };
        student.start_track(TrackId::Python, &mut host);
        student.poll(&mut host);
        assert_eq!(student.conversation.as_deref(), Some("c1"));
        student.choose_account("codex".into(), "Codex");
        assert!(student.conversation.is_none());
        student.draft = "why?".into();
        student.send_student_message(&mut host);
        assert_eq!(host.used, vec!["claude", "codex"]);
        // A new session repeats the rules for the new model.
        assert!(
            host.asked
                .borrow()
                .last()
                .unwrap()
                .contains("Never write the solution")
        );
    }

    #[test]
    fn hints_climb_the_ladder() {
        let dir = tempfile::tempdir().unwrap();
        let mut student = mode(dir.path());
        let mut host = FakeHost {
            replies: vec!["q1".into(), "h1".into(), "h2".into()],
            ..Default::default()
        };
        student.start_track(TrackId::TypeScript, &mut host);
        assert!(host.asked.borrow()[0].contains("Diagnostic question 1"));
        student.poll(&mut host);
        student.ask_hint(&mut host);
        student.poll(&mut host);
        student.ask_hint(&mut host);
        assert!(host.asked.borrow().last().unwrap().contains("level 2 hint"));
        assert_eq!(student.tutor.as_ref().unwrap().hints, 2);
    }

    #[test]
    fn chart_axes_use_only_recorded_sessions() {
        let axes = StudentMode::default().axes();
        assert_eq!(axes.len(), 4);
        assert!(
            axes.iter()
                .all(|a| a.score.is_none() && a.history.is_empty())
        );
    }

    #[test]
    fn view_renders_without_a_session_and_with_an_exercise() {
        let dir = tempfile::tempdir().unwrap();
        let mut student = mode(dir.path());
        let mut host = FakeHost::default();
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| student.ui(ui, &mut host));
        });
        student
            .progress
            .start_lesson(TrackId::Python, "py-01-basics", 1);
        host.replies = vec!["Lesson".into()];
        student.start_track(TrackId::Python, &mut host);
        student.show_progress = true;
        student.tip.open = true;
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| student.ui(ui, &mut host));
            });
        }
        assert!(student.exercise.is_some());
    }
}
