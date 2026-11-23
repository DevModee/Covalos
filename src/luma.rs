//! Luma, the lead developer. Rates how hard an order is, caps that at the
//! user's effort ceiling, and decides how to staff it: which model, which
//! reasoning effort, and whether one agent or a planned team does the work.
//! Only decisions live here; the runtime carries them out.

use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Effort {
    Low,
    #[default]
    Medium,
    High,
}

impl Effort {
    pub(crate) const ALL: [Self; 3] = [Self::Low, Self::Medium, Self::High];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }

    /// The value providers accept for their reasoning-effort option.
    pub(crate) fn flag(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

const HARD: &[&str] = &[
    "refactor",
    "architecture",
    "migrate",
    "migration",
    "redesign",
    "rewrite",
    "across",
    "entire",
    "whole",
    "end to end",
    "end-to-end",
    "from scratch",
    "multiple",
    "concurrency",
    "security",
    "performance",
    "refactoriz",
    "arquitectura",
    "migra",
    "rediseñ",
    "reescrib",
    "todo el",
    "toda la",
    "desde cero",
    "varios",
    "seguridad",
    "rendimiento",
];

const EASY: &[&str] = &[
    "typo",
    "rename",
    "explain",
    "what is",
    "what does",
    "comment",
    "format",
    "readme",
    "explica",
    "renombr",
    "qué es",
    "que hace",
    "comentario",
    "formato",
];

/// Rates an order from its wording alone: length, scope words and lists.
pub(crate) fn assess(order: &str) -> Effort {
    let text = order.to_lowercase();
    let words = text.split_whitespace().count();
    let mut score: i32 = match words {
        0..=7 => 0,
        8..=59 => 1,
        _ => 2,
    };
    let hard = HARD.iter().filter(|k| text.contains(*k)).count() as i32;
    score += (hard * 2).min(4);
    let items = text
        .lines()
        .filter(|l| {
            let l = l.trim_start();
            l.starts_with("- ") || l.starts_with("* ") || l.starts_with(char::is_numeric)
        })
        .count();
    if items >= 3 {
        score += 3;
    }
    if EASY.iter().any(|k| text.contains(k)) {
        score -= 1;
    }
    match score {
        i32::MIN..=0 => Effort::Low,
        1..=2 => Effort::Medium,
        _ => Effort::High,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Decision {
    /// What the order seems to need.
    pub assessed: Effort,
    /// What Luma will use: never above the ceiling.
    pub effort: Effort,
}

impl Decision {
    /// The order looks harder than the ceiling allows; Luma asks first.
    pub(crate) fn capped(self) -> bool {
        self.assessed > self.effort
    }

    /// High effort plans a team; anything else is one agent.
    pub(crate) fn team(self) -> bool {
        self.effort == Effort::High
    }
}

pub(crate) fn decide(order: &str, ceiling: Effort) -> Decision {
    let assessed = assess(order);
    Decision {
        assessed,
        effort: assessed.min(ceiling),
    }
}

/// A model an account offers, with the reasoning levels it accepts.
pub(crate) struct Offer<'a> {
    pub id: &'a str,
    pub efforts: &'a [String],
    pub is_default: bool,
}

fn tier_words(effort: Effort) -> &'static [&'static str] {
    match effort {
        Effort::Low => &["haiku", "mini", "flash", "lite", "nano"],
        Effort::Medium => &["sonnet"],
        Effort::High => &["opus"],
    }
}

/// Picks the model for an effort level by its family name; `None` keeps the
/// account's own default, which is also the fallback when nothing matches.
pub(crate) fn pick_model<'a>(offers: &[Offer<'a>], effort: Effort) -> Option<&'a str> {
    let words = tier_words(effort);
    offers
        .iter()
        .find(|o| {
            let id = o.id.to_lowercase();
            words.iter().any(|w| id.contains(w))
        })
        .map(|o| o.id)
}

/// The effort flag to pass, only when the model that will run lists it.
/// Unknown catalogs pass nothing, so a CLI never receives a value it rejects.
pub(crate) fn effort_flag(
    offers: &[Offer<'_>],
    model: Option<&str>,
    effort: Effort,
) -> Option<String> {
    let offer = match model {
        Some(id) => offers.iter().find(|o| o.id == id),
        None => offers.iter().find(|o| o.is_default),
    }?;
    offer
        .efforts
        .iter()
        .any(|e| e.eq_ignore_ascii_case(effort.flag()))
        .then(|| effort.flag().to_owned())
}

/// The first non-empty line of what an agent last did, cut to fit a row.
pub(crate) fn brief(text: &str, max: usize) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    if line.chars().count() <= max {
        return line.to_owned();
    }
    let cut: String = line.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_and_simple_orders_stay_low() {
        assert_eq!(assess("fix the typo in README"), Effort::Low);
        assert_eq!(assess("add a close button"), Effort::Low);
    }

    #[test]
    fn ordinary_orders_are_medium() {
        assert_eq!(
            assess("add a dark mode toggle to the settings page and remember the choice"),
            Effort::Medium
        );
    }

    #[test]
    fn broad_orders_are_high() {
        assert_eq!(
            assess("refactor the whole persistence layer and migrate the sessions format"),
            Effort::High
        );
        assert_eq!(
            assess("Do this:\n- add login\n- add signup\n- add password reset"),
            Effort::High
        );
    }

    #[test]
    fn the_ceiling_is_never_exceeded() {
        let order = "rewrite the entire renderer from scratch across every platform";
        let decision = decide(order, Effort::Medium);
        assert_eq!(decision.assessed, Effort::High);
        assert_eq!(decision.effort, Effort::Medium);
        assert!(decision.capped());
        assert!(!decision.team());
        assert_eq!(decide(order, Effort::High).effort, Effort::High);
        assert!(!decide("add a close button", Effort::High).capped());
    }

    #[test]
    fn models_follow_the_effort_and_flags_follow_the_catalog() {
        let levels = vec!["low".to_owned(), "medium".to_owned(), "high".to_owned()];
        let offers = [
            Offer {
                id: "claude-opus-4",
                efforts: &levels,
                is_default: false,
            },
            Offer {
                id: "claude-sonnet-4",
                efforts: &levels,
                is_default: true,
            },
            Offer {
                id: "claude-haiku-4",
                efforts: &[],
                is_default: false,
            },
        ];
        assert_eq!(pick_model(&offers, Effort::High), Some("claude-opus-4"));
        assert_eq!(pick_model(&offers, Effort::Low), Some("claude-haiku-4"));
        assert_eq!(pick_model(&[], Effort::Medium), None);
        assert_eq!(
            effort_flag(&offers, Some("claude-opus-4"), Effort::High).as_deref(),
            Some("high")
        );
        assert_eq!(
            effort_flag(&offers, Some("claude-haiku-4"), Effort::Low),
            None
        );
        assert_eq!(
            effort_flag(&offers, None, Effort::Medium).as_deref(),
            Some("medium")
        );
        assert_eq!(effort_flag(&[], None, Effort::Medium), None);
    }

    #[test]
    fn briefs_keep_the_first_line_and_cut_long_ones() {
        assert_eq!(brief("\n  Editing main.rs\nmore", 40), "Editing main.rs");
        assert_eq!(brief("abcdefghij", 5), "abcd…");
    }
}
