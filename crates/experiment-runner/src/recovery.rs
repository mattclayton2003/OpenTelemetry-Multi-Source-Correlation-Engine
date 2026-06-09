use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub enum Signal {
    Health,
    LoadGen5xx,
    PromErrorRate,
}

/// Tracks recovery across a set of health signals. Recovery is declared only
/// once *every required* signal has been continuously clear for `grace`.
///
/// The required set is explicit so that a signal which is unavailable in a
/// given environment (e.g. no load generator producing a stats file) can be
/// excluded rather than silently treated as "clear" — which would let the
/// runner declare a false recovery.
pub struct SignalStateMachine {
    grace: Duration,
    required: BTreeSet<Signal>,
    cleared_at: BTreeMap<Signal, i64>,
}

impl SignalStateMachine {
    pub fn new(grace: Duration, required: BTreeSet<Signal>) -> Self {
        Self {
            grace,
            required,
            cleared_at: BTreeMap::new(),
        }
    }

    pub fn observe(&mut self, sig: Signal, ts_ns: i64, ok: bool) {
        if ok {
            self.cleared_at.entry(sig).or_insert(ts_ns);
        } else {
            self.cleared_at.remove(&sig);
        }
    }

    pub fn recovery_ts_if_held(&self, now_ns: i64) -> Option<i64> {
        // Every required signal must currently be cleared.
        if !self
            .required
            .iter()
            .all(|s| self.cleared_at.contains_key(s))
        {
            return None;
        }
        // Recovery timestamp = the latest clear time among the required signals.
        let last = self
            .required
            .iter()
            .filter_map(|s| self.cleared_at.get(s))
            .copied()
            .max()?;
        if now_ns - last >= self.grace.as_nanos() as i64 {
            Some(last)
        } else {
            None
        }
    }
}
