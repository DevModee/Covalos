//! Learning tracks, lessons and exercises. Every exercise ships with a test harness that
//! prints one `TOTAL n` line followed by `PASS name` or `FAIL name: detail` lines, so
//! grading reads real results instead of trusting the tutor.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TrackId {
    Python,
    TypeScript,
    Sql,
}

impl TrackId {
    pub(crate) const ALL: [Self; 3] = [Self::Python, Self::TypeScript, Self::Sql];

    /// Stable id used on disk and in the cloud rows.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::Sql => "sql",
        }
    }

    pub(crate) fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.key() == key)
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Python => "Python from scratch",
            Self::TypeScript => "TypeScript",
            Self::Sql => "SQL",
        }
    }

    pub(crate) fn language(self) -> Lang {
        match self {
            Self::Python => Lang::Python,
            Self::TypeScript => Lang::TypeScript,
            Self::Sql => Lang::Sql,
        }
    }

    pub(crate) fn lessons(self) -> &'static [Lesson] {
        match self {
            Self::Python => PYTHON,
            Self::TypeScript => TYPESCRIPT,
            Self::Sql => SQL,
        }
    }

    pub(crate) fn lesson(self, id: &str) -> Option<&'static Lesson> {
        self.lessons().iter().find(|l| l.id == id)
    }
}

/// Language of the exercise files, which decides how tests run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lang {
    Python,
    TypeScript,
    Sql,
}

impl Lang {
    /// Name used in code fences and prompts.
    pub(crate) fn fence(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::TypeScript => "ts",
            Self::Sql => "sql",
        }
    }
}

pub(crate) struct Exercise {
    pub(crate) file: &'static str,
    pub(crate) test_file: &'static str,
    pub(crate) brief: &'static str,
    pub(crate) starter: &'static str,
    /// Harness body; the shared helpers are prepended when the file is written.
    pub(crate) tests: &'static str,
    /// Definitions the tutor must never show in full (checked in its replies).
    pub(crate) guarded: &'static [&'static str],
}

pub(crate) struct Lesson {
    pub(crate) id: &'static str,
    pub(crate) day: u8,
    pub(crate) title: &'static str,
    pub(crate) concepts: &'static [&'static str],
    /// Branch the student creates before starting, as a team would.
    pub(crate) branch: &'static str,
    pub(crate) exercise: Exercise,
    /// Smaller step offered after two scores under the pass mark.
    pub(crate) bridge: Exercise,
}

/// Full test file contents, including the helpers each language needs.
pub(crate) fn test_file_contents(lang: Lang, exercise: &Exercise) -> String {
    match lang {
        Lang::Python => format!("{PY_HARNESS}\n{}", exercise.tests),
        Lang::Sql => format!("{PY_HARNESS}\n{SQL_FIXTURE}\n{}", exercise.tests),
        Lang::TypeScript => format!("{TS_HARNESS}\n{}", exercise.tests),
    }
}

const PY_HARNESS: &str = r#"import importlib.util
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]


def load(rel):
    spec = importlib.util.spec_from_file_location("exercise", ROOT / rel)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def check(name, fn):
    try:
        fn()
        print(f"PASS {name}")
    except AssertionError as error:
        print(f"FAIL {name}: {error}")
    except Exception as error:
        print(f"FAIL {name}: {type(error).__name__}: {error}")


def assert_eq(actual, expected):
    assert actual == expected, f"expected {expected!r}, got {actual!r}"


def assert_raises(kind, fn):
    try:
        fn()
    except kind:
        return
    raise AssertionError(f"expected {kind.__name__}")
"#;

const TS_HARNESS: &str = r#"async function check(name: string, fn: () => unknown) {
  try {
    await fn();
    console.log(`PASS ${name}`);
  } catch (error) {
    console.log(`FAIL ${name}: ${error instanceof Error ? error.message : String(error)}`);
  }
}
function equal(actual: unknown, expected: unknown) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a !== e) throw new Error(`expected ${e}, got ${a}`);
}
"#;

