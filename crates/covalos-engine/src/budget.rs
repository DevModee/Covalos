use serde::{Deserialize, Serialize};

/// Limits for a task or a whole plan. `None` means unlimited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub wall_ms: Option<u64>,
    pub tokens: Option<u64>,
    /// Millionths of a US dollar. Only enforceable against providers that
    /// report cost themselves; see `Spend::cost_reported`.
    pub cost_micros: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spend {
    pub wall_ms: u64,
    pub tokens: u64,
    pub cost_micros: u64,
    /// False until a provider has reported cost; a cost limit cannot be
    /// trusted for work whose cost was never reported.
    pub cost_reported: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Limit {
    Wall,
    Tokens,
    Cost,
}

impl Budget {
    pub fn exceeded(&self, spend: &Spend) -> Option<Limit> {
        let over = |limit: Option<u64>, used: u64| limit.is_some_and(|l| used > l);
        if over(self.wall_ms, spend.wall_ms) {
            Some(Limit::Wall)
        } else if over(self.tokens, spend.tokens) {
            Some(Limit::Tokens)
        } else if over(self.cost_micros, spend.cost_micros) {
            Some(Limit::Cost)
        } else {
            None
        }
    }

    /// The stricter of two budgets, field by field.
    pub fn tighter(&self, other: &Self) -> Self {
        let min = |a: Option<u64>, b: Option<u64>| match (a, b) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        Self {
            wall_ms: min(self.wall_ms, other.wall_ms),
            tokens: min(self.tokens, other.tokens),
            cost_micros: min(self.cost_micros, other.cost_micros),
        }
    }
}

impl Spend {
    pub fn add(&mut self, other: &Self) {
        self.wall_ms += other.wall_ms;
        self.tokens += other.tokens;
        self.cost_micros += other.cost_micros;
        self.cost_reported |= other.cost_reported;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_and_tightening() {
        let task = Budget {
            wall_ms: Some(100),
            tokens: None,
            cost_micros: Some(50),
        };
        let plan = Budget {
            wall_ms: Some(80),
            tokens: Some(1000),
            cost_micros: None,
        };
        assert_eq!(
            task.tighter(&plan),
            Budget {
                wall_ms: Some(80),
                tokens: Some(1000),
                cost_micros: Some(50)
            }
        );
        let mut spend = Spend {
            wall_ms: 90,
            ..Spend::default()
        };
        assert_eq!(task.exceeded(&spend), None);
        spend.add(&Spend {
            wall_ms: 20,
            tokens: 5,
            cost_micros: 0,
            cost_reported: false,
        });
        assert_eq!(task.exceeded(&spend), Some(Limit::Wall));
        assert_eq!(Budget::default().exceeded(&spend), None);
    }
}
