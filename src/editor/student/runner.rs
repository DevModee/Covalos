//! The student's practice workspace: a small git repository with the standard layout,
//! exercise files written once, and test harnesses run in a background thread with a
//! timeout. Uses only std and the system's python, node and git.
use super::curriculum::{self, Exercise, Lang, TrackId};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(20);
const MAX_OUTPUT: u64 = 64 * 1024;

pub(crate) fn workspace_root(config: &Path, track: TrackId) -> PathBuf {
    config.join("student").join("workspace").join(track.key())
}

fn command(program: &str) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashes while tests run.
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Creates the layout a team would expect and a git repository on first use.
pub(crate) fn prepare_workspace(root: &Path, track: TrackId) -> Result<(), String> {
    for dir in ["src", "tests"] {
        std::fs::create_dir_all(root.join(dir)).map_err(|e| e.to_string())?;
    }
    write_if_missing(
        &root.join("README.md"),
        &format!(
            "# {} practice\n\nExercises from Covalos Student Mode. Code lives in `src/`, tests in `tests/`.\n",
            track.title()
        ),
    )?;
    write_if_missing(
        &root.join(".gitignore"),
        "__pycache__/\n*.pyc\nnode_modules/\n.env\n*.db\n",
    )?;
    if !root.join(".git").exists() {
        // Without git the lessons still work; the Git checks just stay empty.
        let _ = git(root, &["init", "-b", "main"]);
    }
    Ok(())
}

fn write_if_missing(path: &Path, text: &str) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Writes the starter once (never over the student's work) and the test harness
/// every time, so an edited harness cannot change the grade. Returns the exercise path.
pub(crate) fn write_exercise(
    root: &Path,
    lang: Lang,
    exercise: &Exercise,
) -> Result<PathBuf, String> {
    let file = root.join(exercise.file);
    write_if_missing(&file, exercise.starter)?;
    let tests = root.join(exercise.test_file);
    if let Some(dir) = tests.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&tests, curriculum::test_file_contents(lang, exercise))
        .map_err(|e| e.to_string())?;
    Ok(file)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunOutput {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) timed_out: bool,
}

/// The interpreter command for a language, found on PATH.
fn interpreter(lang: Lang) -> Result<(&'static str, &'static [&'static str]), String> {
    let candidates: &[(&'static str, &'static [&'static str])] = match lang {
        Lang::Python | Lang::Sql => &[("python", &[]), ("py", &["-3"]), ("python3", &[])],
        Lang::TypeScript => &[("node", &["--experimental-strip-types", "--no-warnings"])],
    };
    for (program, args) in candidates {
        let ok = command(program)
            .args(*args)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            return Ok((program, args));
        }
    }
    Err(match lang {
        Lang::TypeScript => {
            "Node.js 22.6 or newer is needed to run TypeScript exercises. Install it from nodejs.org."
                .into()
        }
        Lang::Python | Lang::Sql => {
            "Python 3 is needed to run these exercises. Install it from python.org.".into()
        }
    })
}

fn drain<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut data = Vec::new();
        if let Some(pipe) = pipe {
            let _ = std::io::Read::read_to_end(&mut pipe.take(MAX_OUTPUT), &mut data);
        }
        String::from_utf8_lossy(&data).into_owned()
    })
}

/// Runs the harness with a timeout and returns its output.
pub(crate) fn run_tests(root: &Path, lang: Lang, test_file: &str) -> Result<RunOutput, String> {
    let (program, args) = interpreter(lang)?;
    let mut child = command(program)
        .args(args)
        .arg(test_file)
        .current_dir(root)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Could not start {program}. {e}"))?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let started = Instant::now();
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(RunOutput {
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
        timed_out,
    })
}

/// Runs the tests on a worker thread; the receiver yields once.
pub(crate) fn spawn_tests(
    root: PathBuf,
    lang: Lang,
    test_file: &'static str,
    ctx: eframe::egui::Context,
) -> mpsc::Receiver<Result<RunOutput, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(run_tests(&root, lang, test_file));
        ctx.request_repaint();
    });
    rx
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = command("git")
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "Git is not installed. Install it from git-scm.com.".to_owned())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(err.lines().next().unwrap_or("git failed").trim().to_owned())
    }
}

pub(crate) fn current_branch(root: &Path) -> Option<String> {
    git(root, &["branch", "--show-current"])
        .ok()
        .filter(|b| !b.is_empty())
}

/// Switches to `branch`, creating it from the current one when needed.
pub(crate) fn switch_branch(root: &Path, branch: &str) -> Result<(), String> {
    if git(root, &["switch", branch]).is_ok() {
        return Ok(());
    }
    git(root, &["switch", "-c", branch]).map(|_| ())
}

