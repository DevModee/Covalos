//! Professional practice checks: branch names, commit messages, identifier English and
//! project layout. Each check returns a 0-100 score with short, concrete notes so the
//! tutor and the result card can explain what a real team would ask for.
use super::curriculum::Lang;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Check {
    pub(crate) score: u8,
    pub(crate) notes: Vec<String>,
}

impl Check {
    fn from(deductions: u32, notes: Vec<String>) -> Self {
        Self {
            score: 100u32.saturating_sub(deductions) as u8,
            notes,
        }
    }
}

const BRANCH_KINDS: &[&str] = &["learn", "feat", "fix", "chore", "docs", "test", "refactor"];

/// `kind/short-kebab-description`, never `main`.
pub(crate) fn check_branch(name: &str) -> Check {
    let name = name.trim();
    if name.is_empty() || matches!(name, "main" | "master") {
        return Check::from(
            100,
            vec!["Create a branch before you start; never work directly on main.".into()],
        );
    }
    let mut d = 0;
    let mut notes = Vec::new();
    match name.split_once('/') {
        Some((kind, rest)) => {
            if !BRANCH_KINDS.contains(&kind) {
                d += 25;
                notes.push("Start the branch with a kind such as learn/, feat/ or fix/.".into());
            }
            let kebab = !rest.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
            if !kebab {
                d += 30;
                notes.push(
                    "Use lowercase kebab-case after the slash, like day-01-variables.".into(),
                );
            }
        }
        None => {
            d += 40;
            notes.push("Use kind/description, for example feat/todo-cli.".into());
        }
    }
    if name.len() > 40 {
        d += 10;
        notes.push("Keep branch names short.".into());
    }
    Check::from(d, notes)
}

const VAGUE: &[&str] = &[
    "update", "updates", "fix", "fixes", "final", "changes", "change", "wip", "stuff", "misc",
    "asdf", "test", "commit", "done", "cambios", "arreglo", "listo",
];

const PREFIXES: &[&str] = &[
    "feat:",
    "fix:",
    "chore:",
    "refactor:",
    "docs:",
    "test:",
    "ci:",
    "style:",
];

/// Common Spanish words that show up in student commit messages and names, with the
/// word an English-speaking team would use.
const SPANISH: &[(&str, &str)] = &[
    ("agrego", "add"),
    ("agregar", "add"),
    ("agregado", "add"),
    ("arreglo", "fix"),
    ("arreglar", "fix"),
    ("corrijo", "fix"),
    ("cambio", "change"),
    ("cambios", "changes"),
    ("nuevo", "new"),
    ("nueva", "new"),
    ("funcion", "function"),
    ("función", "function"),
    ("archivo", "file"),
    ("usuario", "user"),
    ("usuarios", "users"),
    ("nombre", "name"),
    ("edad", "age"),
    ("numero", "number"),
    ("número", "number"),
    ("numeros", "numbers"),
    ("lista", "list"),
    ("suma", "sum"),
    ("resultado", "result"),
    ("cantidad", "count"),
    ("precio", "price"),
    ("valor", "value"),
    ("datos", "data"),
    ("mensaje", "message"),
    ("contador", "counter"),
    ("promedio", "average"),
    ("calcular", "calculate"),
    ("obtener", "get"),
    ("imprimir", "print"),
    ("verificar", "check"),
    ("validar", "validate"),
    ("guardar", "save"),
    ("cargar", "load"),
    ("tarea", "task"),
    ("tareas", "tasks"),
    ("palabra", "word"),
    ("palabras", "words"),
    ("cuenta", "account"),
    ("saldo", "balance"),
    ("para", "for"),
    ("con", "with"),
];

fn spanish(word: &str) -> Option<&'static str> {
    let w = word.to_lowercase();
    SPANISH.iter().find(|(es, _)| *es == w).map(|(_, en)| *en)
}

const VERBS: &[&str] = &[
    "add",
    "fix",
    "update",
    "remove",
    "rename",
    "handle",
    "validate",
    "create",
    "implement",
    "move",
    "extract",
    "use",
    "make",
    "return",
    "read",
    "write",
    "parse",
    "print",
    "improve",
    "simplify",
    "test",
    "document",
    "replace",
    "support",
    "allow",
    "prevent",
    "show",
    "hide",
    "store",
    "load",
    "split",
    "check",
    "count",
    "convert",
    "calculate",
    "initialize",
    "set",
    "keep",
    "skip",
    "save",
    "delete",
    "group",
    "format",
    "sort",
    "filter",
    "reject",
    "raise",
    "clean",
    "refactor",
    "change",
    "list",
];

/// The imperative form of `Added`/`Adding`/`Adds` when the stem is a known verb.
fn imperative_of(word: &str) -> Option<String> {
    let lower = word.to_lowercase();
    let verb = if VERBS.contains(&lower.as_str()) {
        lower
    } else {
        ["ing", "ed", "es", "s", "d"]
            .iter()
            .filter_map(|suffix| lower.strip_suffix(suffix))
            .find_map(|stem| {
                let with_e = format!("{stem}e");
                if VERBS.contains(&stem) {
                    Some(stem.to_owned())
                } else if VERBS.contains(&with_e.as_str()) {
                    Some(with_e)
                } else {
                    None
                }
            })?
    };
    let mut chars = verb.chars();
    chars
        .next()
        .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
}