/// Shared SQL fixture; the student's query file is read and run against it.
const SQL_FIXTURE: &str = r#"import sqlite3


def run_query(rel):
    text = (ROOT / rel).read_text()
    lines = [line for line in text.splitlines() if not line.strip().startswith("--")]
    query = "\n".join(lines).strip().rstrip(";")
    conn = sqlite3.connect(":memory:")
    conn.executescript("""
        CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER);
        INSERT INTO users (name, age) VALUES ('Ana', 34), ('Leo', 22), ('Bea', 41), ('Tom', 30);
        CREATE TABLE orders (id INTEGER PRIMARY KEY, user_id INTEGER, total INTEGER);
        INSERT INTO orders (user_id, total) VALUES (1, 20), (1, 30), (3, 15), (2, 5);
    """)
    return conn.execute(query).fetchall()
"#;

const PYTHON: &[Lesson] = &[
    Lesson {
        id: "py-01-basics",
        day: 1,
        title: "Variables, types and conditionals",
        concepts: &["variables", "int and str", "if / elif / else"],
        branch: "learn/day-01-conditionals",
        exercise: Exercise {
            file: "src/day01/age.py",
            test_file: "tests/day01/test_age.py",
            brief: "Write classify_age(age) that returns \"invalid\" for negative ages, \"child\" under 13, \"teen\" from 13 to 17 and \"adult\" from 18.",
            starter: "def classify_age(age):\n    # Return \"invalid\", \"child\", \"teen\" or \"adult\".\n    pass\n",
            tests: r#"print("TOTAL 5")
try:
    m = load("src/day01/age.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
check("negative age is invalid", lambda: assert_eq(m.classify_age(-1), "invalid"))
check("child under 13", lambda: assert_eq(m.classify_age(12), "child"))
check("13 is a teen", lambda: assert_eq(m.classify_age(13), "teen"))
check("17 is a teen", lambda: assert_eq(m.classify_age(17), "teen"))
check("18 is an adult", lambda: assert_eq(m.classify_age(18), "adult"))
"#,
            guarded: &["def classify_age"],
        },
        bridge: Exercise {
            file: "src/day01/adult.py",
            test_file: "tests/day01/test_adult.py",
            brief: "Write is_adult(age) that returns True when age is 18 or more and False otherwise.",
            starter: "def is_adult(age):\n    pass\n",
            tests: r#"print("TOTAL 3")
try:
    m = load("src/day01/adult.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
check("17 is not adult", lambda: assert_eq(m.is_adult(17), False))
check("18 is adult", lambda: assert_eq(m.is_adult(18), True))
check("40 is adult", lambda: assert_eq(m.is_adult(40), True))
"#,
            guarded: &["def is_adult"],
        },
    },
    Lesson {
        id: "py-02-collections",
        day: 2,
        title: "Lists, dictionaries and loops",
        concepts: &["list", "dict", "for", "while"],
        branch: "learn/day-02-loops",
        exercise: Exercise {
            file: "src/day02/words.py",
            test_file: "tests/day02/test_words.py",
            brief: "Write word_count(text) that returns a dict mapping each lowercase word to how many times it appears. Words are separated by spaces.",
            starter: "def word_count(text):\n    counts = {}\n    return counts\n",
            tests: r#"print("TOTAL 4")
try:
    m = load("src/day02/words.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
check("empty text", lambda: assert_eq(m.word_count(""), {}))
check("single word", lambda: assert_eq(m.word_count("hi"), {"hi": 1}))
check("repeated words", lambda: assert_eq(m.word_count("a b a"), {"a": 2, "b": 1}))
check("case is ignored", lambda: assert_eq(m.word_count("Go go"), {"go": 2}))
"#,
            guarded: &["def word_count"],
        },
        bridge: Exercise {
            file: "src/day02/even.py",
            test_file: "tests/day02/test_even.py",
            brief: "Write count_even(numbers) that returns how many numbers in the list are even.",
            starter: "def count_even(numbers):\n    pass\n",
            tests: r#"print("TOTAL 3")
try:
    m = load("src/day02/even.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
check("empty list", lambda: assert_eq(m.count_even([]), 0))
check("mixed list", lambda: assert_eq(m.count_even([1, 2, 3, 4]), 2))
check("negative evens", lambda: assert_eq(m.count_even([-2, 5]), 1))
"#,
            guarded: &["def count_even"],
        },
    },
    Lesson {
        id: "py-03-functions",
        day: 3,
        title: "Functions, return values and errors",
        concepts: &["parameters", "return", "try / except"],
        branch: "learn/day-03-errors",
        exercise: Exercise {
            file: "src/day03/safe.py",
            test_file: "tests/day03/test_safe.py",
            brief: "Write safe_divide(a, b) that returns None when b is 0, and parse_int(text, default) that returns default when text is not a whole number.",
            starter: "def safe_divide(a, b):\n    pass\n\n\ndef parse_int(text, default):\n    pass\n",
            tests: r#"print("TOTAL 4")
try:
    m = load("src/day03/safe.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
check("divides", lambda: assert_eq(m.safe_divide(10, 4), 2.5))
check("division by zero", lambda: assert_eq(m.safe_divide(1, 0), None))
check("parses numbers", lambda: assert_eq(m.parse_int("42", 0), 42))
check("falls back on bad input", lambda: assert_eq(m.parse_int("4x", -1), -1))
"#,
            guarded: &["def safe_divide", "def parse_int"],
        },
        bridge: Exercise {
            file: "src/day03/parse.py",
            test_file: "tests/day03/test_parse.py",
            brief: "Write parse_int(text) that returns the whole number in text, or 0 when it is not a number.",
            starter: "def parse_int(text):\n    pass\n",
            tests: r#"print("TOTAL 2")
try:
    m = load("src/day03/parse.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
check("parses", lambda: assert_eq(m.parse_int("7"), 7))
check("bad input", lambda: assert_eq(m.parse_int("seven"), 0))
"#,
            guarded: &["def parse_int"],
        },
    },
    Lesson {
        id: "py-04-files",
        day: 4,
        title: "Files, JSON and modules",
        concepts: &["open / with", "json", "import"],
        branch: "learn/day-04-files",
        exercise: Exercise {
            file: "src/day04/scores.py",
            test_file: "tests/day04/test_scores.py",
            brief: "Write save_scores(path, scores) that stores a dict as JSON, and load_scores(path) that reads it back or returns {} when the file does not exist.",
            starter: "import json\n\n\ndef save_scores(path, scores):\n    pass\n\n\ndef load_scores(path):\n    pass\n",
            tests: r#"import tempfile
print("TOTAL 3")
try:
    m = load("src/day04/scores.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
folder = pathlib.Path(tempfile.mkdtemp())
check("missing file gives empty dict", lambda: assert_eq(m.load_scores(folder / "none.json"), {}))
def round_trip():
    m.save_scores(folder / "s.json", {"ana": 90})
    assert_eq(m.load_scores(folder / "s.json"), {"ana": 90})
check("round trip", round_trip)
def overwrite():
    m.save_scores(folder / "s.json", {"leo": 70})
    assert_eq(m.load_scores(folder / "s.json"), {"leo": 70})
check("overwrites", overwrite)
"#,
            guarded: &["def save_scores", "def load_scores"],
        },
        bridge: Exercise {
            file: "src/day04/lines.py",
            test_file: "tests/day04/test_lines.py",
            brief: "Write read_lines(path) that returns the lines of a text file without trailing newlines.",
            starter: "def read_lines(path):\n    pass\n",
            tests: r#"import tempfile
print("TOTAL 1")
try:
    m = load("src/day04/lines.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
def reads():
    path = pathlib.Path(tempfile.mkdtemp()) / "a.txt"
    path.write_text("one\ntwo\n")
    assert_eq(m.read_lines(path), ["one", "two"])
check("reads lines", reads)
"#,
            guarded: &["def read_lines"],
        },
    },
    Lesson {
        id: "py-05-classes",
        day: 5,
        title: "Classes and objects",
        concepts: &["class", "__init__", "methods", "raise"],
        branch: "learn/day-05-classes",
        exercise: Exercise {
            file: "src/day05/account.py",
            test_file: "tests/day05/test_account.py",
            brief: "Write a BankAccount class with an owner, a balance starting at 0, deposit(amount) and withdraw(amount). Both raise ValueError for amounts <= 0, and withdraw raises ValueError when funds are not enough.",
            starter: "class BankAccount:\n    def __init__(self, owner):\n        pass\n",
            tests: r#"print("TOTAL 4")
try:
    m = load("src/day05/account.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
check("starts at zero", lambda: assert_eq(m.BankAccount("ana").balance, 0))
def deposit():
    a = m.BankAccount("ana")
    a.deposit(50)
    assert_eq(a.balance, 50)
check("deposit", deposit)
def no_overdraft():
    a = m.BankAccount("ana")
    assert_raises(ValueError, lambda: a.withdraw(1))
check("no overdraft", no_overdraft)
def rejects_negative():
    a = m.BankAccount("ana")
    assert_raises(ValueError, lambda: a.deposit(-5))
check("rejects negative deposit", rejects_negative)
"#,
            guarded: &["class BankAccount"],
        },
        bridge: Exercise {
            file: "src/day05/counter.py",
            test_file: "tests/day05/test_counter.py",
            brief: "Write a Counter class with a value starting at 0 and an increment() method that adds 1.",
            starter: "class Counter:\n    pass\n",
            tests: r#"print("TOTAL 2")
try:
    m = load("src/day05/counter.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
check("starts at zero", lambda: assert_eq(m.Counter().value, 0))
def increments():
    c = m.Counter()
    c.increment()
    c.increment()
    assert_eq(c.value, 2)
check("increments", increments)
"#,
            guarded: &["class Counter"],
        },
    },
    Lesson {
        id: "py-06-sqlite",
        day: 6,
        title: "SQLite with sqlite3",
        concepts: &["connect", "CREATE TABLE", "query parameters", "fetchall"],
        branch: "learn/day-06-sqlite",
        exercise: Exercise {
            file: "src/day06/tasks.py",
            test_file: "tests/day06/test_tasks.py",
            brief: "Using a sqlite3 connection: create_table(conn) creates a tasks table (id, title), add_task(conn, title) inserts with a query parameter and returns the new id, list_tasks(conn) returns the titles in insertion order.",
            starter: "import sqlite3\n\n\ndef create_table(conn):\n    pass\n\n\ndef add_task(conn, title):\n    pass\n\n\ndef list_tasks(conn):\n    pass\n",
            tests: r#"import sqlite3
print("TOTAL 3")
try:
    m = load("src/day06/tasks.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
def fresh():
    conn = sqlite3.connect(":memory:")
    m.create_table(conn)
    return conn
check("empty table", lambda: assert_eq(m.list_tasks(fresh()), []))
def ids():
    conn = fresh()
    first = m.add_task(conn, "read")
    second = m.add_task(conn, "write")
    assert second > first, "ids should grow"
check("returns ids", ids)
def quotes():
    conn = fresh()
    m.add_task(conn, "it's fine")
    assert_eq(m.list_tasks(conn), ["it's fine"])
check("handles quotes safely", quotes)
"#,
            guarded: &["def add_task", "def list_tasks"],
        },
        bridge: Exercise {
            file: "src/day06/count.py",
            test_file: "tests/day06/test_count.py",
            brief: "Write count_people(conn) that returns how many rows the people table has.",
            starter: "def count_people(conn):\n    pass\n",
            tests: r#"import sqlite3
print("TOTAL 1")
try:
    m = load("src/day06/count.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
def counts():
    conn = sqlite3.connect(":memory:")
    conn.execute("CREATE TABLE people (name TEXT)")
    conn.executemany("INSERT INTO people VALUES (?)", [("a",), ("b",)])
    assert_eq(m.count_people(conn), 2)
check("counts rows", counts)
"#,
            guarded: &["def count_people"],
        },
    },
    Lesson {
        id: "py-07-project",
        day: 7,
        title: "Guided project with tests",
        concepts: &["project layout", "writing tests", "small commits"],
        branch: "feat/todo-list",
        exercise: Exercise {
            file: "src/day07/todo.py",
            test_file: "tests/day07/test_todo.py",
            brief: "Build a TodoList class with add(title), complete(title) and pending() returning the titles not completed. Then write at least two test functions of your own in tests/day07/test_todo_mine.py.",
            starter: "class TodoList:\n    pass\n",
            tests: r#"print("TOTAL 4")
try:
    m = load("src/day07/todo.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
def adds():
    t = m.TodoList()
    t.add("read")
    assert_eq(t.pending(), ["read"])
check("add", adds)
def completes():
    t = m.TodoList()
    t.add("read")
    t.add("write")
    t.complete("read")
    assert_eq(t.pending(), ["write"])
check("complete", completes)
check("starts empty", lambda: assert_eq(m.TodoList().pending(), []))
def own_tests():
    mine = ROOT / "tests/day07/test_todo_mine.py"
    assert mine.exists(), "add tests/day07/test_todo_mine.py"
    assert mine.read_text().count("def test_") >= 2, "write at least two test_ functions"
check("your own tests", own_tests)
"#,
            guarded: &["class TodoList"],
        },
        bridge: Exercise {
            file: "src/day07/todo_small.py",
            test_file: "tests/day07/test_todo_small.py",
            brief: "Write a TodoList class with add(title) and items() returning every title in order.",
            starter: "class TodoList:\n    pass\n",
            tests: r#"print("TOTAL 1")
try:
    m = load("src/day07/todo_small.py")
except Exception as error:
    print(f"FAIL load: {error}")
    sys.exit(0)
def adds():
    t = m.TodoList()
    t.add("a")
    t.add("b")
    assert_eq(t.items(), ["a", "b"])
check("add and list", adds)
"#,
            guarded: &["class TodoList"],
        },
    },
];

const TYPESCRIPT: &[Lesson] = &[
    Lesson {
        id: "ts-01-types",
        day: 1,
        title: "Types and functions",
        concepts: &["number", "string", "function signatures"],
        branch: "learn/day-01-types",
        exercise: Exercise {
            file: "src/day01/price.ts",
            test_file: "tests/day01/price.test.ts",
            brief: "Export formatPrice(cents: number): string that returns dollars with two decimals, like formatPrice(1250) === \"$12.50\".",
            starter: "export function formatPrice(cents: number): string {\n  return \"\";\n}\n",
            tests: r#"import { formatPrice } from "../../src/day01/price.ts";
console.log("TOTAL 3");
await check("whole dollars", () => equal(formatPrice(500), "$5.00"));
await check("cents", () => equal(formatPrice(1250), "$12.50"));
await check("zero", () => equal(formatPrice(0), "$0.00"));
"#,
            guarded: &["function formatPrice"],
        },
        bridge: Exercise {
            file: "src/day01/double.ts",
            test_file: "tests/day01/double.test.ts",
            brief: "Export double(n: number): number that returns n times two.",
            starter: "export function double(n: number): number {\n  return 0;\n}\n",
            tests: r#"import { double } from "../../src/day01/double.ts";
console.log("TOTAL 2");
await check("positive", () => equal(double(4), 8));
await check("negative", () => equal(double(-3), -6));
"#,
            guarded: &["function double"],
        },
    },
    Lesson {
        id: "ts-02-collections",
        day: 2,
        title: "Arrays and objects",
        concepts: &["arrays", "object types", "Record"],
        branch: "learn/day-02-collections",
        exercise: Exercise {
            file: "src/day02/group.ts",
            test_file: "tests/day02/group.test.ts",
            brief: "Export groupByCategory(items) that groups item names by category, keeping input order. Items look like { name: string; category: string }.",
            starter: "export type Item = { name: string; category: string };\n\nexport function groupByCategory(items: Item[]): Record<string, string[]> {\n  return {};\n}\n",
            tests: r#"import { groupByCategory } from "../../src/day02/group.ts";
console.log("TOTAL 2");
await check("empty", () => equal(groupByCategory([]), {}));
await check("groups", () =>
  equal(
    groupByCategory([
      { name: "apple", category: "fruit" },
      { name: "leek", category: "vegetable" },
      { name: "pear", category: "fruit" },
    ]),
    { fruit: ["apple", "pear"], vegetable: ["leek"] },
  ),
);
"#,
            guarded: &["function groupByCategory"],
        },
        bridge: Exercise {
            file: "src/day02/names.ts",
            test_file: "tests/day02/names.test.ts",
            brief: "Export names(items: { name: string }[]): string[] that returns only the names.",
            starter: "export function names(items: { name: string }[]): string[] {\n  return [];\n}\n",
            tests: r#"import { names } from "../../src/day02/names.ts";
console.log("TOTAL 1");
await check("maps names", () => equal(names([{ name: "a" }, { name: "b" }]), ["a", "b"]));
"#,
            guarded: &["function names"],
        },
    },
    Lesson {
        id: "ts-03-unions",
        day: 3,
        title: "Unions and error handling",
        concepts: &["union types", "narrowing", "result objects"],
        branch: "learn/day-03-unions",
        exercise: Exercise {
            file: "src/day03/age.ts",
            test_file: "tests/day03/age.test.ts",
            brief: "Export parseAge(text: string) returning { ok: true, value } for whole numbers from 0 to 150 and { ok: false, error } otherwise.",
            starter: "export type ParseResult = { ok: true; value: number } | { ok: false; error: string };\n\nexport function parseAge(text: string): ParseResult {\n  return { ok: false, error: \"not implemented\" };\n}\n",
            tests: r#"import { parseAge } from "../../src/day03/age.ts";
console.log("TOTAL 3");
await check("valid", () => equal(parseAge("30"), { ok: true, value: 30 }));
await check("not a number", () => equal(parseAge("abc").ok, false));
await check("out of range", () => equal(parseAge("200").ok, false));
"#,
            guarded: &["function parseAge"],
        },
        bridge: Exercise {
            file: "src/day03/positive.ts",
            test_file: "tests/day03/positive.test.ts",
            brief: "Export isPositive(n: number): boolean.",
            starter: "export function isPositive(n: number): boolean {\n  return false;\n}\n",
            tests: r#"import { isPositive } from "../../src/day03/positive.ts";
console.log("TOTAL 2");
await check("positive", () => equal(isPositive(2), true));
await check("zero", () => equal(isPositive(0), false));
"#,
            guarded: &["function isPositive"],
        },
    },
];

const SQL: &[Lesson] = &[
    Lesson {
        id: "sql-01-select",
        day: 1,
        title: "SELECT, WHERE and ORDER BY",
        concepts: &["SELECT", "WHERE", "ORDER BY"],
        branch: "learn/day-01-select",
        exercise: Exercise {
            file: "src/day01/older.sql",
            test_file: "tests/day01/test_older.py",
            brief: "Write one query that returns the name of every user older than 30, ordered by name. Tables: users(id, name, age), orders(id, user_id, total).",
            starter: "-- Users older than 30, ordered by name.\n",
            tests: r#"print("TOTAL 1")
check("older than 30", lambda: assert_eq(run_query("src/day01/older.sql"), [("Ana",), ("Bea",)]))
"#,
            guarded: &[],
        },
        bridge: Exercise {
            file: "src/day01/all.sql",
            test_file: "tests/day01/test_all.py",
            brief: "Write one query that returns every user name ordered by name. Table: users(id, name, age).",
            starter: "-- Every user name, ordered by name.\n",
            tests: r#"print("TOTAL 1")
check("all names", lambda: assert_eq(run_query("src/day01/all.sql"), [("Ana",), ("Bea",), ("Leo",), ("Tom",)]))
"#,
            guarded: &[],
        },
    },
    Lesson {
        id: "sql-02-group",
        day: 2,
        title: "GROUP BY and aggregates",
        concepts: &["COUNT", "SUM", "GROUP BY"],
        branch: "learn/day-02-group-by",
        exercise: Exercise {
            file: "src/day02/totals.sql",
            test_file: "tests/day02/test_totals.py",
            brief: "Write one query that returns user_id and the sum of their order totals, ordered by user_id. Table: orders(id, user_id, total).",
            starter: "-- Total spent per user_id, ordered by user_id.\n",
            tests: r#"print("TOTAL 1")
check("totals", lambda: assert_eq(run_query("src/day02/totals.sql"), [(1, 50), (2, 5), (3, 15)]))
"#,
            guarded: &[],
        },
        bridge: Exercise {
            file: "src/day02/count.sql",
            test_file: "tests/day02/test_count.py",
            brief: "Write one query that returns how many orders there are. Table: orders(id, user_id, total).",
            starter: "-- Number of orders.\n",
            tests: r#"print("TOTAL 1")
check("count", lambda: assert_eq(run_query("src/day02/count.sql"), [(4,)]))
"#,
            guarded: &[],
        },
    },
    Lesson {
        id: "sql-03-join",
        day: 3,
        title: "JOIN",
        concepts: &["INNER JOIN", "aliases", "DISTINCT"],
        branch: "learn/day-03-join",
        exercise: Exercise {
            file: "src/day03/buyers.sql",
            test_file: "tests/day03/test_buyers.py",
            brief: "Write one query that returns the distinct names of users with at least one order, ordered by name. Tables: users(id, name, age), orders(id, user_id, total).",
            starter: "-- Names of users who ordered something, ordered by name.\n",
            tests: r#"print("TOTAL 1")
check("buyers", lambda: assert_eq(run_query("src/day03/buyers.sql"), [("Ana",), ("Bea",), ("Leo",)]))
"#,
            guarded: &[],
        },
        bridge: Exercise {
            file: "src/day03/big.sql",
            test_file: "tests/day03/test_big.py",
            brief: "Write one query that returns the ids of orders with a total of 20 or more, ordered by id. Table: orders(id, user_id, total).",
            starter: "-- Order ids with total >= 20.\n",
            tests: r#"print("TOTAL 1")
check("big orders", lambda: assert_eq(run_query("src/day03/big.sql"), [(1,), (2,)]))
"#,
            guarded: &[],
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_track_has_lessons_with_unique_ids_and_paths() {
        for track in TrackId::ALL {
            let lessons = track.lessons();
            assert!(!lessons.is_empty());
            let mut ids: Vec<_> = lessons.iter().map(|l| l.id).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), lessons.len(), "{track:?} lesson ids repeat");
            for lesson in lessons {
                for ex in [&lesson.exercise, &lesson.bridge] {
                    assert!(ex.file.starts_with("src/"), "{}", ex.file);
                    assert!(ex.test_file.starts_with("tests/"), "{}", ex.test_file);
                    let body = test_file_contents(track.language(), ex);
                    assert!(
                        body.contains("TOTAL "),
                        "{} lacks a TOTAL line",
                        ex.test_file
                    );
                    assert!(
                        body.contains(ex.file),
                        "{} does not load {}",
                        ex.test_file,
                        ex.file
                    );
                }
                assert_ne!(lesson.exercise.file, lesson.bridge.file);
                assert!(lesson.branch.contains('/') && !lesson.branch.contains(' '));
            }
        }
    }

    #[test]
    fn python_route_covers_the_seven_days() {
        let days: Vec<u8> = TrackId::Python.lessons().iter().map(|l| l.day).collect();
        assert_eq!(days, vec![1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn track_keys_round_trip() {
        for track in TrackId::ALL {
            assert_eq!(TrackId::from_key(track.key()), Some(track));
        }
        assert_eq!(TrackId::from_key("cobol"), None);
    }
}