/// Stages everything and commits with the student's message. A first commit needs an
/// identity; a local, clearly labelled one is set only when none exists.
pub(crate) fn commit(root: &Path, message: &str) -> Result<String, String> {
    if git(root, &["config", "user.name"]).is_err() {
        git(root, &["config", "user.name", "Covalos Student"])?;
        git(root, &["config", "user.email", "student@localhost"])?;
    }
    git(root, &["add", "-A"])?;
    git(root, &["commit", "-m", message])?;
    git(root, &["rev-parse", "--short", "HEAD"])
}

/// Files changed but not committed yet; used to suggest small commits.
pub(crate) fn changed_files(root: &Path) -> usize {
    git(root, &["status", "--porcelain"]).map_or(0, |s| s.lines().count())
}

#[cfg(test)]
mod tests {
    use super::super::{practice, rubric};
    use super::*;

    #[test]
    fn workspace_gets_the_standard_layout_and_keeps_student_work() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("python");
        prepare_workspace(&root, TrackId::Python).unwrap();
        assert_eq!(practice::check_structure(&root).score, 100);
        let lesson = &TrackId::Python.lessons()[0];
        let file = write_exercise(&root, Lang::Python, &lesson.exercise).unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            lesson.exercise.starter
        );
        std::fs::write(&file, "def classify_age(age):\n    return 'adult'\n").unwrap();
        std::fs::write(root.join(lesson.exercise.test_file), "rigged").unwrap();
        write_exercise(&root, Lang::Python, &lesson.exercise).unwrap();
        assert!(std::fs::read_to_string(&file).unwrap().contains("'adult'"));
        let harness = std::fs::read_to_string(root.join(lesson.exercise.test_file)).unwrap();
        assert!(harness.contains("TOTAL 5"));
    }

    /// Runs the real harnesses when Python is installed; returns early otherwise so
    /// machines without Python stay green.
    #[test]
    fn python_harness_grades_starter_solution_and_syntax_errors() {
        if interpreter(Lang::Python).is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let ex = &TrackId::Python.lessons()[0].exercise;
        let file = write_exercise(root, Lang::Python, ex).unwrap();
        let starter = run_tests(root, Lang::Python, ex.test_file).unwrap();
        let report = rubric::parse_report(&starter.stdout);
        assert_eq!((report.passed, report.total), (0, 5), "{starter:?}");

        std::fs::write(
            &file,
            "def classify_age(age):\n    if age < 0:\n        return \"invalid\"\n    if age < 13:\n        return \"child\"\n    if age < 18:\n        return \"teen\"\n    return \"adult\"\n",
        )
        .unwrap();
        let solved = run_tests(root, Lang::Python, ex.test_file).unwrap();
        let report = rubric::parse_report(&solved.stdout);
        assert_eq!((report.passed, report.total), (5, 5), "{solved:?}");

        std::fs::write(&file, "def classify_age(age)\n").unwrap();
        let broken = run_tests(root, Lang::Python, ex.test_file).unwrap();
        let report = rubric::parse_report(&broken.stdout);
        assert_eq!((report.passed, report.total), (0, 5));
        assert!(report.failures[0].starts_with("load"), "{broken:?}");
    }

    #[test]
    fn every_python_and_sql_harness_runs_on_its_starter() {
        if interpreter(Lang::Python).is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        for track in [TrackId::Python, TrackId::Sql] {
            for lesson in track.lessons() {
                for ex in [&lesson.exercise, &lesson.bridge] {
                    write_exercise(dir.path(), track.language(), ex).unwrap();
                    let out = run_tests(dir.path(), track.language(), ex.test_file).unwrap();
                    let report = rubric::parse_report(&out.stdout);
                    assert!(report.total > 0, "{}: {out:?}", ex.test_file);
                    assert!(
                        report.passed < report.total,
                        "{} passes on the starter",
                        ex.test_file
                    );
                }
            }
        }
    }

    #[test]
    fn sql_harness_runs_the_students_query() {
        if interpreter(Lang::Sql).is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let ex = &TrackId::Sql.lessons()[0].exercise;
        let file = write_exercise(root, Lang::Sql, ex).unwrap();
        std::fs::write(
            &file,
            "-- older users\nSELECT name FROM users WHERE age > 30 ORDER BY name;\n",
        )
        .unwrap();
        let out = run_tests(root, Lang::Sql, ex.test_file).unwrap();
        let report = rubric::parse_report(&out.stdout);
        assert_eq!((report.passed, report.total), (1, 1), "{out:?}");
    }

    #[test]
    fn git_branch_and_commit_flow() {
        if git(Path::new("."), &["--version"]).is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        prepare_workspace(root, TrackId::Python).unwrap();
        switch_branch(root, "learn/day-01-conditionals").unwrap();
        assert_eq!(
            current_branch(root).as_deref(),
            Some("learn/day-01-conditionals")
        );
        assert!(changed_files(root) > 0);
        let hash = commit(root, "Add project layout").unwrap();
        assert!(!hash.is_empty());
        assert_eq!(changed_files(root), 0);
        switch_branch(root, "learn/day-01-conditionals").unwrap();
    }
}