/// Imperative English subject, no conventional prefixes, no vague words, short.
pub(crate) fn check_commit_message(message: &str) -> Check {
    let subject = message.lines().next().unwrap_or("").trim();
    if subject.is_empty() {
        return Check::from(100, vec!["Write a commit message.".into()]);
    }
    let mut d = 0;
    let mut notes = Vec::new();
    let lower = subject.to_lowercase();
    let prefix = PREFIXES.iter().find(|p| lower.starts_with(**p));
    if let Some(prefix) = prefix {
        d += 20;
        notes.push(format!(
            "Drop the `{prefix}` prefix; start with a verb like Add or Fix."
        ));
    }
    let rest = prefix.map_or(subject, |p| subject[p.len()..].trim());
    let words: Vec<&str> = rest.split_whitespace().collect();
    match words.first() {
        Some(first) if words.len() > 1 || !VAGUE.contains(&first.to_lowercase().as_str()) => {
            if let Some(verb) = imperative_of(first)
                && !first.eq_ignore_ascii_case(&verb)
            {
                d += 20;
                notes.push(format!(
                    "Use the imperative mood: `{verb}` instead of `{first}`."
                ));
            }
            if prefix.is_none() && !first.starts_with(char::is_uppercase) {
                d += 5;
                notes.push("Start the subject with a capital letter.".into());
            }
        }
        _ => {
            d += 60;
            notes
                .push("Say what changed, for example `Fix division by zero in calculator`.".into());
        }
    }
    let translated: Vec<String> = words
        .iter()
        .filter_map(|w| {
            let bare = w.trim_matches(|c: char| !c.is_alphanumeric());
            spanish(bare).map(|en| format!("{bare} -> {en}"))
        })
        .collect();
    if !translated.is_empty() {
        d += 25;
        notes.push(format!("Write it in English ({}).", translated.join(", ")));
    }
    if subject.chars().count() > 72 {
        d += 10;
        notes.push("Keep the subject under 72 characters.".into());
    }
    if subject.ends_with('.') {
        d += 5;
        notes.push("Leave out the trailing period.".into());
    }
    Check::from(d, notes)
}

/// Names the student declared: functions, classes, variables and parameters.
pub(crate) fn declared_names(source: &str, lang: Lang) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut push = |n: &str| {
        let n = n.trim();
        if !n.is_empty()
            && n != "self"
            && n.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !names.iter().any(|x| x == n)
        {
            names.push(n.to_owned());
        }
    };
    for line in source.lines().map(str::trim) {
        if line.starts_with(['#', '/', '-']) {
            continue;
        }
        let decl = [
            "export function ",
            "function ",
            "def ",
            "class ",
            "const ",
            "let ",
        ]
        .iter()
        .find_map(|k| line.strip_prefix(k));
        if let Some(rest) = decl {
            let cut = rest.find(['(', ' ', ':', '=', '<']).unwrap_or(rest.len());
            let (name, tail) = rest.split_at(cut);
            push(name);
            if let Some(params) = tail.strip_prefix('(').and_then(|p| p.split(')').next()) {
                for param in params.split(',') {
                    push(param.split([':', '=']).next().unwrap_or(""));
                }
            }
        } else if lang == Lang::Python
            && let Some((left, right)) = line.split_once('=')
            && !right.starts_with('=')
            && !left.contains(['(', '[', '.', '!', '<', '>', '+', '-', '*', '/'])
        {
            push(left);
        }
    }
    names
}

fn words_of(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for c in name.chars() {
        if c == '_' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
        } else if c.is_uppercase() && !current.is_empty() {
            words.push(std::mem::take(&mut current));
            current.push(c.to_ascii_lowercase());
        } else {
            current.push(c.to_ascii_lowercase());
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn camel(words: &[String]) -> String {
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        let mut chars = w.chars();
        match chars.next() {
            Some(first) if i > 0 => {
                out.push(first.to_ascii_uppercase());
                out.push_str(chars.as_str());
            }
            _ => out.push_str(w),
        }
    }
    out
}

/// Flags Spanish or Spanglish names and casing that does not match the language.
pub(crate) fn check_identifiers(source: &str, lang: Lang) -> Check {
    let mut d = 0;
    let mut notes = Vec::new();
    for name in declared_names(source, lang) {
        let words = words_of(&name);
        if words.iter().any(|w| spanish(w).is_some()) {
            let english: Vec<String> = words
                .iter()
                .map(|w| spanish(w).unwrap_or(w).to_owned())
                .collect();
            let suggestion = match lang {
                Lang::TypeScript => camel(&english),
                Lang::Python | Lang::Sql => english.join("_"),
            };
            d += 15;
            notes.push(format!("Name `{name}` in English: `{suggestion}`."));
        } else if lang == Lang::Python
            && name.starts_with(char::is_lowercase)
            && name.chars().any(char::is_uppercase)
        {
            d += 10;
            notes.push(format!(
                "Python uses snake_case for functions and variables: `{}`.",
                words.join("_")
            ));
        }
    }
    Check::from(d.min(100), notes)
}

/// Standard layout: src/, tests/, .gitignore and README.md.
pub(crate) fn check_structure(root: &std::path::Path) -> Check {
    let mut d = 0;
    let mut notes = Vec::new();
    for (entry, why) in [
        ("src", "Keep code in src/."),
        ("tests", "Keep tests in tests/."),
        (
            ".gitignore",
            "Add a .gitignore so caches and .env never reach git.",
        ),
        (
            "README.md",
            "Add a short README.md that says what the project does.",
        ),
    ] {
        if !root.join(entry).exists() {
            d += 25;
            notes.push(why.into());
        }
    }
    Check::from(d, notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_names() {
        assert_eq!(check_branch("learn/day-01-variables").score, 100);
        assert_eq!(check_branch("feat/todo-cli").score, 100);
        assert_eq!(check_branch("main").score, 0);
        assert_eq!(check_branch("").score, 0);
        assert_eq!(check_branch("mi-rama").score, 60);
        assert_eq!(check_branch("stuff/Day One").score, 45);
    }

    #[test]
    fn good_commit_messages_score_full() {
        for msg in [
            "Add input validation for user age",
            "Fix division by zero in calculator",
            "Handle empty lists in word_count",
        ] {
            let c = check_commit_message(msg);
            assert_eq!(c.score, 100, "{msg}: {:?}", c.notes);
        }
    }

    #[test]
    fn vague_and_prefixed_messages_lose_points() {
        assert_eq!(check_commit_message("update").score, 40);
        assert_eq!(check_commit_message("").score, 0);
        let c = check_commit_message("feat: add login");
        assert_eq!(c.score, 80);
        assert!(c.notes[0].contains("feat:"));
    }

    #[test]
    fn past_tense_is_corrected_to_imperative() {
        let c = check_commit_message("Added tests for parser");
        assert_eq!(c.score, 80);
        assert!(c.notes[0].contains("`Add`"), "{:?}", c.notes);
        let c = check_commit_message("Fixing the crash on empty input");
        assert!(c.notes[0].contains("`Fix`"), "{:?}", c.notes);
        let c = check_commit_message("Validated the age");
        assert!(c.notes[0].contains("`Validate`"), "{:?}", c.notes);
    }

    #[test]
    fn spanish_commit_is_translated() {
        let c = check_commit_message("Agrego funcion para calcular promedio");
        assert!(c.score <= 75);
        let english = c.notes.iter().find(|n| n.contains("English")).unwrap();
        assert!(english.contains("funcion -> function"));
        assert!(english.contains("promedio -> average"));
    }

    #[test]
    fn long_subject_and_trailing_period() {
        let long = format!("Add {}", "very ".repeat(20));
        assert_eq!(check_commit_message(&long).score, 90);
        assert_eq!(check_commit_message("Add parser.").score, 95);
    }

    #[test]
    fn finds_declared_names() {
        let src = "def calcular_promedio(numeros, total=0):\n    suma = 0\n    return suma\n";
        assert_eq!(
            declared_names(src, Lang::Python),
            vec!["calcular_promedio", "numeros", "total", "suma"]
        );
        let ts =
            "export function formatPrice(cents: number): string {\n  const label = \"$\";\n}\n";
        assert_eq!(
            declared_names(ts, Lang::TypeScript),
            vec!["formatPrice", "cents", "label"]
        );
    }

    #[test]
    fn spanish_identifiers_get_english_suggestions() {
        let c = check_identifiers("def calcular_promedio(numeros):\n    pass\n", Lang::Python);
        assert!(
            c.notes.iter().any(|n| n.contains("`calculate_average`")),
            "{:?}",
            c.notes
        );
        assert!(c.notes.iter().any(|n| n.contains("`numbers`")));
        let ts = check_identifiers("function obtenerUsuario(id: number) {}\n", Lang::TypeScript);
        assert!(ts.notes[0].contains("`getUser`"), "{:?}", ts.notes);
        assert_eq!(
            check_identifiers("def word_count(text):\n", Lang::Python).score,
            100
        );
    }

    #[test]
    fn python_camel_case_is_flagged() {
        let c = check_identifiers("def wordCount(text):\n", Lang::Python);
        assert_eq!(c.score, 90);
        assert!(c.notes[0].contains("`word_count`"));
    }

    #[test]
    fn structure_checks_the_four_entries() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(check_structure(dir.path()).score, 0);
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::create_dir(dir.path().join("tests")).unwrap();
        std::fs::write(dir.path().join(".gitignore"), "").unwrap();
        assert_eq!(check_structure(dir.path()).score, 75);
        std::fs::write(dir.path().join("README.md"), "").unwrap();
        assert_eq!(check_structure(dir.path()).score, 100);
    }
}
